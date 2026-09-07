use crate::model::Profile;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::io::AsyncWriteExt;

#[derive(Clone, Deserialize, Serialize)]
pub struct Asset {
    name: String,
    bytes: u64,
    sha256: String,
    url: String,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    description: String,
    source: String,
    license: String,
    files: Vec<Asset>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    #[serde(flatten)]
    model: Model,
    installed: bool,
    download_bytes: u64,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Download {
    model_id: String,
    phase: String,
    received: u64,
    total: u64,
    message: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    engine_available: bool,
    models: Vec<ModelStatus>,
    download: Download,
    active_model: String,
    working: bool,
}
struct Job {
    progress: Download,
    abort: Option<tokio::task::AbortHandle>,
}
struct Server {
    child: Child,
    model: String,
    url: String,
    token: String,
    used: Instant,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        ACTIVE_PID.store(0, std::sync::atomic::Ordering::SeqCst);
    }
}
pub struct Engine {
    root: PathBuf,
    binary: PathBuf,
    job: Mutex<Job>,
    server: tokio::sync::Mutex<Option<Server>>,
}
static ACTIVE_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static ENGINE: OnceLock<Arc<Engine>> = OnceLock::new();
pub fn init(root: PathBuf, binary: PathBuf) {
    let engine = Arc::new(Engine {
        root: root.join("local-vision"),
        binary,
        job: Mutex::new(Job {
            progress: Download::default(),
            abort: None,
        }),
        server: tokio::sync::Mutex::new(None),
    });
    let _ = ENGINE.set(engine.clone());
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(15)).await;
            if let Ok(mut slot) = engine.server.try_lock() {
                if slot.as_mut().is_some_and(|s| {
                    s.used.elapsed() > Duration::from_secs(120)
                        || s.child.try_wait().ok().flatten().is_some()
                }) {
                    slot.take();
                }
            }
        }
    });
}
fn engine() -> Result<&'static Arc<Engine>, String> {
    ENGINE.get().ok_or("Motor local indisponível.".into())
}
fn catalog() -> Vec<Model> {
    serde_json::from_str(include_str!("local_models.json")).expect("Catálogo local inválido")
}
fn model(id: &str) -> Result<Model, String> {
    catalog()
        .into_iter()
        .find(|m| m.id == id)
        .ok_or("Escolha um modelo do catálogo local.".into())
}
pub fn validate(p: &Profile) -> Result<(), String> {
    if p.vendor != "builtin"
        || p.auth_method != "local_engine"
        || p.base_url != "agentsmith://local"
        || p.protocol != "chat"
    {
        return Err("Perfil do motor local inválido.".into());
    }
    model(&p.model)?;
    Ok(())
}
impl Engine {
    fn installed(&self, m: &Model) -> bool {
        m.files.iter().all(|f| {
            std::fs::metadata(self.root.join(&m.id).join(&f.name))
                .is_ok_and(|s| s.is_file() && s.len() == f.bytes)
        })
    }
}
pub fn status() -> Result<Status, String> {
    let e = engine()?;
    let (active_model, working) = match e.server.try_lock() {
        Ok(mut slot) => {
            if slot
                .as_mut()
                .is_some_and(|s| s.child.try_wait().ok().flatten().is_some())
            {
                slot.take();
            }
            (
                slot.as_ref().map(|s| s.model.clone()).unwrap_or_default(),
                false,
            )
        }
        Err(_) => (String::new(), true),
    };
    Ok(Status {
        engine_available: e.binary.is_file(),
        models: catalog()
            .into_iter()
            .map(|m| ModelStatus {
                installed: e.installed(&m),
                download_bytes: m.files.iter().map(|f| f.bytes).sum(),
                model: m,
            })
            .collect(),
        download: e.job.lock().unwrap().progress.clone(),
        active_model,
        working,
    })
}
struct Partial(PathBuf);
impl Drop for Partial {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub fn download(id: String) -> Result<(), String> {
    let e = engine()?.clone();
    let m = model(&id)?;
    if !e.binary.is_file() {
        return Err("O motor local não está incluído nesta versão do aplicativo.".into());
    }
    let mut job = e.job.lock().unwrap();
    if job.abort.as_ref().is_some_and(|a| !a.is_finished()) {
        return Err("Aguarde ou cancele o download atual.".into());
    }
    if e.installed(&m) {
        return Ok(());
    }
    job.progress = Download {
        model_id: id,
        phase: "downloading".into(),
        total: m.files.iter().map(|f| f.bytes).sum(),
        message: "Baixando arquivos oficiais…".into(),
        ..Default::default()
    };
    let task_engine = e.clone();
    let task = tauri::async_runtime::spawn(async move {
        let result = fetch(&task_engine, &m).await;
        let mut j = task_engine.job.lock().unwrap();
        match result {
            Ok(()) => {
                j.progress.phase = "completed".into();
                j.progress.message = "Download concluído e integridade verificada.".into();
            }
            Err(err) => {
                j.progress.phase = "error".into();
                j.progress.message = err;
            }
        }
    });
    job.abort = Some(task.inner().abort_handle());
    Ok(())
}
async fn fetch(e: &Engine, m: &Model) -> Result<(), String> {
    let dir = e.root.join(&m.id);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|_| "Não foi possível criar a pasta do modelo.")?;
    let client = reqwest::Client::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(60))
        .build()
        .map_err(|_| "Não foi possível iniciar o download.")?;
    for a in &m.files {
        let path = dir.join(&a.name);
        if verify_file(&path, a).await.is_ok() {
            e.job.lock().unwrap().progress.received += a.bytes;
            continue;
        }
        let partial = Partial(path.with_extension("gguf.part"));
        let mut file = tokio::fs::File::create(&partial.0)
            .await
            .map_err(|_| "Não foi possível salvar o download.")?;
        let mut response = client
            .get(&a.url)
            .send()
            .await
            .map_err(|_| "Download indisponível. Confira a internet e tente novamente.")?;
        if !response.status().is_success() {
            return Err(format!(
                "Download retornou HTTP {}.",
                response.status().as_u16()
            ));
        }
        let mut hasher = Sha256::new();
        let mut received = 0u64;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "O download foi interrompido. Tente novamente.")?
        {
            received += chunk.len() as u64;
            if received > a.bytes {
                return Err("O arquivo recebido excede o tamanho esperado.".into());
            }
            hasher.update(&chunk);
            file.write_all(&chunk)
                .await
                .map_err(|_| "Sem espaço ou falha ao salvar o modelo.")?;
            e.job.lock().unwrap().progress.received += chunk.len() as u64;
        }
        check_digest(received, &format!("{:x}", hasher.finalize()), a)?;
        file.sync_all()
            .await
            .map_err(|_| "Falha ao concluir o arquivo do modelo.")?;
        drop(file);
        tokio::fs::rename(&partial.0, &path)
            .await
            .map_err(|_| "Falha ao concluir a instalação.")?;
    }
    Ok(())
}
fn check_digest(bytes: u64, digest: &str, a: &Asset) -> Result<(), String> {
    if bytes != a.bytes || digest != a.sha256 {
        Err("A integridade do download não confere. Tente baixar novamente.".into())
    } else {
        Ok(())
    }
}
async fn verify_file(path: &PathBuf, a: &Asset) -> Result<(), String> {
    use tokio::io::AsyncReadExt;
    let mut f = tokio::fs::File::open(path)
        .await
        .map_err(|_| "Modelo não baixado.")?;
    if f.metadata()
        .await
        .map_err(|_| "Modelo indisponível.")?
        .len()
        != a.bytes
    {
        return Err("Arquivo incompleto.".into());
    }
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut size = 0;
    loop {
        let n = f
            .read(&mut buf)
            .await
            .map_err(|_| "Falha ao ler o modelo.")?;
        if n == 0 {
            break;
        }
        size += n as u64;
        h.update(&buf[..n]);
    }
    check_digest(size, &format!("{:x}", h.finalize()), a)
}
pub fn cancel_download() -> Result<(), String> {
    let e = engine()?;
    let mut j = e.job.lock().unwrap();
    if let Some(a) = j.abort.as_ref() {
        a.abort();
    }
    j.progress.phase = "cancelled".into();
    j.progress.message = "Download cancelado. Arquivos concluídos serão reaproveitados.".into();
    Ok(())
}
pub fn stop() -> Result<(), String> {
    let mut slot = engine()?
        .server
        .try_lock()
        .map_err(|_| "O modelo está respondendo. Aguarde a resposta para liberar a memória.")?;
    slot.take();
    Ok(())
}
pub fn shutdown() {
    let pid = ACTIVE_PID.load(std::sync::atomic::Ordering::SeqCst);
    if pid > 0 {
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
    }
    if let Ok(e) = engine() {
        if let Some(a) = e.job.lock().unwrap().abort.take() {
            a.abort();
        }
        if let Ok(mut s) = e.server.try_lock() {
            s.take();
        }
    }
}
pub async fn remove(id: &str) -> Result<(), String> {
    let e = engine()?;
    let m = model(id)?;
    let j = e.job.lock().unwrap();
    if j.abort.as_ref().is_some_and(|a| !a.is_finished()) {
        return Err("Aguarde o encerramento do download antes de remover arquivos.".into());
    }
    drop(j);
    let mut slot = e
        .server
        .try_lock()
        .map_err(|_| "Aguarde o modelo terminar de responder.")?;
    if slot.as_ref().is_some_and(|s| s.model == id) {
        slot.take();
    }
    let path = e.root.join(m.id);
    if path.exists() {
        std::fs::remove_dir_all(path).map_err(|_| "Não foi possível remover o modelo.")?;
    }
    Ok(())
}
fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|_| "Falha ao iniciar a conexão local.".into())
}
async fn start(e: &Engine, m: &Model) -> Result<Server, String> {
    if !e.installed(m) {
        return Err("Baixe este modelo na área Visão local do AgentSmith.".into());
    }
    for a in &m.files {
        verify_file(&e.root.join(&m.id).join(&a.name), a).await?;
    }
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|_| "Não foi possível reservar a conexão local.")?
        .local_addr()
        .map_err(|_| "Porta local indisponível.")?
        .port();
    let token = uuid::Uuid::new_v4().to_string();
    let dir = e.root.join(&m.id);
    let mut cmd = Command::new(&e.binary);
    cmd.current_dir(&e.root)
        .env_clear()
        .env("HOME", &e.root)
        .env("PATH", "/usr/bin:/bin")
        .env("LLAMA_API_KEY", &token)
        .args([
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--model",
        ])
        .arg(dir.join(&m.files[0].name))
        .arg("--mmproj")
        .arg(dir.join(&m.files[1].name))
        .args([
            "--alias",
            &m.id,
            "--ctx-size",
            "8192",
            "--parallel",
            "1",
            "--threads",
            "4",
            "--threads-http",
            "2",
            "--gpu-layers",
            "99",
            "--cache-ram",
            "0",
            "--image-max-tokens",
            "1024",
            "--offline",
            "--no-webui",
            "--log-disable",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = cmd
        .spawn()
        .map_err(|_| "Não foi possível iniciar o motor de visão incluído no app.")?;
    ACTIVE_PID.store(child.id(), std::sync::atomic::Ordering::SeqCst);
    let mut server = Server {
        child,
        model: m.id.clone(),
        url: format!("http://127.0.0.1:{port}"),
        token,
        used: Instant::now(),
    };
    let client = http_client()?;
    for _ in 0..240 {
        if server
            .child
            .try_wait()
            .map_err(|_| "Falha no motor local.")?
            .is_some()
        {
            return Err(
                "O motor encerrou ao carregar o modelo. Libere memória e tente novamente.".into(),
            );
        }
        if let Ok(r) = client
            .get(format!("{}/v1/models", server.url))
            .bearer_auth(&server.token)
            .timeout(Duration::from_secs(1))
            .send()
            .await
        {
            if r.status().is_success() {
                if let Ok(v) = r.json::<Value>().await {
                    if v["data"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|v| v["id"] == m.id))
                    {
                        return Ok(server);
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err("O modelo demorou demais para carregar. Libere memória e tente novamente.".into())
}
pub async fn generate(
    p: &Profile,
    system: &str,
    prompt: &str,
    image: Option<&str>,
) -> Result<String, String> {
    validate(p)?;
    let m = model(&p.model)?;
    let e = engine()?;
    let mut slot = e.server.lock().await;
    if slot
        .as_mut()
        .is_some_and(|s| s.model != m.id || s.child.try_wait().ok().flatten().is_some())
    {
        slot.take();
    }
    if slot.is_none() {
        *slot = Some(start(e, &m).await?);
    }
    // Own the child across awaits so cancellation also stops pending inference.
    let mut s = slot.take().unwrap();
    let (_, mut body) = crate::llm::request_body(p, system, prompt, image)?;
    body["max_tokens"] = json!(if image.is_some() || system.contains("[compact-output]") {
        512
    } else {
        4096
    });
    body["temperature"] = json!(0.1);
    body["cache_prompt"] = json!(false);
    let result=async {
        let res=http_client()?.post(format!("{}/v1/chat/completions",s.url)).bearer_auth(&s.token).json(&body).send().await.map_err(|_|"O modelo local não respondeu a tempo. Reduza a imagem ou libere memória.")?;
        if !res.status().is_success() {return Err(format!("Motor local: HTTP {}. A imagem e o roteiro podem exceder a capacidade deste modelo.",res.status().as_u16()));}
        let v=res.json::<Value>().await.map_err(|_|"Resposta local inválida.")?;
        crate::llm::response_text("chat",&v)
    }.await;
    s.used = Instant::now();
    if result.is_ok() {
        *slot = Some(s);
    }
    result
}
pub fn profile(id: &str) -> Result<Profile, String> {
    let m = model(id)?;
    Ok(Profile {
        id: uuid::Uuid::new_v4().to_string(),
        vendor: "builtin".into(),
        name: m.name,
        protocol: "chat".into(),
        base_url: "agentsmith://local".into(),
        model: m.id,
        vision: true,
        enabled: true,
        auth_method: "local_engine".into(),
    })
}
pub async fn test_vision(id: &str) -> Result<String, String> {
    let p = profile(id)?;
    let mut img = image::RgbImage::from_pixel(256, 256, image::Rgb([255, 255, 255]));
    for y in 48..208 {
        for x in 48..208 {
            img.put_pixel(x, y, image::Rgb([220, 20, 20]));
        }
    }
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|_| "Falha ao preparar o teste visual.")?;
    let data = format!(
        "data:image/png;base64,{}",
        STANDARD.encode(png.into_inner())
    );
    let start = Instant::now();
    let answer = generate(
        &p,
        "Answer briefly in English.",
        "What color is the square in this image? Answer with one color word.",
        Some(&data),
    )
    .await?;
    if !answer
        .to_lowercase()
        .split(|c: char| !c.is_alphabetic())
        .any(|w| w == "red")
    {
        return Err(format!(
            "O modelo respondeu, mas não reconheceu a imagem de teste. Resposta: {}",
            answer.chars().take(160).collect::<String>()
        ));
    }
    Ok(format!("Visão local confirmada em {:.1}s, incluindo carregamento se necessário. Reconheceu a cor da imagem de teste. Isso não mede a precisão em interfaces Windows.",start.elapsed().as_secs_f64()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_is_pinned_and_local_profiles_cannot_redirect() {
        for m in catalog() {
            assert_eq!(m.files.len(), 2);
            for f in &m.files {
                assert!(f.url.starts_with("https://huggingface.co/ggml-org/"));
                assert!(!f.url.contains("/main/"));
                assert!(!f.name.contains('/'));
                assert_eq!(f.sha256.len(), 64);
                assert!(f.bytes > 0);
            }
            let mut p = profile(&m.id).unwrap();
            assert!(validate(&p).is_ok());
            p.base_url = "https://example.com".into();
            assert!(validate(&p).is_err());
        }
        assert!(model("../other").is_err());
    }
    #[test]
    fn corrupted_download_is_rejected() {
        let a = Asset {
            name: "test".into(),
            bytes: 3,
            sha256: format!("{:x}", Sha256::digest(b"abc")),
            url: String::new(),
        };
        assert!(check_digest(3, &a.sha256, &a).is_ok());
        assert!(check_digest(2, &a.sha256, &a).is_err());
        assert!(check_digest(3, "incorrect", &a).is_err());
    }
    #[test]
    fn partial_file_is_removed_on_drop() {
        let p = std::env::temp_dir().join(format!("agentsmith-test-{}", uuid::Uuid::new_v4()));
        std::fs::write(&p, b"partial").unwrap();
        {
            let _guard = Partial(p.clone());
        }
        assert!(!p.exists());
    }
    #[tokio::test]
    async fn cancelled_inference_drops_owned_process() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
            let pid = child.id();
            let _server = Server {
                child,
                model: String::new(),
                url: String::new(),
                token: String::new(),
                used: Instant::now(),
            };
            tx.send(pid).unwrap();
            std::future::pending::<()>().await;
        });
        let pid = rx.await.unwrap();
        task.abort();
        let _ = task.await;
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
    }
}
