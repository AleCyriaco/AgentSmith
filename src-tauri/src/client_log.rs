//! Diagnostics for the official clients (Claude Code, Codex, Gemini CLI, Grok
//! Build), appended to `~/Library/Logs/AgentSmith/client-diagnostics.log`
//! whenever one of them exits with a failure.
//!
//! The interface never shows a client's raw output, because it can carry
//! prompts, account identifiers or credentials; this file exists so the reason
//! for a failure is not lost with it. It records the exit code, the client's
//! final result fields, and the tail of its error stream — never the prompt,
//! never an image — and stops growing at eight megabytes.
use std::io::Write;

const CAP: u64 = 8 * 1024 * 1024;
const TAIL: usize = 4 * 1024;

fn path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        std::path::PathBuf::from(home)
            .join("Library/Logs/AgentSmith")
            .join("client-diagnostics.log"),
    )
}

fn tail(bytes: &[u8]) -> String {
    let start = bytes.len().saturating_sub(TAIL);
    String::from_utf8_lossy(&bytes[start..]).trim().to_string()
}

/// Records a failed client run. `result` is the client's final structured
/// result when it produced one; only its status fields are kept.
pub fn failure(client: &str, code: i32, result: Option<&serde_json::Value>, stderr: &[u8]) {
    let Some(path) = path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::metadata(&path).map(|m| m.len() > CAP).unwrap_or(false) {
        return;
    }
    let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let fields = result
        .map(|r| {
            format!(
                "subtype={} is_error={} errors={} result={}",
                r["subtype"],
                r["is_error"],
                r["errors"],
                r["result"].as_str().map(|t| tail(t.as_bytes())).unwrap_or_default()
            )
        })
        .unwrap_or_else(|| "sem resultado estruturado".into());
    let _ = writeln!(
        file,
        "{} · {client} · código {code}\n  {fields}\n  stderr: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        if stderr.is_empty() { "(vazio)".to_string() } else { tail(stderr).replace('\n', "\n          ") }
    );
}
