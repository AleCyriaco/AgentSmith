//! The operator channel: keeps the SimpleX client running, sends notices, and
//! puts questions whose answers arrive from a phone.
use crate::simplex::{
    client::{self, Client, Incoming},
    protocol::{self, Answer, Question},
};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::sync::{mpsc, oneshot, Mutex};

/// Loopback port for the client's API. Bound to 127.0.0.1 by the client
/// itself, so it is never reachable from the network.
const PORT: u16 = 5225;
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
}

struct Live {
    client: Client,
    address: String,
}

pub struct Simplex {
    live: Mutex<Option<Live>>,
    /// Questions waiting on a reply, by run.
    waiting: Arc<Mutex<HashMap<String, oneshot::Sender<Answer>>>>,
    contacts: Arc<Mutex<Vec<String>>>,
}

impl Simplex {
    pub fn new() -> Self {
        Self {
            live: Mutex::new(None),
            waiting: Default::default(),
            contacts: Default::default(),
        }
    }

    fn data_dir() -> Result<PathBuf, String> {
        let home = std::env::var_os("HOME").ok_or("Pasta pessoal indisponível.")?;
        Ok(PathBuf::from(home)
            .join("Library/Application Support/com.agentsmith.desktop/simplex")
            .join("agentsmith"))
    }

    /// Starts the channel against `server`, or reports why it cannot.
    pub async fn start(&self, server: &str) -> Result<Status, String> {
        let binary = client::binary().ok_or(
            "Cliente SimpleX não encontrado em ~/.local/bin/simplex-chat. Instale-o para usar avisos.",
        )?;
        self.stop().await;
        let (sender, receiver) = mpsc::channel(64);
        let client = Client::start(&binary, &Self::data_dir()?, server, PORT, sender).await?;
        // The address exists after the first run; creating it again is an
        // error, so ask for the existing one first.
        let mut address = client
            .command("/show_address")
            .await
            .ok()
            .and_then(|value| client::address(&value))
            .unwrap_or_default();
        if address.is_empty() {
            address = client
                .command("/address")
                .await
                .ok()
                .and_then(|value| client::address(&value))
                .unwrap_or_default();
        }
        // Anyone holding the address is the operator; without this a pairing
        // request would wait for a click in an app that has no interface here.
        let _ = client.command("/auto_accept on").await;
        self.listen(receiver);
        *self.live.lock().await = Some(Live {
            client,
            address: address.clone(),
        });
        Ok(Status {
            active: true,
            address,
            contacts: self.contacts.lock().await.len(),
            message: "Canal ativo.".into(),
        })
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

    pub async fn stop(&self) {
        if let Some(live) = self.live.lock().await.take() {
            live.client.stop().await;
        }
        self.waiting.lock().await.clear();
    }

    pub async fn status(&self) -> Status {
        match self.live.lock().await.as_ref() {
            Some(live) => Status {
                active: true,
                address: live.address.clone(),
                contacts: self.contacts.lock().await.len(),
                message: "Canal ativo.".into(),
            },
            None => Status::default(),
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
