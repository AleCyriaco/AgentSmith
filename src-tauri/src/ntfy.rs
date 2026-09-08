//! Reaching the operator over ntfy, on a server they run.
//!
//! Notices are published to one topic; an answer comes back on another, which
//! AgentSmith listens to. The reply travels as a link the phone opens, so
//! nothing has to listen for connections on this Mac, and it works on phones
//! where a notification cannot carry buttons at all.
use crate::operator::{self, Answer, Question};
use base64::{engine::general_purpose::STANDARD, engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::{oneshot, Mutex};

/// How long a question waits before the run is left blocked for the interface.
/// A phone may be asleep, so this is generous; the run stays stopped meanwhile.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const TOPIC_SUFFIX: &str = "-respostas";
/// Keychain identity for the server password. Secret ids are UUIDs, and this
/// channel is a single fixed entity rather than one per machine.
pub const SECRET_ID: &str = "b7c04e21-3f9a-4d75-8c16-9ae2f0d5b34c";
pub const SECRET_BINDING: &str = "ntfy://password";
pub const REPLY_SECRET_BINDING: &str = "ntfy://reply-password";

/// Where notices go and where answers come back.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Base address of the ntfy server, such as `https://ntfy.exemplo.com`.
    pub server: String,
    /// Topic that receives the notices.
    pub topic: String,
    /// AgentSmith's own account: it publishes the alerts and reads the
    /// answers, so it needs write on the topic and read on the answer topic.
    #[serde(default)]
    pub user: String,
    /// Access token, used instead of a password when present.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token: String,
    /// The account whose credential travels inside the answer link.
    ///
    /// It must be a different one, able to do nothing but write to the answer
    /// topic. The phone opens that link with nothing in between, so whatever
    /// it carries is exposed; an account that could also read would hand over
    /// every question along with it.
    #[serde(default)]
    pub reply_user: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reply_token: String,
}

impl Config {
    pub fn ready(&self) -> bool {
        !self.server.trim().is_empty() && !self.topic.trim().is_empty()
    }

    /// The `Authorization` header for AgentSmith's own requests.
    ///
    /// A token stands for the whole account, so it is the credential to hand
    /// out sparingly; a password is accepted for a server that has no tokens.
    pub fn authorization(&self, password: &str) -> Option<String> {
        header_value(&self.token, &self.user, password)
    }

    /// The `Authorization` for the answer link, from the reply account alone.
    /// It never falls back to AgentSmith's own account: a link that carried
    /// that credential would let whoever read it read every question too.
    pub fn reply_authorization(&self, password: &str) -> Option<String> {
        header_value(&self.reply_token, &self.reply_user, password)
    }

