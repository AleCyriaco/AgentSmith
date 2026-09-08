//! Drives the official `simplex-chat` client: starts it as a child process on
//! a loopback port, speaks its WebSocket API, and hands incoming messages to
//! the caller.
//!
//! Everything here is arm's length. The client is a separate program under its
//! own licence, reached over a socket, exactly as the browser-login clients
//! are reached over a pipe.
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    net::TcpStream,
    process::{Child, Command},
    sync::{mpsc, oneshot, Mutex},
};
use tokio_tungstenite::{tungstenite::Message as WsMessage, MaybeTlsStream, WebSocketStream};

const REPLY_TIMEOUT: Duration = Duration::from_secs(20);
const START_TIMEOUT: Duration = Duration::from_secs(30);

/// A message the operator sent to AgentSmith.
#[derive(Clone, Debug)]
pub struct Incoming {
    pub from: String,
    pub text: String,
}

/// Where the client binary and its data live.
pub fn binary() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let path = PathBuf::from(home).join(".local/bin/simplex-chat");
    path.is_file().then_some(path)
}

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct Client {
    child: Child,
    writer: Mutex<futures_util::stream::SplitSink<Socket, WsMessage>>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>,
    next_id: AtomicU64,
}

impl Client {
    /// Starts the client against `server` and connects to its API.
    ///
    /// `server` is the full `smp://` address, which carries a secret, so it is
    /// passed as an argument to a process this Mac starts and never logged.
    pub async fn start(
        binary: &Path,
        data: &Path,
        server: &str,
        port: u16,
        inbox: mpsc::Sender<Incoming>,
    ) -> Result<Self, String> {
        if let Some(parent) = data.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| "Não foi possível criar a pasta do SimpleX.".to_string())?;
        }
        let mut command = Command::new(binary);
        command
            .arg("-p")
            .arg(port.to_string())
            .arg("-d")
            .arg(data)
            .arg("--create-bot-display-name")
            .arg("AgentSmith")
            .args(["-y", "--mute", "--disable-backup"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if !server.trim().is_empty() {
            command.arg("-s").arg(server.trim());
        }
        let child = command
            .spawn()
            .map_err(|_| "Não foi possível iniciar o cliente SimpleX.".to_string())?;
        let socket = connect(port).await?;
        let (writer, mut reader) = socket.split();
        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>> = Default::default();
        let waiting = pending.clone();
        tokio::spawn(async move {
            while let Some(Ok(WsMessage::Text(text))) = reader.next().await {
                let Ok(value) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                let response = value.get("resp").cloned().unwrap_or(Value::Null);
                match value.get("corrId").and_then(Value::as_str) {
                    Some(id) => {
                        if let Some(sender) = waiting.lock().await.remove(id) {
                            let _ = sender.send(response);
                        }
                    }
                    // No correlation id: an event, not an answer to a command.
                    None => {
                        for message in messages(&response) {
                            if inbox.send(message).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        });
        Ok(Self {
            child,
            writer: Mutex::new(writer),
            pending,
            next_id: AtomicU64::new(0),
        })
    }

    /// Sends one command and waits for the answer that carries its id.
    pub async fn command(&self, command: &str) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst).to_string();
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), sender);
        let payload = serde_json::json!({ "corrId": id, "cmd": command }).to_string();
        self.writer
            .lock()
            .await
            .send(WsMessage::Text(payload.into()))
            .await
            .map_err(|_| "O cliente SimpleX não aceitou o comando.".to_string())?;
        match tokio::time::timeout(REPLY_TIMEOUT, receiver).await {
            Ok(Ok(value)) => Ok(value),
            _ => {
                self.pending.lock().await.remove(&id);
                Err("O cliente SimpleX não respondeu.".into())
            }
        }
    }

    pub async fn stop(mut self) {
        let _ = self.child.kill().await;
    }
}

async fn connect(port: u16) -> Result<Socket, String> {
    let address = format!("ws://127.0.0.1:{port}");
    let deadline = std::time::Instant::now() + START_TIMEOUT;
    // The client opens its port a moment after starting; retry until it does.
    loop {
        match tokio_tungstenite::connect_async(&address).await {
            Ok((socket, _)) => return Ok(socket),
            Err(_) if std::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(300)).await
            }
            Err(_) => return Err("O cliente SimpleX não abriu sua interface.".into()),
        }
    }
}

