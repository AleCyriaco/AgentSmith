//! What AgentSmith says to the operator and what an answer means, independent
//! of how the message travels.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A question put to the operator, waiting on a reply.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Question {
    /// The run this question belongs to.
    pub run_id: String,
    pub title: String,
    /// Why the run stopped and what a yes would do.
    pub detail: String,
}

/// A one-time word that an answer must carry to count.
///
/// An answer travels over a channel anyone may be able to write to, and it
/// authorises a machine to resume acting on its own. Tying each question to a
/// fresh unguessable word means a stray or replayed message cannot resume
/// anything, and an answer to a question already settled is ignored.
pub fn ticket() -> String {
    let mut bytes = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
    format!("{:x}", Sha256::digest(bytes))[..24].to_string()
}

/// What the operator answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Continue,
    Stop,
}

/// Reads an answer out of a message typed on a phone.
///
/// Only an unambiguous reply counts. Anything else is left unanswered rather
/// than guessed, because guessing here either resumes a task the operator
/// wanted stopped or abandons one they wanted finished.
pub fn read_answer(message: &str) -> Option<Answer> {
    let text = message.trim().to_lowercase();
    let text = text.trim_matches(|c: char| !c.is_alphanumeric());
    match text {
        "1" | "sim" | "s" | "yes" | "y" | "continuar" | "continua" | "continue" | "ok"
        | "segue" | "seguir" | "pode" | "autorizo" => Some(Answer::Continue),
        "2" | "nao" | "não" | "n" | "no" | "parar" | "para" | "stop" | "cancelar" | "cancel"
        | "abortar" => Some(Answer::Stop),
        _ => None,
    }
}

impl Question {
    /// The message sent to a chat, where the operator answers by typing.
    pub fn message(&self) -> String {
        format!(
            "AgentSmith precisa de uma decisão.\n\n{}\n\n{}\n\nResponda 1 para continuar ou 2 para parar.",
            self.title, self.detail
        )
    }

    /// The message sent to a channel that answers by opening a link, where
    /// the two choices are spelled out as addresses.
    ///
    /// Both are given as links because an action button reaches only some
    /// phones, while a link in the body is readable on all of them.
    pub fn message_with_links(&self, resume: &str, stop: &str) -> String {
        format!(
            "{}\n\n{}\n\nContinuar: {resume}\nParar: {stop}",
            self.title, self.detail
        )
    }
}

/// Every way AgentSmith can reach the operator. Whichever are configured are
/// all told; a question is put to each, and the first answer settles it.
pub struct Channels {
    pub simplex: std::sync::Arc<crate::simplex::Simplex>,
    pub ntfy: std::sync::Arc<crate::ntfy::Ntfy>,
}

impl Channels {
    pub fn new() -> Self {
        Self {
            simplex: std::sync::Arc::new(crate::simplex::Simplex::new()),
            ntfy: std::sync::Arc::new(crate::ntfy::Ntfy::new()),
        }
    }

    pub async fn notify(&self, title: &str, detail: &str) {
        // Both are told, so a channel being down does not silence the other.
        tokio::join!(
            self.simplex.notify(title, detail),
            self.ntfy.notify(title, detail)
        );
    }

    /// Puts the question to every channel and takes the first answer. A person
    /// answers once, from whichever device is at hand.
    pub async fn ask(&self, question: &Question) -> Option<Answer> {
        let simplex = self.simplex.clone();
        let ntfy = self.ntfy.clone();
        let (a, b) = (question.clone(), question.clone());
        tokio::select! {
            answer = async move { simplex.ask(&a).await } => answer,
            answer = async move { ntfy.ask(&b).await } => answer,
        }
    }
}

/// A status notice. Kept separate from questions: a notice expects no reply.
pub fn notice(title: &str, detail: &str) -> String {
    if detail.trim().is_empty() {
        format!("AgentSmith · {title}")
    } else {
        format!("AgentSmith · {title}\n\n{detail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clear_reply_is_read_and_anything_else_is_left_unanswered() {
        for yes in ["1", "sim", "Sim", " SIM ", "s", "yes", "ok", "continuar", "pode", "1."] {
            assert_eq!(read_answer(yes), Some(Answer::Continue), "{yes}");
        }
        for no in ["2", "não", "nao", "NÃO", "n", "no", "parar", "cancelar", "2!"] {
            assert_eq!(read_answer(no), Some(Answer::Stop), "{no}");
        }
        // Ambiguous or unrelated: never guessed in either direction.
        for unclear in [
            "",
            "   ",
            "sim, mas depois",
            "acho que sim",
            "não sei",
            "o que houve?",
            "12",
            "stopwatch",
        ] {
            assert_eq!(read_answer(unclear), None, "{unclear}");
        }
    }

    #[test]
    fn a_question_spells_out_how_to_answer() {
        let question = Question {
            run_id: "r1".into(),
            title: "Abrir Calculadora".into(),
            detail: "A etapa 2 não encontrou a caixa de pesquisa.".into(),
        };
        let message = question.message();
        assert!(message.contains("Abrir Calculadora"));
        assert!(message.contains("não encontrou"));
        assert!(message.contains("1 para continuar"));
        assert!(message.contains("2 para parar"));
    }

    #[test]
    fn the_channels_keychain_identity_passes_the_secret_guard() {
        // Secret ids must be UUIDs; a plain name is refused, which silently
        // broke saving the server address.
        crate::model::valid_id(crate::simplex::SECRET_ID).unwrap();
        assert!(crate::model::valid_id("simplex").is_err());
    }

    #[test]
    fn a_ticket_is_unguessable_and_never_repeats() {
        let first = ticket();
        assert_eq!(first.len(), 24);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        let many: std::collections::HashSet<String> = (0..200).map(|_| ticket()).collect();
        assert_eq!(many.len(), 200, "um bilhete repetido autorizaria duas vezes");
    }

    #[test]
    fn a_question_sent_as_links_offers_both_choices() {
        let question = Question {
            run_id: "r1".into(),
            title: "Abrir Calculadora".into(),
            detail: "A etapa 2 não encontrou a caixa.".into(),
        };
        let text = question.message_with_links("https://exemplo/sim", "https://exemplo/nao");
        assert!(text.contains("Abrir Calculadora") && text.contains("não encontrou"));
        assert!(text.contains("Continuar: https://exemplo/sim"));
        assert!(text.contains("Parar: https://exemplo/nao"));
    }

    #[test]
    fn a_notice_without_detail_stays_one_line() {
        assert_eq!(notice("Tarefa concluída", ""), "AgentSmith · Tarefa concluída");
        assert!(notice("Conexão perdida", "O par encerrou.").contains("O par encerrou."));
    }
}
