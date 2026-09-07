use crate::model::*;
use crate::rustdesk::{
    decoder::Decoder,
    input::{self, Input},
    session::{self, Codec, Event},
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::{
    io::Cursor,
    path::Path,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStdin, Command},
};

struct Connection {
    child: Child,
    input: ChildStdin,
}

/// The two transports a session can run on. Both feed the same frame slot and
/// accept the same actions, so nothing above this layer needs to tell them apart.
enum Transport {
    Rdp(Connection),
    /// Input goes to the task that owns the RustDesk session; dropping the
    /// sender is what ends that task.
    RustDesk(tokio::sync::mpsc::Sender<Vec<Input>>),
}
pub struct Remote {
    connection: tokio::sync::Mutex<Option<Transport>>,
    pub info: Arc<Mutex<SessionInfo>>,
    frame: Arc<Mutex<Option<Snapshot>>>,
    generation: Arc<AtomicU64>,
    pub epoch: Arc<AtomicU64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Inspect {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    Click {
        x: u32,
        y: u32,
    },
    DoubleClick {
        x: u32,
        y: u32,
    },
    RightClick {
        x: u32,
        y: u32,
    },
    TypeText {
        text: String,
    },
    Key {
        keys: Vec<String>,
    },
    Scroll {
        direction: String,
        amount: u32,
    },
    Wait {
        seconds: u32,
    },
    StepDone {
        evidence: String,
    },
    Blocked {
        reason: String,
    },
}
impl Remote {
    #[cfg(test)]
    pub fn observed_fixture(frame: Snapshot, machine_id: &str) -> Self {
        let remote = Self::new();
        *remote.frame.lock().unwrap() = Some(frame);
        remote.info.lock().unwrap().status = "connected".into();
        remote.info.lock().unwrap().machine_id = machine_id.into();
        remote
    }
    pub fn new() -> Self {
        Self {
            connection: tokio::sync::Mutex::new(None),
            info: Arc::new(Mutex::new(SessionInfo::default())),
            frame: Arc::new(Mutex::new(None)),
            generation: Arc::new(AtomicU64::new(0)),
            epoch: Arc::new(AtomicU64::new(0)),
        }
    }
    pub async fn disconnect(&self) -> Result<(), String> {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Some(Transport::Rdp(mut c)) = self.connection.lock().await.take() {
            let _ = c.child.kill().await;
            let _ = c.child.wait().await;
        }
        *self.frame.lock().unwrap() = None;
        *self.info.lock().unwrap() = SessionInfo::default();
        Ok(())
    }
    pub async fn connect(
        &self,
        m: &Machine,
        password: &str,
        helper: &Path,
        capture_interval_ms: u32,
    ) -> Result<(), String> {
        if m.protocol == "rustdesk" {
            return self.connect_rustdesk(m, password).await;
        }
        if m.protocol != "rdp" {
            return Err("Este conector está previsto na arquitetura, mas ainda não foi implementado nesta versão.".into());
        }
        if password.contains(['\r', '\n', '\0']) || password.len() > 4000 {
            return Err("Formato de senha não suportado pelo conector atual.".into());
        }
        m.display.validate()?;
        validate_capture_interval(capture_interval_ms)?;
        self.disconnect().await?;
        let generation = self.generation.load(Ordering::SeqCst);
        let mut child = Command::new(helper)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| {
                "Conector FreeRDP não encontrado. Recompile os recursos nativos.".to_string()
            })?;
        let mut input = child
            .stdin
            .take()
            .ok_or("Entrada do conector indisponível")?;
        let mut output = child
            .stdout
            .take()
            .ok_or("Saída do conector indisponível")?;
        input
            .write_all(
                format!(
                    "{}\n{}\n{}\n{}\n{}\n{}\n{} {} {} {}\n",
                    m.host,
                    m.port,
                    m.username,
                    m.domain,
                    password,
                    m.fingerprint,
                    m.display.width,
                    m.display.height,
                    m.display.scale,
                    capture_interval_ms
                )
                .as_bytes(),
            )
            .await
            .map_err(|_| "Falha ao iniciar conexão.")?;
        *self.info.lock().unwrap() = SessionInfo {
            machine_id: m.id.clone(),
            status: "connecting".into(),
            message: "Autenticando no Windows…".into(),
        };
        *self.connection.lock().await = Some(Transport::Rdp(Connection { child, input }));
        let info = self.info.clone();
        let frame = self.frame.clone();
        let gen = self.generation.clone();
        tokio::spawn(async move {
            let mut seq = 0;
            loop {
                let mut tag = [0];
                if output.read_exact(&mut tag).await.is_err() {
                    break;
                }
                if gen.load(Ordering::SeqCst) != generation {
                    return;
                }
                if tag[0] == b'S' {
                    let Ok(n) = output.read_u32_le().await else {
                        break;
                    };
                    if n > 65536 {
                        break;
                    }
                    let mut b = vec![0; n as usize];
                    if output.read_exact(&mut b).await.is_err() {
                        break;
                    }
                    let msg = String::from_utf8_lossy(&b).to_string();
                    let mut i = info.lock().unwrap();
                    if msg == "connected" {
                        i.status = "connected".into();
                        i.message = "Conexão RDP ativa".into();
                    } else if msg == "disconnected" {
                        i.status = "disconnected".into();
                        i.message = "Conexão encerrada.".into();
                    } else {
                        i.status = "error".into();
                        if i.message.contains("impressão digital") {
                            i.message.push_str(&format!(" • {msg}"));
                        } else {
                            i.message = msg;
                        }
                    }
                } else if tag[0] == b'F' {
                    let (Ok(w), Ok(h), Ok(n)) = (
                        output.read_u32_le().await,
                        output.read_u32_le().await,
                        output.read_u32_le().await,
                    ) else {
                        break;
                    };
                    if w == 0 || h == 0 || w > 4096 || h > 2160 || n != w * h * 4 {
                        break;
                    }
                    let mut bytes = vec![0; n as usize];
                    if output.read_exact(&mut bytes).await.is_err() {
                        break;
                    }
                    seq += 1;
                    let encoded = tokio::task::spawn_blocking(move || {
                        // FreeRDP hands over BGRA with an unset alpha channel.
                        for p in bytes.chunks_exact_mut(4) {
                            p.swap(0, 2);
                            p[3] = 255;
                        }
                        encode_frame(bytes, w, h, seq)
                    })
                    .await
                    .ok()
                    .flatten();
                    if gen.load(Ordering::SeqCst) != generation {
                        return;
                    }
                    if encoded.is_some() {
                        *frame.lock().unwrap() = encoded;
                    }
                } else {
                    break;
                }
            }
            if gen.load(Ordering::SeqCst) == generation {
                let mut i = info.lock().unwrap();
                if i.status != "error" {
                    i.status = "disconnected".into();
                    i.message = "A conexão foi interrompida. Reconecte para retomar.".into();
                }
                *frame.lock().unwrap() = None;
            }
        });
        Ok(())
    }
    /// Opens a RustDesk session and keeps it running in its own task, feeding
    /// the same frame slot the RDP transport uses.
    async fn connect_rustdesk(&self, m: &Machine, password: &str) -> Result<(), String> {
        self.disconnect().await?;
        let generation = self.generation.load(Ordering::SeqCst);
        // Connecting before spawning means a bad ID, a refused password or an
        // offline machine reaches the operator as an error, not as a silent wait.
        let session = session::Session::connect(&session::Options {
            id: m.host.trim().into(),
            password: password.into(),
            rendezvous: m.rustdesk_server.clone(),
            key: m.rustdesk_key.clone(),
        })
        .await?;
        let (peer, mut events, mut commands) = session.split();
        *self.info.lock().unwrap() = SessionInfo {
            machine_id: m.id.clone(),
            status: "connected".into(),
            message: format!(
                "Sessão RustDesk ativa com {} ({}×{})",
                if peer.hostname.is_empty() {
                    m.name.clone()
                } else {
                    peer.hostname.clone()
                },
                peer.width,
                peer.height
            ),
        };
        let (sender, mut inbox) = tokio::sync::mpsc::channel::<Vec<Input>>(64);
        *self.connection.lock().await = Some(Transport::RustDesk(sender));
        let info = self.info.clone();
        let frame = self.frame.clone();
        let gen = self.generation.clone();
        tokio::spawn(async move {
            let _ = commands.request_refresh().await;
            let mut decoder: Option<(Codec, Decoder)> = None;
            let mut seq = 0u64;
            let ended = loop {
                if gen.load(Ordering::SeqCst) != generation {
                    return;
                }
                tokio::select! {
                    inputs = inbox.recv() => {
                        // The sender is dropped on disconnect, which ends the task.
                        let Some(inputs) = inputs else { return };
                        if let Err(error) = commands.send(&inputs).await {
                            break error;
                        }
                    }
                    event = events.next() => match event {
                        Err(error) => break error,
                        Ok(Event::Closed(reason)) => {
                            break format!("O par RustDesk encerrou a sessão: {reason}")
                        }
                        Ok(Event::Ping(delay)) => {
                            if let Err(error) = commands.pong(delay).await {
                                break error;
                            }
                        }
                        Ok(Event::Idle) => {}
                        Ok(Event::Video { codec, data, .. }) => {
                            // A codec switch mid-stream needs its own decoder.
                            if !matches!(&decoder, Some((current, _)) if *current == codec) {
                                match Decoder::new(codec) {
                                    Ok(fresh) => decoder = Some((codec, fresh)),
                                    Err(error) => break error,
                                }
                            }
                            let Some((_, active)) = decoder.as_mut() else { continue };
                            let picture = match active.decode(&data) {
                                Ok(Some(picture)) => picture,
                                Ok(None) => continue,
                                Err(error) => break error,
                            };
                            seq += 1;
                            let encoded = tokio::task::spawn_blocking(move || {
                                encode_frame(picture.rgba, picture.width, picture.height, seq)
                            })
                            .await
                            .ok()
                            .flatten();
                            if gen.load(Ordering::SeqCst) != generation {
                                return;
                            }
                            if encoded.is_some() {
                                *frame.lock().unwrap() = encoded;
                            }
                        }
                    }
                }
            };
            if gen.load(Ordering::SeqCst) == generation {
                let mut current = info.lock().unwrap();
                current.status = "error".into();
                current.message = ended;
                *frame.lock().unwrap() = None;
            }
        });
        Ok(())
    }
    pub async fn configure_capture(&self, interval: u32) -> Result<(), String> {
        validate_capture_interval(interval)?;
        // A RustDesk peer paces its own stream; the setting is kept for RDP.
        if let Some(Transport::Rdp(c)) = self.connection.lock().await.as_mut() {
            c.input
                .write_all(format!("interval {interval}\n").as_bytes())
                .await
                .map_err(|_| "Ritmo salvo. Reconecte para aplicar ao conector.")?;
        }
        Ok(())
    }
    pub fn snapshot_if_new(&self, sequence: u64) -> Result<Option<Snapshot>, String> {
        if self.info.lock().unwrap().status != "connected" {
            return Ok(None);
        }
        let guard = self.frame.lock().unwrap();
        let f = guard.as_ref().ok_or("Aguardando imagem do Windows.")?;
        if now().saturating_sub(f.captured_at) > 5000 {
            return Err("A imagem está desatualizada.".into());
        }
        Ok((f.sequence != sequence).then(|| f.clone()))
    }
    pub async fn wait_for_new_frame(&self, sequence: u64) -> Result<(), String> {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if self.snapshot_if_new(sequence)?.is_some() {
                    return Ok(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            }
        })
        .await
        .map_err(|_| "Aguardando atualização da tela; verifique a conexão.".to_string())?
    }
    pub fn snapshot(&self) -> Result<Snapshot, String> {
        if self.info.lock().unwrap().status != "connected" {
            return Err("Conecte a máquina antes de continuar.".into());
        }
        let f = self
            .frame
            .lock()
            .unwrap()
            .clone()
            .ok_or("Aguardando a primeira imagem do Windows.")?;
        if now().saturating_sub(f.captured_at) > 5000 {
            return Err("A imagem está desatualizada; verifique a conexão.".into());
        }
        Ok(f)
    }
    pub async fn release(&self) -> Result<(), String> {
        let mut guard = self.connection.lock().await;
        if let Some(Transport::RustDesk(sender)) = guard.as_ref() {
            return sender
                .send(input::release())
                .await
                .map_err(|_| "A sessão RustDesk foi encerrada.".into());
        }
        if let Some(Transport::Rdp(c)) = guard.as_mut() {
            let mut commands = String::new();
            for key in [0x1d, 0x11d, 0x2a, 0x36, 0x38, 0x138, 0x15b, 0x15c] {
                commands.push_str(&format!("key {key} 0\n"));
            }
            for flags in [0x1000, 0x2000, 0x4000] {
                commands.push_str(&format!("mouse {flags} 0 0\n"));
            }
            c.input
                .write_all(commands.as_bytes())
                .await
                .map_err(|_| "Conexão interrompida.")?;
        }
        Ok(())
    }
    pub async fn act(&self, action: &Action, epoch: u64) -> Result<(), String> {
        let f = self.snapshot()?;
        let mut guard = self.connection.lock().await;
        if self.epoch.load(Ordering::SeqCst) != epoch {
            return Err("Execução pausada.".into());
        }
        match guard.as_mut().ok_or("Não existe conexão ativa.")? {
            Transport::Rdp(c) => c
                .input
                .write_all(action_commands(action, f.width, f.height)?.as_bytes())
                .await
                .map_err(|_| "Conexão interrompida ao enviar entrada.".into()),
            Transport::RustDesk(sender) => sender
                .send(input::translate(action, f.width, f.height)?)
                .await
                .map_err(|_| "A sessão RustDesk foi encerrada.".into()),
        }
    }
}
/// Wraps an RGBA image as the PNG data URL the interface and the vision
/// pipeline already consume. Shared by both transports.
fn encode_frame(rgba: Vec<u8>, width: u32, height: u32, sequence: u64) -> Option<Snapshot> {
    let image = image::RgbaImage::from_raw(width, height, rgba)?;
    let mut png = Cursor::new(vec![]);
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut png, image::ImageFormat::Png)
        .ok()?;
    Some(Snapshot {
        data_url: format!("data:image/png;base64,{}", STANDARD.encode(png.into_inner())),
        width,
        height,
        sequence,
        captured_at: now(),
    })
}

