//! Reaching the operator over ntfy, on a server they run.
//!
//! Notices are published to one topic; an answer comes back on another, which
//! AgentSmith listens to. The reply travels as a link the phone opens, so
//! nothing has to listen for connections on this Mac, and it works on phones
//! where a notification cannot carry buttons at all.
use crate::operator::{self, Answer, Question};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::{oneshot, Mutex};

/// How long a question waits before the run is left blocked for the interface.
/// A phone may be asleep, so this is generous; the run stays stopped meanwhile.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const TOPIC_SUFFIX: &str = "-respostas";

/// Where notices go and where answers come back.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Base address of the ntfy server, such as `https://ntfy.exemplo.com`.
    pub server: String,
    /// Topic that receives the notices.
    pub topic: String,
}

impl Config {
    pub fn ready(&self) -> bool {
        !self.server.trim().is_empty() && !self.topic.trim().is_empty()
    }

    fn base(&self) -> String {
        self.server.trim().trim_end_matches('/').to_string()
    }

    fn topic(&self) -> String {
        self.topic.trim().trim_matches('/').to_string()
    }

    /// The topic answers are published to. Kept apart from the notice topic so
    /// that reading notices does not also grant answering them.
    pub fn reply_topic(&self) -> String {
        format!("{}{TOPIC_SUFFIX}", self.topic())
    }

    pub fn notice_url(&self) -> String {
        format!("{}/{}", self.base(), self.topic())
    }

    /// The address that answers a question, carrying its one-time ticket.
    pub fn answer_url(&self, ticket: &str, answer: Answer) -> String {
        let word = match answer {
            Answer::Continue => "continuar",
            Answer::Stop => "parar",
        };
        format!(
            "{}/{}/trigger?message={ticket}%20{word}",
            self.base(),
            self.reply_topic()
        )
    }

    /// The stream of answers, asking only for what arrives from now on.
    pub fn listen_url(&self) -> String {
        format!("{}/{}/json?since=now", self.base(), self.reply_topic())
    }
}

/// Reads a reply published to the answer topic: a ticket and a decision.
///
/// Both must be present and the ticket must match, so a message written by
/// anyone else, or an answer to a question already settled, decides nothing.
pub fn read_reply(message: &str) -> Option<(String, Answer)> {
    let mut parts = message.split_whitespace();
    let ticket = parts.next()?.to_string();
    let answer = operator::read_answer(&parts.collect::<Vec<_>>().join(" "))?;
    (ticket.len() >= 8 && ticket.chars().all(|c| c.is_ascii_hexdigit()))
        .then_some((ticket, answer))
}

struct Waiting {
    ticket: String,
    sender: oneshot::Sender<Answer>,
}

pub struct Ntfy {
    config: Mutex<Config>,
    waiting: Arc<Mutex<HashMap<String, Waiting>>>,
    listening: Mutex<Option<tokio::task::JoinHandle<()>>>,
    http: reqwest::Client,
}

impl Ntfy {
    pub fn new() -> Self {
        Self {
            config: Mutex::new(Config::default()),
            waiting: Default::default(),
            listening: Mutex::new(None),
            http: reqwest::Client::new(),
        }
    }

    pub async fn config(&self) -> Config {
        self.config.lock().await.clone()
    }

    /// Points the channel at a server and starts listening for answers.
    pub async fn start(&self, config: Config) -> Result<(), String> {
        if !config.ready() {
            return Err("Informe o endereço do servidor ntfy e o tópico.".into());
        }
        if !config.server.trim().starts_with("http") {
            return Err("O endereço do servidor deve começar com http:// ou https://.".into());
        }
        self.stop().await;
        *self.config.lock().await = config.clone();
        let waiting = self.waiting.clone();
        let http = self.http.clone();
        let url = config.listen_url();
        *self.listening.lock().await = Some(tokio::spawn(async move {
            listen(http, url, waiting).await;
        }));
        Ok(())
    }

    pub async fn stop(&self) {
        if let Some(task) = self.listening.lock().await.take() {
            task.abort();
        }
        self.waiting.lock().await.clear();
    }

    pub async fn active(&self) -> bool {
        self.listening.lock().await.is_some()
    }

    /// Publishes a notice. A notice that cannot be delivered never fails the
    /// task that produced it.
    pub async fn notify(&self, title: &str, detail: &str) {
        let config = self.config().await;
        if !config.ready() {
            return;
        }
        let _ = self
            .http
            .post(config.notice_url())
            .header("Title", header(title))
            .header("Tags", "robot")
            .body(detail.to_string())
            .timeout(Duration::from_secs(15))
            .send()
            .await;
    }

    /// Publishes a question and waits for the operator to open one of its two
    /// links. `None` means nobody answered in time.
    pub async fn ask(&self, question: &Question) -> Option<Answer> {
        let config = self.config().await;
        if !config.ready() || !self.active().await {
            return None;
        }
        let ticket = operator::ticket();
        let resume = config.answer_url(&ticket, Answer::Continue);
        let stop = config.answer_url(&ticket, Answer::Stop);
        let (sender, receiver) = oneshot::channel();
        self.waiting.lock().await.insert(
            question.run_id.clone(),
            Waiting {
                ticket: ticket.clone(),
                sender,
            },
        );
        let sent = self
            .http
            .post(config.notice_url())
            .header("Title", header(&format!("Decisão: {}", question.title)))
            .header("Priority", "high")
            .header("Tags", "warning")
            .header(
                "Actions",
                format!(
                    "http, Continuar, {resume}, method=GET, clear=true; http, Parar, {stop}, method=GET, clear=true"
                ),
            )
            .body(question.message_with_links(&resume, &stop))
            .timeout(Duration::from_secs(15))
            .send()
            .await;
        if sent.is_err() {
            self.waiting.lock().await.remove(&question.run_id);
            return None;
        }
        let answer = tokio::time::timeout(ANSWER_TIMEOUT, receiver).await;
        self.waiting.lock().await.remove(&question.run_id);
        answer.ok().and_then(Result::ok)
    }
}

