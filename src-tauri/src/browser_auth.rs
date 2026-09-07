//! Official clients own OAuth credentials. No token files are read or copied here.
use crate::model::Profile;
use base64::Engine;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

static ROOT: OnceLock<PathBuf> = OnceLock::new();
pub fn init(root: PathBuf) {
    let _ = ROOT.set(root);
}
fn root() -> Result<&'static PathBuf, String> {
    ROOT.get()
        .ok_or("Armazenamento de autenticação indisponível.".into())
}
fn client(vendor: &str) -> Result<(&'static str, &'static str), String> {
    match vendor {
        "openai" => Ok(("codex", "Codex")),
        "anthropic" => Ok(("claude", "Claude Code")),
        "google" => Ok(("gemini", "Gemini CLI")),
        "xai" => Ok(("grok", "Grok Build")),
        _ => Err("Este provedor não oferece login integrado.".into()),
    }
}
pub fn validate(p: &Profile, local_only: bool) -> Result<(), String> {
    client(&p.vendor)?;
    if local_only {
        return Err(
            "Login de provedor usa a nuvem e não está disponível no modo somente local.".into(),
        );
    }
    if p.base_url != format!("official://{}", p.vendor) {
        return Err("O login só pode usar o cliente oficial do provedor.".into());
    }
    if p.model.len() > 160 || p.model.contains(['\n', '\r', '\0']) {
        return Err("Modelo inválido.".into());
    }
    Ok(())
}
fn search_paths() -> Vec<PathBuf> {
    let mut paths = vec![];
    if let Ok(r) = root() {
        paths.push(r.join("components/node_modules/.bin"));
    }
    if let Some(h) = std::env::var_os("HOME") {
        let h = PathBuf::from(h);
        paths.extend([
            h.join(".local/bin"),
            h.join(".grok/bin"),
            h.join(".npm-global/bin"),
        ]);
    }
    paths.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ]);
    paths
}
fn executable(name: &str) -> Option<PathBuf> {
    search_paths()
        .into_iter()
        .map(|p| p.join(name))
        .find(|p| p.is_file())
        .or_else(|| {
            if name == "codex" {
                [
                    "/Applications/Codex.app/Contents/Resources/codex",
                    "/Applications/ChatGPT.app/Contents/Resources/codex",
                ]
                .into_iter()
                .map(PathBuf::from)
                .find(|p| p.is_file())
            } else {
                None
            }
        })
}
fn command(path: &Path, cwd: &Path) -> Command {
    use std::os::unix::process::CommandExt;
    let mut c = Command::new(path);
    c.as_std_mut().process_group(0);
    c.current_dir(cwd)
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c.env(
        "PATH",
        std::env::join_paths(search_paths()).unwrap_or_default(),
    );
    // API credentials must not silently turn a browser profile into paid API access.
    for key in [
        "OPENAI_API_KEY",
        "OPENAI_BASE_URL",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_BASE_URL",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "CLAUDE_CODE_USE_BEDROCK",
        "CLAUDE_CODE_USE_VERTEX",
        "CLAUDE_CODE_USE_FOUNDRY",
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "GOOGLE_GENAI_USE_VERTEXAI",
        "GOOGLE_GEMINI_BASE_URL",
        "XAI_API_KEY",
        "GROK_API_KEY",
    ] {
        c.env_remove(key);
    }
    c
}
fn installed(vendor: &str) -> Result<PathBuf, String> {
    let (bin, name) = client(vendor)?;
    executable(bin).ok_or(format!(
        "Instale o componente oficial {name} para continuar."
    ))
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!("agentsmith-auth-{}", uuid::Uuid::new_v4()));
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .map_err(|_| "Não foi possível preparar a sessão.")?;
        Ok(Self(path))
    }
    fn write(&self, name: &str, data: impl AsRef<[u8]>) -> Result<PathBuf, String> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let path = self.0.join(name);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|_| "Não foi possível preparar a solicitação.")?;
        f.write_all(data.as_ref())
            .map_err(|_| "Falha ao gravar solicitação.")?;
        Ok(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub installed: bool,
    pub client: String,
    pub phase: String,
    pub message: String,
}
struct Job {
    status: Status,
    abort: tokio::task::AbortHandle,
}
#[derive(Clone, Default)]
pub struct AuthManager {
    jobs: Arc<Mutex<HashMap<String, Job>>>,
}
impl AuthManager {
    pub fn status(&self, vendor: &str) -> Result<Status, String> {
        let (_, name) = client(vendor)?;
        let has = installed(vendor).is_ok();
        if let Some(job) = self.jobs.lock().unwrap().get(vendor) {
            let mut s = job.status.clone();
            s.installed = has;
            return Ok(s);
        }
        Ok(Status {
            installed: has,
            client: name.into(),
            phase: "idle".into(),
            message: if has {
                "Componente disponível. Entre na conta ou teste uma sessão já existente."
            } else {
                "O componente oficial precisa ser instalado neste Mac."
            }
            .into(),
        })
    }
    pub fn cancel(&self, vendor: &str) -> Result<(), String> {
        client(vendor)?;
        if let Some(job) = self.jobs.lock().unwrap().get_mut(vendor) {
            job.abort.abort();
            job.status.phase = "cancelled".into();
            job.status.message =
                "Operação cancelada. Você pode fechar a aba de autenticação.".into();
        }
        Ok(())
    }
    pub fn start(&self, vendor: String, install: bool) -> Result<Status, String> {
        client(&vendor)?;
        if !install {
            installed(&vendor)?;
        }
        let has_client = installed(&vendor).is_ok();
        let id = vendor.clone();
        self.start_job(vendor, install, has_client, async move {
            if install {
                install_client(&id).await
            } else {
                login(&id).await
            }
        })
    }
    fn start_job(
        &self,
        vendor: String,
        install: bool,
        has_client: bool,
        operation: impl std::future::Future<Output = Result<(), String>> + Send + 'static,
    ) -> Result<Status, String> {
        let (_, name) = client(&vendor)?;
        let mut jobs = self.jobs.lock().unwrap();
        if install && jobs.values().any(|j| j.status.phase == "installing") {
            return Err("Aguarde a instalação do outro componente terminar.".into());
        }
        if jobs
            .get(&vendor)
            .is_some_and(|j| ["connecting", "installing"].contains(&j.status.phase.as_str()))
        {
            return Err("Já existe uma operação em andamento para este provedor.".into());
        }
        let status = Status {
            installed: has_client,
            client: name.into(),
            phase: if install { "installing" } else { "connecting" }.into(),
            message: if install {
                "Instalando o componente oficial. Aguarde."
            } else {
                "Conclua a autenticação na janela do seu navegador."
            }
            .into(),
        };
        let shared = self.jobs.clone();
        let id = vendor.clone();
        // Synchronous Tauri commands run on the UI thread, outside a Tokio
        // context. Always schedule through Tauri's managed runtime.
        let task = tauri::async_runtime::spawn(async move {
            let result = tokio::time::timeout(
                Duration::from_secs(if install { 900 } else { 600 }),
                operation,
            )
            .await
            .unwrap_or_else(|_| {
                Err(
                    "Tempo esgotado. Tente novamente quando estiver pronto para concluir o login."
                        .into(),
                )
            });
            if let Some(job) = shared.lock().unwrap().get_mut(&id) {
                match result {
                    Ok(()) => {
                        job.status.phase = if install {
                            "installed"
                        } else {
                            "authenticated"
                        }
                        .into();
                        job.status.message=if install {"Componente instalado. Clique em Entrar pelo navegador."}else{"Login concluído pelo cliente oficial. Teste o modelo para verificar o acesso."}.into();
                    }
                    Err(e) => {
                        job.status.phase = "error".into();
                        job.status.message = e;
                    }
                }
            }
        });
        jobs.insert(
            vendor,
            Job {
                status: status.clone(),
                abort: task.inner().abort_handle(),
            },
        );
        Ok(status)
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

async fn bounded(mut stream: impl AsyncRead + Unpin, cap: u64) -> Result<Vec<u8>, String> {
    let mut v = vec![];
    (&mut stream)
        .take(cap + 1)
        .read_to_end(&mut v)
        .await
        .map_err(|_| "Falha ao ler o cliente oficial.")?;
    if v.len() as u64 > cap {
        Err("Resposta do cliente excedeu o limite.".into())
    } else {
        Ok(v)
    }
}
async fn process(mut c: Command, input: Vec<u8>) -> Result<Vec<u8>, String> {
    let mut child = OwnedChild::new(c.spawn().map_err(|_| {
        "Não foi possível iniciar o componente oficial. Confira a instalação e a versão."
    })?);
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let write = async move {
        stdin
            .write_all(&input)
            .await
            .map_err(|_| "O cliente encerrou a entrada.".to_string())?;
        drop(stdin);
        Ok::<_, String>(())
    };
    let (_, out, _) = tokio::try_join!(
        write,
        bounded(stdout, 16 * 1024 * 1024),
        bounded(stderr, 2 * 1024 * 1024)
    )?;
    let status = child
        .wait()
        .await
        .map_err(|_| "Falha ao aguardar o cliente.")?;
    if !status.success() {
        return Err(format!("O cliente oficial encerrou com código {}. Confira login, acesso ao modelo e limites da conta. Atualize o componente se necessário.",status.code().unwrap_or(-1)));
    }
    Ok(out)
}

struct OwnedChild {
    inner: Child,
    group: i32,
}
impl OwnedChild {
    fn new(inner: Child) -> Self {
        let group = inner.id().expect("spawned child id") as i32;
        Self { inner, group }
    }
}
impl std::ops::Deref for OwnedChild {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.inner
    }
}
impl std::ops::DerefMut for OwnedChild {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.inner
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        // Clients may relaunch a Node worker. End the group created at spawn,
        // including OAuth callback listeners, when a request is cancelled.
        unsafe {
            libc::kill(-self.group, libc::SIGKILL);
        }
        let _ = self.inner.start_kill();
    }
}
struct Rpc {
    child: OwnedChild,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next: u32,
    text: String,
    drain: tokio::task::JoinHandle<()>,
}
impl Drop for Rpc {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        self.drain.abort();
    }
}
impl Rpc {
    fn spawn(mut c: Command) -> Result<Self, String> {
        let mut child = OwnedChild::new(
            c.spawn()
                .map_err(|_| "Não foi possível iniciar o cliente oficial.")?,
        );
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut stderr = child.stderr.take().unwrap();
        let drain = tokio::spawn(async move {
            let mut sink = tokio::io::sink();
            let _ = tokio::io::copy(&mut stderr, &mut sink).await;
        });
        Ok(Self {
            child,
            input,
            output,
            next: 1,
            text: String::new(),
            drain,
        })
    }
    async fn send(&mut self, v: Value) -> Result<(), String> {
        self.input
            .write_all(format!("{v}\n").as_bytes())
            .await
            .map_err(|_| "O cliente encerrou a sessão.".into())
    }
    async fn read(&mut self) -> Result<Value, String> {
        loop {
            let mut bytes = vec![];
            let n = (&mut self.output)
                .take(16 * 1024 * 1024 + 1)
                .read_until(b'\n', &mut bytes)
                .await
                .map_err(|_| "Falha ao receber evento do cliente.")?;
            if n == 0 {
                return Err("O cliente encerrou a sessão. Confira a instalação e o login.".into());
            }
            if n > 16 * 1024 * 1024 {
                return Err("Evento do cliente excedeu o limite.".into());
            }
            let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
                continue;
            };
            if v.get("method").is_some() && v.get("id").is_some() {
                let answer = if v["method"] == "session/request_permission" {
                    json!({"jsonrpc":"2.0","id":v["id"],"result":{"outcome":{"outcome":"cancelled"}}})
                } else {
                    json!({"jsonrpc":"2.0","id":v["id"],"error":{"code":-32601,"message":"AgentSmith does not expose local tools"}})
                };
                self.send(answer).await?;
                continue;
            }
            let u = &v["params"]["update"];
            if u["sessionUpdate"] == "agent_message_chunk" {
                if let Some(t) = u["content"]["text"].as_str() {
                    self.text.push_str(t);
                }
                if self.text.len() > 2 * 1024 * 1024 {
                    return Err("Resposta do modelo excedeu o limite.".into());
                }
            }
            return Ok(v);
        }
    }
    async fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next;
        self.next += 1;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await?;
        loop {
            let v = self.read().await?;
            if v["id"] == id {
                if v.get("error").is_some() {
                    return Err(format!("O cliente recusou {method}. Confira login, modelo, cota e versão do componente."));
                }
                return Ok(v["result"].clone());
            }
        }
    }
}
fn open_auth_url(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "URL de login inválida.")?;
    if parsed.scheme() != "https"
        || !matches!(
            parsed.host_str(),
            Some("auth.openai.com" | "auth0.openai.com" | "login.openai.com" | "chatgpt.com")
        )
    {
        return Err("O cliente retornou um endereço de login inesperado.".into());
    }
    std::process::Command::new("/usr/bin/open")
        .arg(url)
        .spawn()
        .map_err(|_| "Não foi possível abrir o navegador.")?;
    Ok(())
}
fn gemini_settings(scratch: &Scratch) -> Result<PathBuf, String> {
    scratch.write("gemini-settings.json", gemini_config(false).to_string())
}
fn gemini_config(ready: bool) -> Value {
    // Defer OAuth until ACP is initialized; otherwise the CLI blocks before
    // returning its capabilities on a Mac that has not signed in yet.
    json!({"tools":{"core":["agentsmith_no_local_tools"]},"mcp":{"allowed":["agentsmith_no_mcp"]},"hooksConfig":{"enabled":false},"security":{"auth":{"selectedType":if ready {Some("oauth-personal")}else{None},"enforcedType":"oauth-personal","useExternal":true}},"advanced":{"ignoreLocalEnv":true},"context":{"fileName":"AGENTSMITH_UNUSED_CONTEXT.md"},"general":{"enableAutoUpdate":false},"telemetry":{"enabled":false}})
}
async fn acp(vendor: &str, scratch: &Scratch) -> Result<(Rpc, Value), String> {
    let mut c = command(&installed(vendor)?, &scratch.0);
    if vendor == "google" {
        c.args(["--experimental-acp", "--extensions", "none"])
            .env("GEMINI_CLI_SYSTEM_SETTINGS_PATH", gemini_settings(scratch)?);
    } else {
        c.args([
            "--tools",
            "",
            "--deny",
            "*",
            "--permission-mode",
            "dontAsk",
            "--no-subagents",
            "--no-memory",
            "--disable-web-search",
            "agent",
            "--no-leader",
            "stdio",
        ]);
    }
    let mut rpc = Rpc::spawn(c)?;
    let init=rpc.request("initialize",json!({"protocolVersion":1,"clientInfo":{"name":"AgentSmith","version":"0.2.0"},"clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false}})).await?;
    if vendor == "google" {
        std::fs::write(
            scratch.0.join("gemini-settings.json"),
            gemini_config(true).to_string(),
        )
        .map_err(|_| "Não foi possível preparar a autenticação Google.")?;
    }
    Ok((rpc, init))
}
async fn login(vendor: &str) -> Result<(), String> {
    let scratch = Scratch::new()?;
    match vendor {
        "openai" => {
            let mut c = command(&installed(vendor)?, &scratch.0);
            c.args(["app-server", "--listen", "stdio://"]);
            let mut rpc = Rpc::spawn(c)?;
            rpc.request(
                "initialize",
                json!({"clientInfo":{"name":"agentsmith","version":"0.2.0"}}),
            )
            .await?;
            rpc.send(json!({"method":"initialized"})).await?;
            let result = rpc
                .request("account/login/start", json!({"type":"chatgpt"}))
                .await?;
            open_auth_url(
                result["authUrl"]
                    .as_str()
                    .ok_or("O Codex não retornou o endereço de login.")?,
            )?;
            loop {
                let v = rpc.read().await?;
                if v["method"] == "account/login/completed"
                    && v["params"]["loginId"] == result["loginId"]
                {
                    return if v["params"]["success"] == true {
                        Ok(())
                    } else {
                        Err("O login não foi concluído. Tente novamente.".into())
                    };
                }
            }
        }
        "google" => {
            let (mut rpc, init) = acp(vendor, &scratch).await?;
            if !init["authMethods"]
                .as_array()
                .is_some_and(|a| a.iter().any(|m| m["id"] == "oauth-personal"))
            {
                return Err(
                    "Atualize o Gemini CLI: autenticação Google não foi anunciada pelo componente."
                        .into(),
                );
            }
            rpc.request("authenticate", json!({"methodId":"oauth-personal"}))
                .await?;
            Ok(())
        }
        "anthropic" | "xai" => {
            let mut c = command(&installed(vendor)?, &scratch.0);
            if vendor == "anthropic" {
                c.args(["auth", "login", "--claudeai"]);
            } else {
                c.args(["login", "--oauth"]);
            }
            process(c, vec![]).await.map(|_| ())
        }
        _ => Err("Provedor inválido.".into()),
    }
}
async fn install_client(vendor: &str) -> Result<(), String> {
    let package = match vendor {
        "openai" => "@openai/codex",
        "anthropic" => "@anthropic-ai/claude-code",
        "google" => "@google/gemini-cli",
        "xai" => "@xai-official/grok",
        _ => return Err("Provedor inválido.".into()),
    };
    let npm =
        executable("npm").ok_or("Instale o Node.js LTS para instalar os componentes oficiais.")?;
    let dir = root()?.join("components");
    std::fs::create_dir_all(&dir).map_err(|_| "Não foi possível preparar a instalação.")?;
    let mut c = command(&npm, &dir);
    c.args(["install", "--prefix"])
        .arg(&dir)
        .args(["--no-audit", "--no-fund", package]);
    process(c, vec![]).await?;
    installed(vendor).map(|_| ())
}
fn png_data(image: Option<&str>) -> Result<Option<&str>, String> {
    image
        .map(|s| {
            s.strip_prefix("data:image/png;base64,")
                .ok_or("A captura deve ser PNG.".into())
        })
        .transpose()
}
pub async fn generate(
    p: &Profile,
    system: &str,
    prompt: &str,
    image: Option<&str>,
) -> Result<String, String> {
    if image.is_some() && !p.vision {
        return Err("Habilite visão neste perfil para operar a tela remota.".into());
    }
    tokio::time::timeout(
        Duration::from_secs(180),
        generate_inner(p, system, prompt, image),
    )
    .await
    .map_err(|_| {
        "O cliente não respondeu em 180 segundos. Confira sua conexão e os limites da conta."
            .to_string()
    })?
}
async fn generate_inner(
    p: &Profile,
    system: &str,
    prompt: &str,
    image: Option<&str>,
) -> Result<String, String> {
    let scratch = Scratch::new()?;
    let data = png_data(image)?;
    let full=format!("{system}\n\nVocê é o componente de raciocínio do AgentSmith. Use apenas o texto e a imagem fornecidos. Não execute ferramentas locais. Retorne somente a resposta solicitada.\n\n{prompt}");
    let custom = !p.model.trim().is_empty() && p.model != "default";
    match p.vendor.as_str() {
        "openai" => {
            let mut check = command(&installed(&p.vendor)?, &scratch.0);
            check.args(["app-server", "--listen", "stdio://"]);
            let mut auth = Rpc::spawn(check)?;
            auth.request(
                "initialize",
                json!({"clientInfo":{"name":"agentsmith","version":"0.2.0"}}),
            )
            .await?;
            auth.send(json!({"method":"initialized"})).await?;
            let account = auth
                .request("account/read", json!({"refreshToken":false}))
                .await?;
            if account["account"]["type"] != "chatgpt" {
                return Err("Entre com ChatGPT pelo navegador antes de usar este perfil. Uma API key salva no Codex não será usada.".into());
            }
            drop(auth);
            let mut c = command(&installed(&p.vendor)?, &scratch.0);
            c.args([
                "exec",
                "--ignore-user-config",
                "--ignore-rules",
                "--skip-git-repo-check",
                "--ephemeral",
                "--sandbox",
                "read-only",
                "-c",
                "features.shell_tool=false",
                "-c",
                "features.unified_exec=false",
                "-c",
                "web_search=\"disabled\"",
                "--json",
                "--output-last-message",
            ])
            .arg(scratch.0.join("answer.txt"));
            if custom {
                c.arg("--model").arg(&p.model);
            }
            if let Some(data) = data {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .map_err(|_| "Imagem inválida.")?;
                c.arg("--image").arg(scratch.write("screen.png", bytes)?);
            }
            c.arg("-");
            process(c, full.into_bytes()).await?;
            let text = std::fs::read_to_string(scratch.0.join("answer.txt"))
                .map_err(|_| "O Codex não produziu uma resposta final.")?;
            nonempty(text)
        }
        "anthropic" => {
            let mut check = command(&installed(&p.vendor)?, &scratch.0);
            check.args(["auth", "status"]);
            let account: Value =
                serde_json::from_slice(&process(check, vec![]).await.map_err(|_| {
                    "Entre no Claude Code pelo navegador antes de testar este perfil."
                })?)
                .map_err(|_| "Atualize o Claude Code para verificar a autenticação.")?;
            if account["loggedIn"] != true || account["authMethod"] != "claude.ai" {
                return Err("Entre com sua conta Claude pelo navegador. Este perfil não usa API key nem autenticação do Console.".into());
            }
            let mut c = command(&installed(&p.vendor)?, &scratch.0);
            c.args([
                "-p",
                "--safe-mode",
                "--tools",
                "",
                "--disallowedTools",
                "mcp__*",
                "--strict-mcp-config",
                "--mcp-config",
                "{\"mcpServers\":{}}",
                "--no-session-persistence",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--verbose",
            ]);
            if custom {
                c.arg("--model").arg(&p.model);
            }
            let mut content = vec![json!({"type":"text","text":full})];
            if let Some(data) = data {
                content.push(json!({"type":"image","source":{"type":"base64","media_type":"image/png","data":data}}));
            }
            let payload = json!({"type":"user","message":{"role":"user","content":content}});
            let out = process(c, format!("{payload}\n").into_bytes()).await?;
            parse_claude(&out)
        }
        "google" | "xai" => {
            let (mut rpc, init) = acp(&p.vendor, &scratch).await?;
            if image.is_some() && init["agentCapabilities"]["promptCapabilities"]["image"] != true {
                return Err("Este cliente/modelo não anunciou suporte a imagens. Use outro perfil para operação e verificação.".into());
            }
            if p.vendor == "xai" {
                if !init["authMethods"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|m| m["id"] == "cached_token"))
                {
                    return Err(
                        "Entre no Grok Build pelo navegador antes de testar este perfil.".into(),
                    );
                }
                rpc.request(
                    "authenticate",
                    json!({"methodId":"cached_token","_meta":{"headless":true}}),
                )
                .await?;
            }
            // Gemini loads only OAuth-personal. With no cached login it returns an auth-required error.
            let session = rpc
                .request("session/new", json!({"cwd":scratch.0,"mcpServers":[]}))
                .await?;
            let sid = &session["sessionId"];
            if custom {
                rpc.request(
                    "session/set_model",
                    json!({"sessionId":sid,"modelId":p.model}),
                )
                .await?;
            }
            let mut parts = vec![json!({"type":"text","text":full})];
            if let Some(data) = data {
                parts.push(json!({"type":"image","mimeType":"image/png","data":data}));
            }
            let params = acp_prompt(&p.vendor, sid, parts, system);
            let structured = params["_meta"]["outputSchema"].is_object();
            let result = rpc.request("session/prompt", params).await?;
            acp_answer(&result, &rpc.text, structured)
        }
        _ => Err("Provedor inválido.".into()),
    }
}
// Grok Build's own headless client uses outputSchema on PromptRequest and
// reads the validated result from PromptResponse._meta, not streamed prose.
fn acp_prompt(vendor: &str, sid: &Value, parts: Vec<Value>, system: &str) -> Value {
    let mut params = json!({"sessionId":sid,"prompt":parts});
    if vendor == "xai" {
        if let Some(schema) = crate::harness::output_schema(system) {
            params["_meta"] = json!({"outputSchema":schema,"screenMode":"headless"});
        }
    }
    params
}
fn acp_answer(result: &Value, text: &str, structured: bool) -> Result<String, String> {
    if result["stopReason"] != "end_turn" {
        return Err("O agente interrompeu a resposta. Nenhuma ação será executada.".into());
    }
    if structured {
        let meta = &result["_meta"];
        if meta.get("structuredOutputError").is_some() {
            return Err("Grok Build não conseguiu validar a resposta no contrato solicitado. Nenhuma ação foi executada.".into());
        }
        if let Some(value) = meta.get("structuredOutput").filter(|v| v.is_object()) {
            return Ok(value.to_string());
        }
        return Err("Grok Build não retornou a saída estruturada solicitada. Atualize o componente oficial e teste novamente. Nenhuma ação foi executada.".into());
    }
    nonempty(text.into())
}
fn nonempty(text: String) -> Result<String, String> {
    if text.trim().is_empty() {
        Err("O cliente não retornou uma resposta final.".into())
    } else {
        Ok(text)
    }
}
fn parse_claude(bytes: &[u8]) -> Result<String, String> {
    let invalid = "O Claude Code retornou um formato inesperado. Atualize o componente.";
    // stream-json emits system, assistant and tool events before its final result.
    // Only that result may reach the harness; intermediate messages are not actions.
    let result: Value = if let Ok(single) = serde_json::from_slice::<Value>(bytes) {
        single
    } else {
        let mut final_result = None;
        for line in bytes
            .split(|b| *b == b'\n')
            .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        {
            let event: Value = serde_json::from_slice(line).map_err(|_| invalid)?;
            if event["type"] == "result" {
                if final_result.is_some() {
                    return Err(invalid.into());
                }
                final_result = Some(event);
            }
        }
        final_result.ok_or("O Claude Code não retornou uma resposta final.")?
    };
    if result.get("type").is_some_and(|kind| kind != "result") {
        return Err("O Claude Code não retornou uma resposta final.".into());
    }
    if result["is_error"] == true || result["subtype"].as_str().is_some_and(|s| s != "success") {
        return Err(
            "O Claude Code não concluiu a resposta. Confira login, modelo e limites.".into(),
        );
    }
    nonempty(result["result"].as_str().unwrap_or_default().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grok_schema_metadata_is_scoped_and_takes_priority_over_prose() {
        let system = crate::harness::system("text-action");
        let p = acp_prompt("xai", &json!("s"), vec![], &system);
        assert!(p["_meta"]["outputSchema"]["anyOf"].is_array());
        assert!(acp_prompt("google", &json!("s"), vec![], &system)
            .get("_meta")
            .is_none());
        assert!(acp_prompt("xai", &json!("s"), vec![], "connection test")
            .get("_meta")
            .is_none());
        let result = json!({"stopReason":"end_turn","_meta":{"structuredOutput":{"kind":"click","target":0}}});
        let text = acp_answer(&result, "Let me explain this action...", true).unwrap();
        let decision: crate::observation::Decision = serde_json::from_str(&text).unwrap();
        assert!(matches!(
            decision,
            crate::observation::Decision::Click { target: 0 }
        ));
        assert!(acp_answer(
            &json!({"stopReason":"end_turn"}),
            "{\"kind\":\"click\",\"target\":0}",
            true
        )
        .is_err());
        assert!(acp_answer(&json!({"stopReason":"end_turn","_meta":{"structuredOutputError":"invalid","structuredOutput":{"kind":"click","target":0}}}), "", true).is_err());
        assert!(acp_answer(&json!({"stopReason":"cancelled","_meta":{"structuredOutput":{"kind":"click","target":0}}}), "", true).is_err());
        assert_eq!(
            acp_answer(&json!({"stopReason":"end_turn"}), "ok", false).unwrap(),
            "ok"
        );
    }

    #[test]
    fn browser_jobs_start_and_cancel_without_a_tokio_context() {
        // Mirrors the native button callback, not an async test/runtime thread.
        assert!(tokio::runtime::Handle::try_current().is_err());
        struct OnDrop(std::sync::mpsc::Sender<()>);
        impl Drop for OnDrop {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        let manager = AuthManager::default();
        for vendor in ["openai", "anthropic", "google", "xai"] {
            for install in [false, true] {
                let (started_tx, started_rx) = std::sync::mpsc::channel();
                let (dropped_tx, dropped_rx) = std::sync::mpsc::channel();
                let guard = OnDrop(dropped_tx);
                let state = manager
                    .start_job(vendor.into(), install, true, async move {
                        let _guard = guard;
                        // Exercises a real timer on the managed runtime without
                        // opening a browser, installing a client or reading accounts.
                        tokio::time::sleep(Duration::from_millis(1)).await;
                        started_tx.send(()).unwrap();
                        std::future::pending::<Result<(), String>>().await
                    })
                    .unwrap();
                assert_eq!(
                    state.phase,
                    if install { "installing" } else { "connecting" }
                );
                started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
                assert!(manager
                    .start_job(vendor.into(), install, true, async { Ok(()) })
                    .is_err());
                manager.cancel(vendor).unwrap();
                dropped_rx.recv_timeout(Duration::from_secs(3)).unwrap();
                assert_eq!(manager.status(vendor).unwrap().phase, "cancelled");
            }
        }
    }

    #[test]
    fn browser_profiles_cannot_redirect_or_run_locally() {
        let mut p:Profile=serde_json::from_value(json!({"id":uuid::Uuid::new_v4(),"vendor":"openai","name":"test","protocol":"responses","baseUrl":"official://openai","model":"default","vision":true,"enabled":true,"authMethod":"browser"})).unwrap();
        assert!(validate(&p, false).is_ok());
        assert!(validate(&p, true).is_err());
        p.base_url = "https://attacker.test".into();
        assert!(validate(&p, false).is_err());
        p.base_url = "official://openai".into();
        p.vendor = "local".into();
        assert!(validate(&p, false).is_err());
    }
    #[test]
    fn legacy_profiles_keep_api_auth() {
        let p:Profile=serde_json::from_value(json!({"id":"test","vendor":"openai","name":"test","protocol":"responses","baseUrl":"https://api.openai.com/v1","model":"test","vision":true,"enabled":true})).unwrap();
        assert_eq!(p.auth_method, "api_key");
    }
    #[test]
    fn claude_errors_and_empty_results_are_rejected() {
        assert_eq!(
            parse_claude(br#"{"subtype":"success","result":"ok","is_error":false}"#).unwrap(),
            "ok"
        );
        assert!(parse_claude(br#"{"subtype":"error_max_turns","result":"do something"}"#).is_err());
        assert!(parse_claude(br#"{"is_error":true,"result":"do something"}"#).is_err());
        assert!(parse_claude(br#"{"result":""}"#).is_err());
    }
    #[test]
    fn claude_stream_uses_only_the_final_result() {
        let stream = concat!(
            "{\"type\":\"system\",\"subtype\":\"init\"}\n",
            "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"intermediate action\"}]}}\n",
            "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"OK\"}\n"
        );
        assert_eq!(parse_claude(stream.as_bytes()).unwrap(), "OK");
        assert!(parse_claude(br#"{"type":"assistant","result":"unsafe"}"#).is_err());
        assert!(parse_claude(format!("{stream}{stream}").as_bytes()).is_err());
        assert!(parse_claude(b"{\"type\":\"system\"}\n{\"type\":\"assistant\"}\n").is_err());
        assert!(parse_claude(b"{\"type\":\"system\"}\n{\"type\":\"result\",\"subtype\":\"error_max_turns\",\"result\":\"unsafe\"}\n").is_err());
        assert!(parse_claude(format!("{stream}not-json").as_bytes()).is_err());
    }

    #[tokio::test]
    async fn process_drains_both_streams_and_rejects_nonzero() {
        let s = Scratch::new().unwrap();
        let mut c = command(Path::new("/bin/cat"), &s.0);
        assert_eq!(process(c, vec![b'x'; 100000]).await.unwrap().len(), 100000);
        c = command(Path::new("/usr/bin/false"), &s.0);
        assert!(process(c, vec![]).await.is_err());
    }
    #[tokio::test]
    async fn acp_transport_collects_text_and_refuses_local_permissions() {
        let s = Scratch::new().unwrap();
        let script=s.write("fake.py",br#"import json,sys
req=json.loads(sys.stdin.readline())
assert req['method']=='session/prompt'
print(json.dumps({'jsonrpc':'2.0','id':99,'method':'session/request_permission','params':{}}),flush=True)
reply=json.loads(sys.stdin.readline())
assert reply['result']['outcome']['outcome']=='cancelled'
print(json.dumps({'jsonrpc':'2.0','id':100,'method':'fs/read_text_file','params':{'path':'/private'}}),flush=True)
reply=json.loads(sys.stdin.readline())
assert reply['error']['code']==-32601
for part in ['{"kind":','"wait"}']:
 print(json.dumps({'method':'session/update','params':{'update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':part}}}}),flush=True)
print(json.dumps({'jsonrpc':'2.0','id':req['id'],'result':{'stopReason':'end_turn'}}),flush=True)
"#).unwrap();
        let mut c = command(Path::new("/usr/bin/python3"), &s.0);
        c.arg(script);
        let mut rpc = Rpc::spawn(c).unwrap();
        let v = rpc
            .request("session/prompt", json!({"sessionId":"test","prompt":[]}))
            .await
            .unwrap();
        assert_eq!(v["stopReason"], "end_turn");
        assert_eq!(rpc.text, "{\"kind\":\"wait\"}");
    }
    #[tokio::test]
    async fn cancelled_rpc_kills_the_owned_client() {
        let s = Scratch::new().unwrap();
        let mut c = command(Path::new("/bin/sleep"), &s.0);
        c.arg("30");
        let rpc = Rpc::spawn(c).unwrap();
        let pid = rpc.child.id().unwrap();
        drop(rpc);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let alive = std::process::Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success();
        assert!(!alive);
    }
}
