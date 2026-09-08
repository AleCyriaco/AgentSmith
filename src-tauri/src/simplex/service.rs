//! The operator channel: keeps the SimpleX client running, sends notices, and
//! puts questions whose answers arrive from a phone.
use crate::operator::{self as protocol, Answer, Question};
use crate::simplex::client::{self, Client, Incoming};
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot, Mutex};

/// How long a question waits before the run is left blocked for the interface.
/// A phone may be asleep, so this is generous; the run stays paused meanwhile.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// What the interface shows about the channel.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Running and connected to the configured server.
    pub active: bool,
    /// The address to pair with, once the client has one.
    pub address: String,
    /// How many contacts can receive notices. Zero means nobody is paired yet.
    pub contacts: usize,
    pub message: String,
    pub local: bool,
    pub host: super::host::HostStatus,
    pub pairing_until: i64,
    pub qr: String,
}

struct Live {
    client: Arc<Client>,
    local: bool,
    pairing_until: i64,
    address: String,
}

pub struct Simplex {
    pub lifecycle: Mutex<()>,
    pub host: super::host::Host,
    live: Mutex<Option<Live>>,
    /// Questions waiting on a reply, by run.
    waiting: Arc<Mutex<HashMap<String, oneshot::Sender<Answer>>>>,
    contacts: Arc<Mutex<Vec<String>>>,
}

impl Simplex {
    pub fn new() -> Self {
        Self {
            live: Mutex::new(None),
            lifecycle: Mutex::new(()),
            host: Default::default(),
            waiting: Default::default(),
            contacts: Default::default(),
        }
    }

    /// The contact address, asking for the existing one before trying to
    /// create it: creating a second is an error, not a new address.
    async fn address(client: &Client) -> String {
        for command in ["/show_address", "/address"] {
            if let Some(found) = client
                .command(command)
                .await
                .ok()
                .and_then(|value| client::address(&value))
            {
                return found;
            }
        }
        String::new()
    }

    fn data_dir() -> Result<PathBuf, String> {
        let home = std::env::var_os("HOME").ok_or("Pasta pessoal indisponível.")?;
        Ok(PathBuf::from(home)
            .join("Library/Application Support/com.agentsmith.desktop/simplex")
            .join("agentsmith"))
    }

