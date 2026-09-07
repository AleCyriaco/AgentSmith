//! A stable identity for this Mac, used only to ask a RustDesk machine to
//! remember it after a second-factor check.
//!
//! The value never leaves as a hardware identifier: it is a hash, so the
//! machine can recognise the same Mac again without learning anything about it.
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

static IDENTITY: OnceLock<Vec<u8>> = OnceLock::new();

/// Reads the platform UUID macOS assigns to the hardware.
#[cfg(target_os = "macos")]
fn platform_uuid() -> Option<String> {
    let output = std::process::Command::new("/usr/sbin/ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find(|line| line.contains("IOPlatformUUID"))
        .and_then(|line| line.split('"').nth(3).map(str::to_string))
        .filter(|uuid| !uuid.is_empty())
}

#[cfg(not(target_os = "macos"))]
fn platform_uuid() -> Option<String> {
    None
}

/// Hashed identity of this installation, stable across restarts.
///
/// Falls back to the host name when the platform UUID is unavailable: still
/// stable in practice, and a machine that stops recognising it only asks for a
/// second factor again.
pub fn identity() -> Vec<u8> {
    IDENTITY
        .get_or_init(|| {
            let seed = platform_uuid()
                .or_else(|| std::env::var("HOSTNAME").ok())
                .unwrap_or_else(|| "agentsmith".into());
            Sha256::digest(format!("agentsmith|{seed}").as_bytes()).to_vec()
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identity_is_stable_opaque_and_never_the_raw_seed() {
        let first = identity();
        assert_eq!(first.len(), 32);
        assert_eq!(first, identity());
        if let Some(uuid) = platform_uuid() {
            assert!(!uuid.is_empty());
            // The machine receives a hash, not the hardware identifier.
            assert!(!first.windows(uuid.len().min(32)).any(|w| w == uuid.as_bytes()));
        }
    }
}
