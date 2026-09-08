//! A private, independently licensed SMP relay, managed through Podman.
//! No SimpleX server code is linked into AgentSmith. Never expose process output:
//! the official server prints its queue-creation password during initialization.
use serde::Serialize;
use serde_json::Value;
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{process::Command, sync::Mutex};

const IMAGE: &str = "docker.io/simplexchat/smp-server:v6.5.0@sha256:35fd210753fd6552b8d59591bee095b6839f3b69a903e2e9505fbaac09467041";
const NAME: &str = "agentsmith-simplex";
const LABEL: &str = "com.agentsmith.service";
const TARGET: &str = "127.0.0.1:17423";

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostStatus {
    pub phase: String,
    pub message: String,
    pub server_address: String,
    pub server_qr: String,
}
#[derive(Default)]
pub struct Host {
    status: Mutex<HostStatus>,
    connection: Mutex<String>,
}

pub fn qr(value: &str) -> Result<String, String> {
    Ok(qrcode::QrCode::new(value.as_bytes())
        .map_err(|_| "Não foi possível criar o QR code.")?
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(280, 280)
        .build())
}
pub fn data_dir() -> Result<PathBuf, String> {
    Ok(
        PathBuf::from(std::env::var_os("HOME").ok_or("Pasta pessoal indisponível.")?)
            .join("Library/Application Support/com.agentsmith.desktop/simplex"),
    )
}
fn executable(paths: &[&str]) -> Option<PathBuf> {
    paths.iter().map(PathBuf::from).find(|p| p.is_file())
}
fn podman() -> Result<PathBuf, String> {
    executable(&["/opt/homebrew/bin/podman", "/usr/local/bin/podman"])
        .ok_or("Instale o Podman neste Mac para hospedar o SimpleX. Depois clique em Preparar e ativar; o AgentSmith cria o servidor automaticamente.".into())
}
fn tailscale() -> Result<PathBuf, String> {
    executable(&[
        "/usr/local/bin/tailscale",
        "/opt/homebrew/bin/tailscale",
        "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
    ])
    .ok_or("Instale e conecte o Tailscale neste Mac antes de ativar o SimpleX.".into())
}
async fn output(mut command: Command, timeout: u64, context: &str) -> Result<String, String> {
    command.stdin(Stdio::null()).kill_on_drop(true);
    let result = tokio::time::timeout(Duration::from_secs(timeout), command.output())
        .await
        .map_err(|_| format!("{context}: tempo de espera excedido. Tente novamente."))?
        .map_err(|_| format!("{context}: não foi possível iniciar o componente."))?;
    if !result.status.success() {
        return Err(format!("{context}: o componente recusou a operação. Verifique se está disponível e tente novamente."));
    }
    String::from_utf8(result.stdout).map_err(|_| format!("{context}: resposta inválida."))
}
fn command(path: &std::path::Path, args: &[&str]) -> Command {
    let mut c = Command::new(path);
    c.args(args);
    c
}
fn json(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|_| "Resposta inválida do componente local.".into())
}
fn hostname(value: &Value) -> Result<String, String> {
    if value["BackendState"] != "Running" {
        return Err("Conecte o Tailscale neste Mac e tente novamente.".into());
    }
    let host = value["Self"]["DNSName"]
        .as_str()
        .unwrap_or("")
        .trim_end_matches('.');
    if !host.ends_with(".ts.net")
        || host.len() > 253
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    {
        return Err("O Tailscale ainda não informou o endereço privado deste Mac.".into());
    }
    Ok(host.to_string())
}
fn check_forwarding(config: &Value) -> Result<bool, String> {
    match config.pointer("/TCP/5223") {
        None => Ok(false),
        Some(rule) if rule["TCPForward"]==TARGET && rule.get("TerminateTLS").is_none_or(|v|v.as_str().unwrap_or("").is_empty()) => Ok(true),
        _ => Err("A porta 5223 do Tailscale já está em uso por outro serviço. A configuração existente foi preservada.".into())
    }
}
fn server_address(fingerprint: &str, password: &str, host: &str) -> Result<String, String> {
    let fp = fingerprint.trim();
    if !(40..=100).contains(&fp.len())
        || !fp
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_=".contains(&b))
        || password.len() < 24
        || !password
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("A identidade do servidor SimpleX não pôde ser validada.".into());
    }
    Ok(format!("smp://{fp}:{password}@{host}:5223"))
}
impl Host {
    pub async fn status(&self) -> HostStatus {
        self.status.lock().await.clone()
    }
    pub async fn phase(&self, phase: &str, message: &str) {
        let mut s = self.status.lock().await;
        s.phase = phase.into();
        s.message = message.into();
    }
    async fn container(
        &self,
        args: &[&str],
        timeout: u64,
        context: &str,
    ) -> Result<String, String> {
        let mut c = Command::new(podman()?);
        let connection = self.connection.lock().await.clone();
        if !connection.is_empty() {
            c.args(["--connection", &connection]);
        }
        c.args(args);
        output(c, timeout, context).await
    }
    pub async fn prepare(&self) -> Result<String, String> {
        *self.status.lock().await = HostStatus::default();
        let result = self.prepare_inner().await;
        if let Err(error) = &result {
            self.phase("error", error).await;
        }
        result
    }
    async fn prepare_inner(&self) -> Result<String, String> {
        self.phase("network", "Verificando a rede privada…").await;
        let ts = tailscale()?;
        let host = hostname(&json(
            &output(
                command(&ts, &["status", "--json"]),
                20,
                "Verificação do Tailscale",
            )
            .await?,
        )?)?;
        let serve = json(
            &output(
                command(&ts, &["serve", "status", "--json"]),
                20,
                "Verificação do acesso privado",
            )
            .await?,
        )?;
        let forwarded = check_forwarding(&serve)?;
        self.phase("engine", "Preparando o motor local…").await;
        let p = podman()?;
        // Use a dedicated VM and explicit connection; never switch the user's default.
        let machines = json(
            &output(
                command(&p, &["machine", "list", "--format", "json"]),
                20,
                "Verificação do motor local",
            )
            .await?,
        )?;
        let list = machines
            .as_array()
            .ok_or("Lista inválida de ambientes locais.")?;
        let existing = list.iter().find(|v| v["Name"] == NAME);
        if existing.is_none() {
            output(
                command(
                    &p,
                    &[
                        "machine",
                        "init",
                        "--cpus",
                        "2",
                        "--memory",
                        "2048",
                        "--disk-size",
                        "10",
                        NAME,
                    ],
                ),
                600,
                "Preparação do ambiente SimpleX",
            )
            .await?;
        }
        *self.connection.lock().await = NAME.into();
        if !existing.is_some_and(|m| m["Running"] == true) {
            if list
                .iter()
                .any(|m| m["Running"] == true && m["Name"] != NAME)
            {
                return Err("Outro ambiente Podman está ligado. Desligue-o no Podman e tente novamente; o AgentSmith não interrompe seus outros ambientes.".into());
            }
            output(
                command(&p, &["machine", "start", NAME]),
                180,
                "Inicialização do ambiente SimpleX",
            )
            .await?;
        }
        self.phase("download", "Preparando o servidor oficial SimpleX…")
            .await;
        if self
            .container(
                &["image", "exists", IMAGE],
                15,
                "Verificação do servidor baixado",
            )
            .await
            .is_err()
        {
            self.container(&["pull", IMAGE], 600, "Download do servidor SimpleX")
                .await?;
        }
        let exists = self
            .container(
                &["container", "exists", NAME],
                15,
                "Verificação do servidor",
            )
            .await
            .is_ok();
        if exists {
            let inspected = json(
                &self
                    .container(
                        &["inspect", NAME],
                        20,
                        "Verificação da identidade do servidor",
                    )
                    .await?,
            )?;
            if inspected[0]["Config"]["Labels"][LABEL] != "simplex"
                || inspected[0]["Config"]["Labels"]["com.agentsmith.host"] != host
            {
                return Err("Já existe um servidor com este nome ou com outro endereço Tailscale. A configuração foi preservada.".into());
            }
            self.container(&["start", NAME], 45, "Inicialização do servidor SimpleX")
                .await?;
        } else {
            let password = format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            );
            let mut c = Command::new(&p);
            c.args([
                "--connection",
                NAME,
                "run",
                "-d",
                "--name",
                NAME,
                "--label",
                "com.agentsmith.service=simplex",
                "--label",
                &format!("com.agentsmith.host={host}"),
                "--stop-signal",
                "SIGINT",
                "--stop-timeout",
                "30",
                "-p",
                "127.0.0.1:17423:5223",
                "-v",
                "agentsmith-simplex-config:/etc/opt/simplex",
                "-v",
                "agentsmith-simplex-queues:/var/opt/simplex",
                "--env",
                "PASS",
                "--env",
                "ADDR",
                "--env",
                "WEB_MANUAL=1",
                IMAGE,
            ]);
            c.env("PASS", password).env("ADDR", &host);
            output(c, 120, "Criação do servidor SimpleX").await?;
        }
        self.phase("server", "Aguardando o servidor…").await;
        let deadline = std::time::Instant::now() + Duration::from_secs(45);
        loop {
            if tokio::net::TcpStream::connect(TARGET).await.is_ok() {
                break;
            }
            if std::time::Instant::now() > deadline {
                return Err(
                    "O servidor SimpleX não abriu a conexão local. Tente ativar novamente.".into(),
                );
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        let fp = self
            .container(
                &["exec", NAME, "cat", "/etc/opt/simplex/fingerprint"],
                15,
                "Leitura da identidade do servidor",
            )
            .await?;
        let config = self
            .container(
                &["exec", NAME, "cat", "/etc/opt/simplex/smp-server.ini"],
                15,
                "Leitura da configuração do servidor",
            )
            .await?;
        let password = config
            .lines()
            .find_map(|line| line.trim().strip_prefix("create_password = "))
            .ok_or("Senha de criação de filas não encontrada.")?;
        let address = server_address(&fp, password.trim(), &host)?;
        if !forwarded {
            output(
                command(
                    &ts,
                    &["serve", "--bg", "--tcp=5223", "tcp://127.0.0.1:17423"],
                ),
                30,
                "Publicação na rede privada",
            )
            .await?;
        }
        // Verify the resulting rule, including when it was already configured.
        check_forwarding(&json(
            &output(
                command(&ts, &["serve", "status", "--json"]),
                20,
                "Verificação do acesso privado",
            )
            .await?,
        )?)?
        .then_some(())
        .ok_or("O acesso privado do SimpleX ainda não está disponível.")?;
        *self.status.lock().await = HostStatus {
            phase: "client".into(),
            message: "Conectando o AgentSmith ao servidor…".into(),
            server_qr: qr(&address)?,
            server_address: address.clone(),
        };
        Ok(address)
    }
    pub async fn stop(&self) -> Result<(), String> {
        if !self.connection.lock().await.is_empty() {
            if self
                .container(
                    &["container", "exists", NAME],
                    15,
                    "Verificação do servidor",
                )
                .await
                .is_ok()
            {
                self.container(
                    &["stop", "--time", "30", NAME],
                    45,
                    "Encerramento do servidor SimpleX",
                )
                .await?;
            }
            // This VM belongs exclusively to this channel. Release its memory,
            // but keep its disk, queues and certificates for the next activation.
            let p = podman()?;
            let machines = json(
                &output(
                    command(&p, &["machine", "list", "--format", "json"]),
                    20,
                    "Verificação do ambiente SimpleX",
                )
                .await?,
            )?;
            if machines.as_array().is_some_and(|items| {
                items
                    .iter()
                    .any(|m| m["Name"] == NAME && m["Running"] == true)
            }) {
                output(
                    command(&p, &["machine", "stop", NAME]),
                    60,
                    "Encerramento do ambiente SimpleX",
                )
                .await?;
            }
        }
        *self.status.lock().await = HostStatus::default();
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn forwarding_never_replaces_other_services() {
        assert!(!check_forwarding(&json!({"TCP":{"443":{"HTTPS":true}}})).unwrap());
        assert!(check_forwarding(&json!({"TCP":{"5223":{"TCPForward":TARGET}}})).unwrap());
        assert!(check_forwarding(&json!({"TCP":{"5223":{"HTTPS":true}}})).is_err());
        assert!(check_forwarding(&json!({"TCP":{"5223":{"TCPForward":"localhost:80"}}})).is_err());
    }
    #[test]
    fn only_the_current_private_hostname_is_used() {
        assert_eq!(
            hostname(&json!({"BackendState":"Running","Self":{"DNSName":"mac.example.ts.net."}}))
                .unwrap(),
            "mac.example.ts.net"
        );
        for host in ["example.org", "-x.ts.net\narg", ""] {
            assert!(hostname(&json!({"BackendState":"Running","Self":{"DNSName":host}})).is_err());
        }
    }
    #[test]
    fn relay_identity_cannot_inject_uri_fields() {
        let fp = "a".repeat(44);
        let pass = "b".repeat(64);
        assert!(server_address(&fp, &pass, "mac.example.ts.net")
            .unwrap()
            .ends_with(":5223"));
        assert!(server_address(&fp, "bad@host", "mac.example.ts.net").is_err());
        assert!(server_address("bad:password", &pass, "mac.example.ts.net").is_err());
    }
}