/// Pulls operator messages out of an event, ignoring everything AgentSmith
/// itself sent and every event that carries no text.
pub fn messages(event: &Value) -> Vec<Incoming> {
    if event.get("type").and_then(Value::as_str) != Some("newChatItems") {
        return vec![];
    }
    event
        .get("chatItems")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let content = item.pointer("/chatItem/content")?;
                    // Only messages received from a contact; a sent message
                    // echoes back the same way and must not be read as a reply.
                    if content.get("type").and_then(Value::as_str) != Some("rcvMsgContent") {
                        return None;
                    }
                    let text = content.pointer("/msgContent/text")?.as_str()?.to_string();
                    let from = item
                        .pointer("/chatInfo/contact/localDisplayName")
                        .and_then(Value::as_str)
                        .unwrap_or("operador")
                        .to_string();
                    (!text.trim().is_empty()).then_some(Incoming { from, text })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The contact address out of a `/show_address` or `/address` answer.
pub fn address(value: &Value) -> Option<String> {
    fn search(value: &Value) -> Option<String> {
        match value {
            Value::String(text)
                if text.starts_with("https://simplex.chat/")
                    || text.starts_with("simplex:/") =>
            {
                Some(text.clone())
            }
            Value::Array(items) => items.iter().find_map(search),
            Value::Object(fields) => fields.values().find_map(search),
            _ => None,
        }
    }
    search(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_messages_received_from_a_contact_are_read() {
        let event = json!({
            "type": "newChatItems",
            "chatItems": [
                {"chatInfo": {"contact": {"localDisplayName": "Ale"}},
                 "chatItem": {"content": {"type": "rcvMsgContent", "msgContent": {"text": "1"}}}},
                // Sent by AgentSmith: echoes back and must not count as a reply.
                {"chatInfo": {"contact": {"localDisplayName": "Ale"}},
                 "chatItem": {"content": {"type": "sndMsgContent", "msgContent": {"text": "2"}}}},
                // Received but empty.
                {"chatInfo": {"contact": {"localDisplayName": "Ale"}},
                 "chatItem": {"content": {"type": "rcvMsgContent", "msgContent": {"text": "  "}}}},
            ]
        });
        let read = messages(&event);
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].text, "1");
        assert_eq!(read[0].from, "Ale");
        // Any other event carries nothing to answer.
        assert!(messages(&json!({"type": "hostConnected"})).is_empty());
        assert!(messages(&json!({"type": "newChatItems"})).is_empty());
        assert!(messages(&Value::Null).is_empty());
    }

    #[test]
    fn the_contact_address_is_found_wherever_the_client_puts_it() {
        // The shape a real client answers `/show_address` with.
        let answer = json!({
            "type": "userContactLink",
            "user": {"userId": 1, "localDisplayName": "AgentSmith"},
            "contactLink": {"userContactLinkId": 1, "connLinkContact": {
                "connFullLink": "simplex:/contact#/?v=2-7&smp=smp%3A%2F%2Fexemplo",
                "connShortLink": null}}
        });
        assert_eq!(
            address(&answer).unwrap(),
            "simplex:/contact#/?v=2-7&smp=smp%3A%2F%2Fexemplo"
        );
        // And the shape `/address` answers with when it creates one.
        assert_eq!(
            address(&json!({"type": "userContactLinkCreated", "contactLink": {"connLinkContact": {
                "connFullLink": "https://simplex.chat/contact#/?v=2&smp=exemplo"}}}))
                .unwrap(),
            "https://simplex.chat/contact#/?v=2&smp=exemplo"
        );
        // A refusal carries no address, and the profile in the answer must not
        // be mistaken for one.
        assert!(address(&json!({"type": "chatCmdError", "chatError": {
            "type": "errorStore", "storeError": {"type": "duplicateContactLink"}}}))
            .is_none());
        assert!(address(&json!({"type": "activeUser", "user": {"localDisplayName": "AgentSmith"}}))
            .is_none());
    }
}
