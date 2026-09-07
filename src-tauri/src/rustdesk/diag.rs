//! Session log for the RustDesk transport: one line per event, timed from the
//! start of the connection, written to
//! `~/Library/Logs/AgentSmith/rustdesk-session.log` and started fresh on every
//! connection. It exists to be read after something went wrong. It never
//! records pixels, passwords or verification codes.
use std::{
    fs::File,
    io::Write,
    path::PathBuf,
    sync::Mutex,
    time::Instant,
};

struct Log {
    file: File,
    started: Instant,
    written: usize,
}

static LOG: Mutex<Option<Log>> = Mutex::new(None);

/// Past this the log stops growing; a session that misbehaves for hours
/// should not fill the disk to prove it.
const CAP: usize = 32 * 1024 * 1024;

pub fn path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Library/Logs/AgentSmith")
            .join("rustdesk-session.log"),
    )
}

/// Opens a fresh log for a connection. Failure to open is not an error the
/// connection should care about: the session runs the same without a log.
pub fn start(label: &str) {
    let Some(path) = path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(file) = File::create(&path) else { return };
    *LOG.lock().unwrap() = Some(Log {
        file,
        started: Instant::now(),
        written: 0,
    });
    log(format_args!("sessão {label}"));
}

pub fn log(args: std::fmt::Arguments) {
    let mut guard = LOG.lock().unwrap();
    let Some(log) = guard.as_mut() else { return };
    if log.written > CAP {
        return;
    }
    let elapsed = log.started.elapsed();
    let line = format!(
        "{:>8}.{:03} {}\n",
        elapsed.as_secs(),
        elapsed.subsec_millis(),
        args
    );
    if log.file.write_all(line.as_bytes()).is_ok() {
        log.written += line.len();
    }
}

#[macro_export]
macro_rules! diag {
    ($($arg:tt)*) => {
        $crate::rustdesk::diag::log(format_args!($($arg)*))
    };
}
