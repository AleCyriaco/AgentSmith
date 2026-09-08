//! Reaching the operator over SimpleX: status notices, and questions the
//! operator answers from a phone.
//!
//! The official `simplex-chat` client runs as a separate process and speaks a
//! WebSocket API, the same arrangement as the browser-login clients. No
//! SimpleX source is copied or linked, so AgentSmith stays MIT.
pub mod client;
pub mod host;
pub mod install;
pub mod service;

/// Keychain identity for the one secret this channel keeps, the server
/// address. Secret ids are UUIDs so a caller cannot name an arbitrary Keychain
/// account; the channel is a single fixed entity rather than one per machine,
/// so it carries a constant of the same shape.
pub const SECRET_ID: &str = "5a1e9c3f-7b62-4d18-9e4a-2c8f6d0b1a73";
pub const SECRET_BINDING: &str = "simplex://server";

pub use service::{Simplex, Status};