    /// A credential as a query parameter, for a link a phone opens. The server
    /// reads `?auth=` as base64url without padding of the whole header value.
    fn auth_param(header: Option<String>) -> String {
        match header {
            Some(value) => format!("&auth={}", URL_SAFE_NO_PAD.encode(value)),
            None => String::new(),
        }
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

    /// The address that answers a question, carrying its one-time ticket and
    /// whatever credential the server needs to accept the write.
    pub fn answer_url(&self, ticket: &str, answer: Answer, reply_password: &str) -> String {
        let word = match answer {
            Answer::Continue => "continuar",
            Answer::Stop => "parar",
        };
        format!(
            "{}/{}/trigger?message={ticket}%20{word}{}",
            self.base(),
            self.reply_topic(),
            Self::auth_param(self.reply_authorization(reply_password))
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
    /// Held only in memory. Passwords are secrets and belong in the Keychain,
    /// not in the settings file the configuration lives in.
    password: Mutex<String>,
    reply_password: Mutex<String>,
    waiting: Arc<Mutex<HashMap<String, Waiting>>>,
    listening: Mutex<Option<tokio::task::JoinHandle<()>>>,
    http: reqwest::Client,
}

impl Ntfy {
    pub fn new() -> Self {
        Self {
            config: Mutex::new(Config::default()),
            password: Mutex::new(String::new()),
            reply_password: Mutex::new(String::new()),
            waiting: Default::default(),
            listening: Mutex::new(None),
            http: reqwest::Client::new(),
        }
    }

    pub async fn config(&self) -> Config {
        self.config.lock().await.clone()
    }

    /// Points the channel at a server and starts listening for answers.
    pub async fn start(
        &self,
        config: Config,
        password: String,
        reply_password: String,
    ) -> Result<(), String> {
        if !config.ready() {
            return Err("Informe o endereço do servidor ntfy e o tópico.".into());
        }
        if !config.server.trim().starts_with("http") {
            return Err("O endereço do servidor deve começar com http:// ou https://.".into());
        }
        self.stop().await;
        *self.config.lock().await = config.clone();
        *self.password.lock().await = password.clone();
        *self.reply_password.lock().await = reply_password;
        let waiting = self.waiting.clone();
        let http = self.http.clone();
        let url = config.listen_url();
        let authorization = config.authorization(&password);
        *self.listening.lock().await = Some(tokio::spawn(async move {
            listen(http, url, authorization, waiting).await;
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
        let password = self.password.lock().await.clone();
        let mut request = self
            .http
            .post(config.notice_url())
            .header("Title", header(title))
            .header("Tags", "robot")
            .body(detail.to_string());
        if let Some(authorization) = config.authorization(&password) {
            request = request.header("Authorization", authorization);
        }
        let _ = request
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
        let password = self.password.lock().await.clone();
        let reply_password = self.reply_password.lock().await.clone();
        let ticket = operator::ticket();
        let resume = config.answer_url(&ticket, Answer::Continue, &reply_password);
        let stop = config.answer_url(&ticket, Answer::Stop, &reply_password);
        let (sender, receiver) = oneshot::channel();
        self.waiting.lock().await.insert(
            question.run_id.clone(),
            Waiting {
                ticket: ticket.clone(),
                sender,
            },
        );
        let mut request = self
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
            .timeout(Duration::from_secs(15));
        if let Some(authorization) = config.authorization(&password) {
            request = request.header("Authorization", authorization);
        }
        let sent = request.send().await;
        if sent.is_err() {
            self.waiting.lock().await.remove(&question.run_id);
            return None;
        }
        let answer = tokio::time::timeout(ANSWER_TIMEOUT, receiver).await;
        self.waiting.lock().await.remove(&question.run_id);
        answer.ok().and_then(Result::ok)
    }
}

fn header_value(token: &str, user: &str, password: &str) -> Option<String> {
    let token = token.trim();
    if !token.is_empty() {
        return Some(format!("Bearer {token}"));
    }
    let user = user.trim();
    (!user.is_empty() && !password.is_empty())
        .then(|| format!("Basic {}", STANDARD.encode(format!("{user}:{password}"))))
}

/// A header carries one line: a title with a newline would end the request.
fn header(text: &str) -> String {
    text.replace(['\n', '\r'], " ").chars().take(200).collect()
}

/// Keeps the answer stream open, reconnecting for as long as the channel runs.
async fn listen(
    http: reqwest::Client,
    url: String,
    authorization: Option<String>,
    waiting: Arc<Mutex<HashMap<String, Waiting>>>,
) {
    loop {
        let mut request = http.get(&url);
        if let Some(value) = authorization.as_ref() {
            request = request.header("Authorization", value);
        }
        if let Ok(response) = request.send().await {
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
            user: String::new(),
            token: String::new(),
            reply_user: String::new(),
            reply_token: String::new(),
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
            c.answer_url("abc123", Answer::Continue, ""),
            "https://ntfy.exemplo.com/agentsmith-respostas/trigger?message=abc123%20continuar"
        );
        assert!(c.answer_url("abc123", Answer::Stop, "").ends_with("parar"));
        assert!(!Config::default().ready());
        assert!(!Config { server: "https://x".into(), topic: " ".into(), ..config() }.ready());
    }

    #[test]
    fn a_server_that_refuses_anonymous_writes_is_answered_with_a_credential() {
        let mut c = config();
        // No account: nothing is sent, rather than an empty header.
        assert_eq!(c.authorization(""), None);
        assert_eq!(Config::auth_param(c.authorization("segredo")), "");
        c.user = "ale".into();
        // Basic, exactly as the server reads it from the header.
        assert_eq!(c.authorization("segredo").unwrap(), "Basic YWxlOnNlZ3JlZG8=");
        // A token stands for the account and wins over a password.
        c.token = "tk_exemplo".into();
        assert_eq!(c.authorization("segredo").unwrap(), "Bearer tk_exemplo");
    }

    #[test]
    fn the_answer_link_carries_only_the_reply_account() {
        let mut c = config();
        c.user = "smith".into();
        c.token = "tk_da_conta_principal".into();
        // With no reply account, the link carries nothing rather than falling
        // back to the account that can also read every question.
        let bare = c.answer_url("abc123", Answer::Continue, "segredo");
        assert!(!bare.contains("auth="));
        assert!(!bare.contains("tk_da_conta_principal"));
        c.reply_user = "smith-resposta".into();
        let link = c.answer_url("abc123", Answer::Continue, "segredo");
        // base64url without padding of `Basic base64(smith-resposta:segredo)`,
        // which is how the server decodes the auth parameter.
        assert!(link.contains("&auth=QmFzaWMgYzIxcGRHZ3RjbVZ6Y0c5emRHRTZjMlZuY21Wa2J3"));
        assert!(!link.contains("tk_da_conta_principal"));
        assert_eq!(
            c.reply_authorization("segredo").unwrap(),
            "Basic c21pdGgtcmVzcG9zdGE6c2VncmVkbw=="
        );
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