pub fn action_commands(a: &Action, w: u32, h: u32) -> Result<String, String> {
    let click = |x: u32, y: u32, button: u32, n: u32| -> Result<String, String> {
        if x >= w || y >= h {
            return Err("O clique ficou fora da imagem.".into());
        }
        let mut s = format!("mouse 2048 {x} {y}\n");
        for _ in 0..n {
            s += &format!(
                "mouse {} {x} {y}\nmouse {button} {x} {y}\n",
                button | 0x8000
            );
        }
        Ok(s)
    };
    match a {
        Action::Click { x, y } => click(*x, *y, 0x1000, 1),
        Action::DoubleClick { x, y } => click(*x, *y, 0x1000, 2),
        Action::RightClick { x, y } => click(*x, *y, 0x2000, 1),
        Action::TypeText { text } => {
            if text.is_empty() || text.chars().count() > 400 || text.contains('\0') {
                return Err("Digite no máximo 400 caracteres por ação.".into());
            }
            Ok(text
                .encode_utf16()
                .map(|u| format!("unicode {u} 1\nunicode {u} 0\n"))
                .collect())
        }
        Action::Key { keys } => {
            if keys.is_empty() || keys.len() > 5 {
                return Err("Combinação de teclas inválida.".into());
            }
            let normalized: Vec<_> = keys.iter().map(|k| k.to_ascii_lowercase()).collect();
            let primary = normalized
                .iter()
                .filter(|k| !matches!(k.as_str(), "ctrl" | "alt" | "shift" | "win" | "meta"))
                .count();
            let unique: std::collections::HashSet<_> = normalized.iter().collect();
            if primary > 1 || unique.len() != keys.len() {
                return Err("keys aceita um atalho por ação, não uma sequência. Envie cada atalho/Enter em uma ação separada.".into());
            }
            let codes: Vec<u32> = keys
                .iter()
                .map(|k| scancode(k).ok_or(format!("Tecla não suportada: {k}")))
                .collect::<Result<_, _>>()?;
            let mut out = String::new();
            for c in &codes {
                out += &format!("key {c} 1\n");
            }
            for c in codes.iter().rev() {
                out += &format!("key {c} 0\n");
            }
            Ok(out)
        }
        Action::Scroll { direction, amount } => {
            if !(1..=10).contains(amount) || !["up", "down"].contains(&direction.as_str()) {
                return Err("Rolagem inválida.".into());
            }
            let flag = if direction == "up" { 0x0278 } else { 0x0388 };
            Ok((0..*amount)
                .map(|_| format!("mouse {flag} 0 0\n"))
                .collect())
        }
        Action::Inspect { .. }
        | Action::Wait { .. }
        | Action::StepDone { .. }
        | Action::Blocked { .. } => Err("Essa ação não envia entrada ao Windows.".into()),
    }
}
fn scancode(k: &str) -> Option<u32> {
    Some(match k.to_ascii_lowercase().as_str() {
        "ctrl" => 0x1d,
        "alt" => 0x38,
        "shift" => 0x2a,
        "win" | "meta" => 0x15b,
        "enter" => 0x1c,
        "tab" => 0x0f,
        "esc" | "escape" => 0x01,
        "backspace" => 0x0e,
        "delete" => 0x153,
        "space" => 0x39,
        "up" => 0x148,
        "down" => 0x150,
        "left" => 0x14b,
        "right" => 0x14d,
        "home" => 0x147,
        "end" => 0x14f,
        "pageup" => 0x149,
        "pagedown" => 0x151,
        "f1" => 0x3b,
        "f2" => 0x3c,
        "f3" => 0x3d,
        "f4" => 0x3e,
        "f5" => 0x3f,
        "f6" => 0x40,
        "f7" => 0x41,
        "f8" => 0x42,
        "f9" => 0x43,
        "f10" => 0x44,
        "f11" => 0x57,
        "f12" => 0x58,
        "a" => 0x1e,
        "b" => 0x30,
        "c" => 0x2e,
        "d" => 0x20,
        "e" => 0x12,
        "f" => 0x21,
        "g" => 0x22,
        "h" => 0x23,
        "i" => 0x17,
        "j" => 0x24,
        "k" => 0x25,
        "l" => 0x26,
        "m" => 0x32,
        "n" => 0x31,
        "o" => 0x18,
        "p" => 0x19,
        "q" => 0x10,
        "r" => 0x13,
        "s" => 0x1f,
        "t" => 0x14,
        "u" => 0x16,
        "v" => 0x2f,
        "w" => 0x11,
        "x" => 0x2d,
        "y" => 0x15,
        "z" => 0x2c,
        "1" => 2,
        "2" => 3,
        "3" => 4,
        "4" => 5,
        "5" => 6,
        "6" => 7,
        "7" => 8,
        "8" => 9,
        "9" => 10,
        "0" => 11,
        _ => return None,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn input_is_bounded_and_keys_released() {
        assert!(action_commands(&Action::Click { x: 1280, y: 0 }, 1280, 800).is_err());
        assert!(action_commands(
            &Action::TypeText {
                text: "x".repeat(401)
            },
            1280,
            800
        )
        .is_err());
        let s = action_commands(
            &Action::Key {
                keys: vec!["ctrl".into(), "s".into()],
            },
            1280,
            800,
        )
        .unwrap();
        assert_eq!(s, "key 29 1\nkey 31 1\nkey 31 0\nkey 29 0\n");
    }
    #[tokio::test]
    async fn capture_cache_skips_duplicates_and_waits_for_fresh_frame() {
        let remote = Arc::new(Remote::new());
        remote.info.lock().unwrap().status = "connected".into();
        *remote.frame.lock().unwrap() = Some(Snapshot {
            data_url: "test".into(),
            width: 1280,
            height: 800,
            sequence: 1,
            captured_at: now(),
        });
        assert!(remote.snapshot_if_new(1).unwrap().is_none());
        assert!(remote.snapshot_if_new(0).unwrap().is_some());
        let writer = remote.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(60)).await;
            writer.frame.lock().unwrap().as_mut().unwrap().sequence = 2;
        });
        remote.wait_for_new_frame(1).await.unwrap();
        assert_eq!(remote.snapshot().unwrap().sequence, 2);
        remote.frame.lock().unwrap().as_mut().unwrap().captured_at = now() - 6000;
        assert!(remote.snapshot_if_new(1).is_err());
    }
    #[test]
    fn unknown_actions_cannot_execute() {
        assert!(
            serde_json::from_str::<Action>(r#"{"kind":"shell","command":"anything"}"#).is_err()
        );
        assert!(serde_json::from_str::<Action>(
            r#"{"kind":"click","x":2,"y":3,"command":"anything"}"#
        )
        .is_err());
    }
}

fn validate_capture_interval(interval: u32) -> Result<(), String> {
    if (20..=2000).contains(&interval) {
        Ok(())
    } else {
        Err("Intervalo de captura inválido.".into())
    }
}

#[cfg(test)]
#[test]
fn capture_interval_accepts_fast_rates_and_rejects_unbounded_polling() {
    for value in [20, 50, 99, 100, 2000] {
        assert!(validate_capture_interval(value).is_ok());
    }
    for value in [0, 19, 2001] {
        assert!(validate_capture_interval(value).is_err());
    }
}
