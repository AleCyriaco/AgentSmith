//! Reaching the operator over SimpleX: status notices, and questions the
//! operator answers from a phone.
//!
//! The official `simplex-chat` client runs as a separate process and speaks a
//! WebSocket API, the same arrangement as the browser-login clients. No
//! SimpleX source is copied or linked, so AgentSmith stays MIT.
pub mod client;
pub mod protocol;
pub mod service;

pub use protocol::{Answer, Question};
pub use service::{Simplex, Status};