    /// Starts the channel against `server`, or reports why it cannot.
    pub async fn start(&self, server: &str, local: bool) -> Result<Status, String> {
        self.stop_client().await;
        let binary = super::install::client().await?;
        let (sender, receiver) = mpsc::channel(64);
        let data = if local {
            super::host::data_dir()?.join("local/agentsmith")
        } else {
            Self::data_dir()?
        };
        // A previous app version may have left a CLI on its fixed API port.
        // Allocate a fresh loopback port instead of attaching to another profile.
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
            .map_err(|_| "Não foi possível reservar uma conexão local para o SimpleX.")?;
        let port = listener
            .local_addr()
            .map_err(|_| "Porta local indisponível.")?
            .port();
        drop(listener);
        let client = Arc::new(Client::start(&binary, &data, server, port, sender, local).await?);
        // The client accepts the connection before it has finished creating
        // its profile, so asking straight away fails and the channel would be
        // left without an address for good.
        for _ in 0..20 {
            let ready = client.command("/u").await.ok().and_then(|value| {
                value
                    .get("type")
                    .and_then(|t| t.as_str())
                    .map(str::to_string)
            });
            if ready.as_deref() == Some("activeUser") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        let address = Self::address(&client).await;
        // Pairing survives in the client's own database, so the contacts are
        // read back rather than waited for.
        if let Ok(value) = client.command("/contacts").await {
            let known = client::contacts(&value);
            if !known.is_empty() {
                *self.contacts.lock().await = known;
            }
        }
        // Pairing is an explicit five-minute window, never enabled by startup.
        let _ = client.command("/auto_accept off").await;
        if address.is_empty() {
            client.stop().await;
            return Err("O cliente abriu, mas não conseguiu criar o contato no servidor. Verifique o Tailscale e tente novamente.".into());
        }
        self.listen(receiver);
        *self.live.lock().await = Some(Live {
            client,
            local,
            pairing_until: 0,
            address: address.clone(),
        });
        if local {
            self.host.phase("ready", "Servidor pronto neste Mac.").await;
        }
        Ok(self.status().await)
    }

    pub async fn start_local(&self) -> Result<Status, String> {
        let server = self.host.prepare().await?;
        let result = self.start(&server, true).await;
        if let Err(error) = &result {
            self.host.phase("error", error).await;
        }
        result
    }

    pub async fn pair(&self) -> Result<Status, String> {
        let mut guard = self.live.lock().await;
        let live = guard.as_mut().ok_or("Ligue o SimpleX primeiro.")?;
        if live.pairing_until > chrono::Utc::now().timestamp_millis() {
            return Err("O QR atual ainda está válido. Use-o ou aguarde sua expiração.".into());
        }
        let result = live.client.command("/auto_accept on").await?;
        if result["type"] == "chatCmdError" {
            return Err("O cliente recusou o pareamento.".into());
        }
        live.pairing_until = chrono::Utc::now().timestamp_millis() + 300_000;
        let client = live.client.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(300)).await;
            let _ = client.command("/auto_accept off").await;
        });
        drop(guard);
        Ok(self.status().await)
    }

    fn listen(&self, mut receiver: mpsc::Receiver<Incoming>) {
        let waiting = self.waiting.clone();
        let contacts = self.contacts.clone();
        tokio::spawn(async move {
            while let Some(message) = receiver.recv().await {
                {
                    let mut known = contacts.lock().await;
                    if !known.contains(&message.from) {
                        known.push(message.from.clone());
                    }
                }
                let Some(answer) = protocol::read_answer(&message.text) else {
                    continue;
                };
                // The oldest unanswered question is the one being replied to;
                // AgentSmith runs one task at a time, so there is normally one.
                let key = waiting.lock().await.keys().next().cloned();
                if let Some(key) = key {
                    if let Some(sender) = waiting.lock().await.remove(&key) {
                        let _ = sender.send(answer);
                    }
                }
            }
        });
    }

    async fn stop_client(&self) {
        if let Some(live) = self.live.lock().await.take() {
            live.client.stop().await;
        }
        self.waiting.lock().await.clear();
        self.contacts.lock().await.clear();
    }

    pub async fn stop(&self) -> Result<(), String> {
        self.stop_client().await;
        self.host.stop().await
    }
    pub async fn status(&self) -> Status {
        let mut guard = self.live.lock().await;
        let host = self.host.status().await;
        let Some(live) = guard.as_mut() else {
            return Status {
                host,
                ..Default::default()
            };
        };
        if !live.client.is_running().await {
            self.waiting.lock().await.clear();
            self.contacts.lock().await.clear();
            return Status {
                host,
                message: "O cliente SimpleX encerrou. Ative novamente.".into(),
                ..Default::default()
            };
        }
        if let Ok(value) = live.client.command("/contacts").await {
            if value["type"] == "contactsList" {
                *self.contacts.lock().await = client::contacts(&value);
            }
        }
        let pairing = live.pairing_until > chrono::Utc::now().timestamp_millis();
        Status {
            active: true,
            local: live.local,
            host,
            address: if pairing {
                live.address.clone()
            } else {
                String::new()
            },
            qr: if pairing {
                super::host::qr(&live.address).unwrap_or_default()
            } else {
                String::new()
            },
            pairing_until: if pairing { live.pairing_until } else { 0 },
            contacts: self.contacts.lock().await.len(),
            message: "Canal ativo.".into(),
        }
    }

    /// Sends a notice to every paired contact. Failure to reach the operator
    /// never fails the caller: a task must not stop because a notice did not
    /// go out.
    pub async fn notify(&self, title: &str, detail: &str) {
        let text = protocol::notice(title, detail);
        let guard = self.live.lock().await;
        let Some(live) = guard.as_ref() else { return };
        for contact in self.contacts.lock().await.iter() {
            let _ = live.client.command(&format!("@{contact} {text}")).await;
        }
    }

    pub async fn test_notice(&self) -> Result<(), String> {
        let guard = self.live.lock().await;
        let live = guard.as_ref().ok_or("Ligue o SimpleX primeiro.")?;
        let contacts = self.contacts.lock().await.clone();
        if contacts.is_empty() {
            return Err("Nenhum contato pareado ainda. Conecte o celular e envie uma mensagem ao AgentSmith.".into());
        }
        let text=protocol::notice("Aviso de teste","Se você recebeu isto, o canal está funcionando. É por aqui que chegam os avisos e os pedidos de decisão.");
        for contact in contacts {
            let result = live.client.command(&format!("@{contact} {text}")).await?;
            if result["type"] == "chatCmdError" {
                return Err(
                    "O SimpleX não aceitou o aviso de teste. Confira a conexão do servidor.".into(),
                );
            }
        }
        Ok(())
    }

    /// Puts a question and waits for the operator. `None` means nobody
    /// answered in time, or there was nobody to ask.
    pub async fn ask(&self, question: &Question) -> Option<Answer> {
        let (sender, receiver) = oneshot::channel();
        {
            let guard = self.live.lock().await;
            let live = guard.as_ref()?;
            let contacts = self.contacts.lock().await.clone();
            if contacts.is_empty() {
                return None;
            }
            self.waiting
                .lock()
                .await
                .insert(question.run_id.clone(), sender);
            let text = question.message();
            for contact in &contacts {
                let _ = live.client.command(&format!("@{contact} {text}")).await;
            }
        }
        let answer = tokio::time::timeout(ANSWER_TIMEOUT, receiver).await;
        self.waiting.lock().await.remove(&question.run_id);
        answer.ok().and_then(Result::ok)
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    /// Opt-in: creates the managed relay, changes its private Tailscale forwarding,
    /// and pairs two isolated official clients. Never uses the operator's contacts.
    #[tokio::test]
    #[ignore = "requires Podman, Tailscale and network access"]
    async fn local_relay_exchanges_a_message_between_official_clients() {
        let host = super::super::host::Host::default();
        let server = host.prepare().await.expect("prepare managed relay");
        let root =
            std::env::temp_dir().join(format!("agentsmith-simplex-test-{}", uuid::Uuid::new_v4()));
        let binary = super::super::install::client().await.unwrap();
        let (a_tx, mut a_rx) = mpsc::channel(64);
        let (b_tx, _b_rx) = mpsc::channel(64);
        let a = Client::start(&binary, &root.join("a"), &server, 15225, a_tx, true)
            .await
            .unwrap();
        let b = Client::start(&binary, &root.join("b"), &server, 15226, b_tx, true)
            .await
            .unwrap();
        for c in [&a, &b] {
            for _ in 0..30 {
                if c.command("/u").await.unwrap()["type"] == "activeUser" {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
        let address = Simplex::address(&a).await;
        assert!(
            !address.is_empty(),
            "contact address must exist on private relay"
        );
        a.command("/auto_accept on").await.unwrap();
        let result = b.command(&format!("/c {address}")).await.unwrap();
        assert_ne!(result["type"], "chatCmdError", "contact request failed");
        let mut contact = None;
        for _ in 0..60 {
            contact = client::contacts(&b.command("/contacts").await.unwrap())
                .first()
                .cloned();
            if contact.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        let contact = contact.expect("contact pairing must complete");
        let result = b
            .command(&format!("@{contact} AgentSmith isolated relay test"))
            .await
            .unwrap();
        assert_ne!(result["type"], "chatCmdError", "message send failed");
        let received = tokio::time::timeout(Duration::from_secs(45), a_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(received.text, "AgentSmith isolated relay test");
        a.stop().await;
        b.stop().await;
        host.stop().await.unwrap();
        let _ = std::fs::remove_dir_all(root);
    }
}
