//! The parts of the SimpleX conversation that can be decided without a
//! network: what a message from the operator means, and what AgentSmith says.
use serde::{Deserialize, Serialize};

/// A question put to the operator, waiting on a reply.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Question {
    /// The run this question belongs to.
    pub run_id: String,
    pub title: String,
    /// Why the run stopped and what a yes would do.
    pub detail: String,
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
    /// The message sent to the phone. The reply keywords are spelled out,
    /// because the operator answers from a chat, with no buttons.
    pub fn message(&self) -> String {
        format!(
            "AgentSmith precisa de uma decisão.\n\n{}\n\n{}\n\nResponda 1 para continuar ou 2 para parar.",
            self.title, self.detail
        )
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
    fn a_notice_without_detail_stays_one_line() {
        assert_eq!(notice("Tarefa concluída", ""), "AgentSmith · Tarefa concluída");
        assert!(notice("Conexão perdida", "O par encerrou.").contains("O par encerrou."));
    }
}