/// A header carries one line: a title with a newline would end the request.
fn header(text: &str) -> String {
    text.replace(['\n', '\r'], " ").chars().take(200).collect()
}

/// Keeps the answer stream open, reconnecting for as long as the channel runs.
async fn listen(
    http: reqwest::Client,
    url: String,
    waiting: Arc<Mutex<HashMap<String, Waiting>>>,
) {
    loop {
        if let Ok(response) = http.get(&url).send().await {
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            use futures_util::StreamExt;
            while let Some(Ok(chunk)) = stream.next().await {
                buffer.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(end) = buffer.find('\n') {
                    let line: String = buffer.drain(..=end).collect();
                    deliver(line.trim(), &waiting).await;
                }
            }
        }
        // The server restarts and networks drop; a channel that stopped
        // listening would leave every later question unanswerable.
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn deliver(line: &str, waiting: &Arc<Mutex<HashMap<String, Waiting>>>) {
    if line.is_empty() {
        return;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return;
    };
    // The stream also carries connection and keepalive events, which say
    // nothing about a decision.
    if value.get("event").and_then(|e| e.as_str()) != Some("message") {
        return;
    }
    let Some(message) = value.get("message").and_then(|m| m.as_str()) else {
        return;
    };
    let Some((ticket, answer)) = read_reply(message) else {
        return;
    };
    let mut pending = waiting.lock().await;
    let Some(key) = pending
        .iter()
        .find(|(_, w)| w.ticket == ticket)
        .map(|(k, _)| k.clone())
    else {
        return;
    };
    if let Some(w) = pending.remove(&key) {
        let _ = w.sender.send(answer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            server: "https://ntfy.exemplo.com/".into(),
            topic: "agentsmith".into(),
        }
    }

    #[test]
    fn addresses_are_built_from_the_server_and_topic() {
        let c = config();
        assert_eq!(c.notice_url(), "https://ntfy.exemplo.com/agentsmith");
        // Answers land on their own topic: reading notices must not also
        // grant the ability to answer them.
        assert_eq!(c.reply_topic(), "agentsmith-respostas");
        assert!(c.listen_url().ends_with("/agentsmith-respostas/json?since=now"));
        assert_eq!(
            c.answer_url("abc123", Answer::Continue),
            "https://ntfy.exemplo.com/agentsmith-respostas/trigger?message=abc123%20continuar"
        );
        assert!(c.answer_url("abc123", Answer::Stop).ends_with("parar"));
        assert!(!Config::default().ready());
        assert!(!Config { server: "https://x".into(), topic: " ".into() }.ready());
    }

    #[test]
    fn only_a_reply_carrying_its_ticket_decides_anything() {
        assert_eq!(
            read_reply("a1b2c3d4e5f6 continuar"),
            Some(("a1b2c3d4e5f6".into(), Answer::Continue))
        );
        assert_eq!(
            read_reply("a1b2c3d4e5f6 parar"),
            Some(("a1b2c3d4e5f6".into(), Answer::Stop))
        );
        for stray in [
            "continuar",                 // sem bilhete: qualquer um poderia enviar
            "a1b2c3d4e5f6",              // sem decisão
            "zzzz continuar",            // bilhete curto demais
            "naohexadecimal! continuar", // bilhete impossível
            "a1b2c3d4e5f6 talvez",       // decisão ambígua
            "",
        ] {
            assert_eq!(read_reply(stray), None, "{stray}");
        }
    }

    #[test]
    fn a_title_never_breaks_out_of_its_header() {
        let broken = header("Tarefa\nInjected: valor\r\noutra");
        assert!(!broken.contains('\n') && !broken.contains('\r'));
        assert!(header(&"x".repeat(500)).chars().count() <= 200);
    }

    #[tokio::test]
    async fn an_answer_reaches_only_the_question_that_carries_its_ticket() {
        let waiting: Arc<Mutex<HashMap<String, Waiting>>> = Default::default();
        let (sender, receiver) = oneshot::channel();
        waiting.lock().await.insert(
            "run-1".into(),
            Waiting { ticket: "a1b2c3d4e5f6".into(), sender },
        );
        // Another question's ticket, a keepalive, and a stray message all
        // leave it waiting.
        deliver(r#"{"event":"message","message":"ffffffffffff continuar"}"#, &waiting).await;
        deliver(r#"{"event":"keepalive"}"#, &waiting).await;
        deliver(r#"{"event":"message","message":"continuar"}"#, &waiting).await;
        assert_eq!(waiting.lock().await.len(), 1);
        deliver(r#"{"event":"message","message":"a1b2c3d4e5f6 continuar"}"#, &waiting).await;
        assert_eq!(receiver.await.unwrap(), Answer::Continue);
        assert!(waiting.lock().await.is_empty());
    }
}
