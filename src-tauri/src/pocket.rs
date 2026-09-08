//! Private mobile surface. Loopback only; Tailscale Serve supplies HTTPS.
//! Pairing and sessions are ephemeral. No credentials/settings are exposed.
use crate::{
    executor,
    model::*,
    operator,
    remote::{Action, Remote},
    store::Store,
};
use axum::{
    extract::{DefaultBodyLimit, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::oneshot;

pub const PORT: u16 = 17420;
const PAIR_MS: u64 = 5 * 60 * 1000;
const SESSION_MS: u64 = 12 * 60 * 60 * 1000;
fn secret() -> String {
    format!("{}{}", operator::ticket(), operator::ticket())
}
fn hash(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub id: String,
    pub run_id: String,
    pub title: String,
    pub step: String,
    pub action: Value,
    pub expires_at: u64,
}
struct Pending {
    view: Approval,
    sender: oneshot::Sender<bool>,
}
#[derive(Default)]
struct Access {
    base: String,
    pair: Option<(String, u64)>,
    sessions: HashMap<String, u64>,
    pending: Option<Pending>,
    failures: u32,
    retry_at: u64,
}
#[derive(Default)]
pub struct Pocket {
    access: Mutex<Access>,
    server: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    lifecycle: tokio::sync::Mutex<()>,
    planning: AtomicBool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    active: bool,
    base_url: String,
    devices: usize,
    port: u16,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pairing {
    url: String,
    qr: String,
    expires_at: u64,
}

pub fn validate_base(value: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(value.trim())
        .map_err(|_| "Informe o endereço HTTPS do Tailscale.".to_string())?;
    if url.scheme() != "https"
        || !url.host_str().is_some_and(|h| h.ends_with(".ts.net"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || url.port().is_some()
    {
        return Err(
            "Use https://nome-do-mac.sua-rede.ts.net, sem caminho, porta ou credenciais.".into(),
        );
    }
    Ok(url.origin().ascii_serialization())
}
impl Pocket {
    pub async fn status(&self) -> Status {
        let active = self
            .server
            .lock()
            .await
            .as_ref()
            .is_some_and(|s| !s.is_finished());
        let mut a = self.access.lock().unwrap();
        a.sessions.retain(|_, expiry| *expiry > now());
        Status {
            active,
            base_url: a.base.clone(),
            devices: a.sessions.len(),
            port: PORT,
        }
    }
    pub fn pair(&self) -> Result<Pairing, String> {
        let mut a = self.access.lock().unwrap();
        if a.base.is_empty() {
            return Err("Ligue o Pocket primeiro.".into());
        }
        let token = secret();
        let expires_at = now() + PAIR_MS;
        a.pair = Some((hash(&token), expires_at));
        a.failures = 0;
        let url = format!("{}/#pair={token}", a.base);
        let qr = qrcode::QrCode::new(url.as_bytes())
            .map_err(|_| "Falha ao gerar QR.")?
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(240, 240)
            .build();
        Ok(Pairing {
            url,
            qr,
            expires_at,
        })
    }
    pub async fn start(self: &Arc<Self>, ctx: Context, base: String) -> Result<Status, String> {
        let _lock = self.lifecycle.lock().await;
        let base = validate_base(&base)?;
        if self
            .server
            .lock()
            .await
            .as_ref()
            .is_some_and(|s| !s.is_finished())
        {
            return Err("Desligue o Pocket antes de mudar seu endereço.".into());
        }
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, PORT))
            .await
            .map_err(|_| "A porta do Pocket está ocupada.".to_string())?;
        {
            let mut a = self.access.lock().unwrap();
            a.sessions.clear();
            a.pair = None;
            a.pending = None;
            a.base = base.clone();
        }
        let router = router(ctx);
        *self.server.lock().await = Some(tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        }));
        Ok(self.status().await)
    }
    pub async fn stop(&self) {
        let _lock = self.lifecycle.lock().await;
        if let Some(s) = self.server.lock().await.take() {
            s.abort();
        }
        let mut a = self.access.lock().unwrap();
        a.sessions.clear();
        a.pair = None;
        a.pending = None;
        a.base.clear();
        // Approval policy lives in Run: revoking devices must never disable it.
    }
    pub fn cancel_pending(&self, id: &str) {
        let mut a = self.access.lock().unwrap();
        if a.pending.as_ref().is_some_and(|p| p.view.run_id == id) {
            a.pending = None;
        }
    }
    pub async fn before_input(
        &self,
        run: &Run,
        step: usize,
        action: &Action,
    ) -> Result<(), String> {
        if !run.require_approval {
            return Ok(());
        }
        let id = secret();
        let (sender, receiver) = oneshot::channel();
        {
            let mut a = self.access.lock().unwrap();
            if a.base.is_empty() {
                return Err("Ligue o Pocket para aprovar esta ação.".into());
            }
            a.pending = Some(Pending {
                view: Approval {
                    id: id.clone(),
                    run_id: run.id.clone(),
                    title: run.title.clone(),
                    step: run.steps[step].title.clone(),
                    action: serde_json::to_value(action).unwrap_or(Value::Null),
                    expires_at: now() + 120_000,
                },
                sender,
            });
        }
        struct Cleanup<'a>(&'a Pocket, String);
        impl Drop for Cleanup<'_> {
            fn drop(&mut self) {
                let mut a = self.0.access.lock().unwrap();
                if a.pending.as_ref().is_some_and(|p| p.view.id == self.1) {
                    a.pending = None;
                }
            }
        }
        let _cleanup = Cleanup(self, id);
        match tokio::time::timeout(Duration::from_secs(120), receiver).await {
            Ok(Ok(true)) => Ok(()),
            Ok(Ok(false)) => {
                Err("Ação rejeitada pelo operador no Pocket. Nenhuma entrada enviada.".into())
            }
            _ => Err("Aprovação indisponível ou expirada. Nenhuma entrada enviada.".into()),
        }
    }
}

#[derive(Clone)]
pub struct Context {
    pub pocket: Arc<Pocket>,
    pub store: Arc<Store>,
    pub remote: Arc<Remote>,
    pub busy: Arc<AtomicBool>,
    pub control: Arc<Mutex<executor::Control>>,
    pub channels: Arc<operator::Channels>,
}
impl Context {
    pub fn from_app(s: &crate::AppState) -> Self {
        Self {
            pocket: s.remote.pocket.clone(),
            store: s.store.clone(),
            remote: s.remote.clone(),
            busy: s.busy.clone(),
            control: s.execution.clone(),
            channels: s.simplex.clone(),
        }
    }
}
type ApiResult = Result<Json<Value>, (StatusCode, Json<Value>)>;
fn error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"error":message.into()})))
}
fn conflict(e: impl Into<String>) -> (StatusCode, Json<Value>) {
    error(StatusCode::CONFLICT, e)
}
fn cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|p| p.trim().strip_prefix("smith_pocket="))
}
async fn guard(State(c): State<Context>, req: Request, next: Next) -> Response {
    let allowed = {
        let a = c.pocket.access.lock().unwrap();
        let host = req
            .headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let origin = req
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok());
        host == a.base.strip_prefix("https://").unwrap_or("")
            && (req.method() == "GET" || origin == Some(a.base.as_str()))
            && req
                .headers()
                .get("sec-fetch-site")
                .is_none_or(|s| s != "cross-site")
    };
    if !allowed {
        return error(StatusCode::FORBIDDEN, "Origem não autorizada.").into_response();
    }
    let path = req.uri().path();
    if path.starts_with("/api/") && path != "/api/pair" {
        let valid = {
            let a = c.pocket.access.lock().unwrap();
            cookie(req.headers())
                .and_then(|v| a.sessions.get(&hash(v)))
                .is_some_and(|e| *e > now())
        };
        if !valid {
            return error(
                StatusCode::UNAUTHORIZED,
                "Pareie este aparelho pelo AgentSmith no Mac.",
            )
            .into_response();
        }
    }
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    h.insert("referrer-policy", "no-referrer".parse().unwrap());
    h.insert("x-content-type-options", "nosniff".parse().unwrap());
    h.insert("content-security-policy","default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
    res
}
pub fn router(c: Context) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                    include_str!("../../pocket/index.html"),
                )
            }),
        )
        .route(
            "/pocket.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../../pocket/pocket.js"),
                )
            }),
        )
        .route(
            "/pocket.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css")],
                    include_str!("../../pocket/pocket.css"),
                )
            }),
        )
        .route(
            "/sw.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../../pocket/sw.js"),
                )
            }),
        )
        .route(
            "/manifest.webmanifest",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "application/manifest+json")],
                    include_str!("../../pocket/manifest.webmanifest"),
                )
            }),
        )
        .route(
            "/icon.svg",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "image/svg+xml")],
                    include_str!("../../pocket/icon.svg"),
                )
            }),
        )
        .route("/api/pair", post(pair))
        .route("/api/state", get(state))
        .route("/api/frame", get(frame))
        .route("/api/command", post(command))
        .route("/api/logout", post(logout))
        .layer(DefaultBodyLimit::max(40_000))
        .layer(middleware::from_fn_with_state(c.clone(), guard))
        .with_state(c)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PairRequest {
    token: String,
}
async fn pair(State(c): State<Context>, Json(body): Json<PairRequest>) -> Response {
    let mut a = c.pocket.access.lock().unwrap();
    if a.retry_at > now() {
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "Aguarde um minuto e tente novamente.",
        )
        .into_response();
    }
    if !a
        .pair
        .as_ref()
        .is_some_and(|(h, expiry)| *expiry > now() && *h == hash(&body.token))
    {
        a.failures += 1;
        if a.failures >= 5 {
            a.retry_at = now() + 60_000;
            a.failures = 0;
        }
        return error(
            StatusCode::UNAUTHORIZED,
            "Pareamento inválido, usado ou expirado. Gere outro QR no Mac.",
        )
        .into_response();
    }
    a.sessions.retain(|_, e| *e > now());
    if a.sessions.len() >= 8 {
        return error(
            StatusCode::CONFLICT,
            "Limite de oito aparelhos. Revogue os acessos no Mac.",
        )
        .into_response();
    }
    a.pair = None;
    a.failures = 0;
    let token = secret();
    a.sessions.insert(hash(&token), now() + SESSION_MS);
    let mut response = Json(json!({"ok":true})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        format!("smith_pocket={token}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=43200")
            .parse()
            .unwrap(),
    );
    response
}
async fn logout(State(c): State<Context>, headers: HeaderMap) -> Response {
    if let Some(token) = cookie(&headers) {
        c.pocket
            .access
            .lock()
            .unwrap()
            .sessions
            .remove(&hash(token));
    }
    (
        [(
            header::SET_COOKIE,
            "smith_pocket=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0",
        )],
        Json(json!({"ok":true})),
    )
        .into_response()
}
async fn state(State(c): State<Context>) -> ApiResult {
    let settings = c.store.settings().map_err(conflict)?;
    let runs = c.store.runs().map_err(conflict)?;
    let a = c.pocket.access.lock().unwrap();
    Ok(Json(
        json!({"busy":c.busy.load(Ordering::SeqCst),"planning":c.pocket.planning.load(Ordering::SeqCst),"session":c.remote.info.lock().unwrap().clone(),"machines":settings.machines.iter().map(|m|json!({"id":m.id,"name":m.name,"protocol":m.protocol})).collect::<Vec<_>>(),"runs":runs.iter().map(|r|json!({"id":r.id,"title":r.title,"machineId":r.machine_id,"steps":r.steps,"status":r.status,"log":r.log.iter().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>(),"actionCount":r.action_count,"progress":r.progress,"updatedAt":r.updated_at,"requireApproval":r.require_approval})).collect::<Vec<_>>(),"guarded":runs.iter().filter(|r|r.require_approval).map(|r|&r.id).collect::<Vec<_>>(),"approval":a.pending.as_ref().filter(|p|p.view.expires_at>now()).map(|p|&p.view)}),
    ))
}
async fn frame(State(c): State<Context>) -> ApiResult {
    if c.remote.info.lock().unwrap().status != "connected" {
        return Err(conflict("Sem sessão conectada."));
    }
    let frame = c.remote.snapshot().map_err(conflict)?;
    Ok(Json(
        json!({"machineId":c.remote.info.lock().unwrap().machine_id,"frame":frame}),
    ))
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Plan {
        machine_id: String,
        instructions: String,
    },
    Start {
        id: String,
        updated_at: u64,
        approvals: bool,
    },
    Pause {
        id: String,
    },
    Stop {
        id: String,
    },
    Guide {
        id: String,
        updated_at: u64,
        message: String,
    },
    Decide {
        id: String,
        approve: bool,
    },
}
async fn command(State(c): State<Context>, Json(body): Json<Command>) -> ApiResult {
    match body {
        Command::Plan {
            machine_id,
            instructions,
        } => {
            if c.pocket.planning.swap(true, Ordering::SeqCst) {
                return Err(conflict("Um roteiro já está sendo preparado."));
            }
            struct Reset(Arc<Pocket>);
            impl Drop for Reset {
                fn drop(&mut self) {
                    self.0.planning.store(false, Ordering::SeqCst);
                }
            }
            let _reset = Reset(c.pocket.clone());
            let settings = c.store.settings().map_err(conflict)?;
            let m = settings
                .machines
                .iter()
                .find(|m| m.id == machine_id)
                .ok_or_else(|| conflict("Máquina não encontrada."))?;
            crate::rustdesk::require_automation(m).map_err(conflict)?;
            let run = executor::plan(&c.store, machine_id, instructions)
                .await
                .map_err(conflict)?;
            Ok(Json(json!({"run":run})))
        }
        Command::Start {
            id,
            updated_at,
            approvals,
        } => {
            executor::launch_reviewed(&c, id, updated_at, approvals)
                .await
                .map_err(conflict)?;
            Ok(Json(json!({"ok":true})))
        }
        Command::Stop { id } => {
            c.pocket.cancel_pending(&id);
            executor::stop(&c.store, &c.remote, &c.control, &id)
                .await
                .map_err(conflict)?;
            Ok(Json(json!({"ok":true})))
        }
        Command::Pause { id } => {
            {
                let guard = c.control.lock().unwrap();
                if !guard.is_active(&id) {
                    return Err(conflict("Esta tarefa não está em execução."));
                }
                c.remote.epoch.fetch_add(1, Ordering::SeqCst);
            }
            c.pocket.cancel_pending(&id);
            c.remote.release().await.map_err(conflict)?;
            Ok(Json(json!({"ok":true})))
        }
        Command::Guide {
            id,
            updated_at,
            message,
        } => {
            let _lock = c.control.lock().unwrap();
            if c.busy.load(Ordering::SeqCst) {
                return Err(conflict(
                    "Pause a tarefa e aguarde antes de enviar a orientação.",
                ));
            }
            let mut run = c.store.run(&id).map_err(conflict)?;
            crate::plan_edit::editable(&run, updated_at).map_err(conflict)?;
            if message.trim().is_empty()
                || message.len() > 4000
                || run.instructions.len() + message.len() + 64 > 30000
            {
                return Err(conflict("Escreva uma orientação de até 4.000 caracteres."));
            }
            if ["completed", "cancelled", "expired"].contains(&run.status.as_str()) {
                return Err(conflict(
                    "Esta tarefa já foi encerrada. Prepare um novo pedido.",
                ));
            }
            run.instructions.push_str(&format!(
                "\n\nOrientação do operador pelo Pocket:\n{}",
                message.trim()
            ));
            run.log.push(
                "Orientação recebida pelo Pocket. Revise e retome quando estiver pronto.".into(),
            );
            run.updated_at = now().max(run.updated_at + 1);
            c.store.put_run(&run).map_err(conflict)?;
            Ok(Json(json!({"run":run})))
        }
        Command::Decide { id, approve } => {
            let mut a = c.pocket.access.lock().unwrap();
            if !a
                .pending
                .as_ref()
                .is_some_and(|p| p.view.id == id && p.view.expires_at > now())
            {
                return Err(conflict("Aprovação expirada ou já respondida."));
            }
            let pending = a.pending.take().unwrap();
            pending
                .sender
                .send(approve)
                .map_err(|_| conflict("A tarefa já deixou de esperar esta aprovação."))?;
            Ok(Json(json!({"ok":true})))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use tower::ServiceExt;
    fn context() -> Context {
        let remote = Arc::new(Remote::new());
        remote.pocket.access.lock().unwrap().base = "https://smith.example.ts.net".into();
        Context {
            pocket: remote.pocket.clone(),
            remote,
            store: Arc::new(Store::new(std::path::Path::new(":memory:")).unwrap()),
            busy: Arc::new(AtomicBool::new(false)),
            control: Default::default(),
            channels: Arc::new(operator::Channels::new()),
        }
    }
    async fn request(
        c: &Context,
        path: &str,
        body: Option<Value>,
        token: Option<&str>,
        origin: &str,
    ) -> Response {
        let mut request = Request::builder()
            .uri(path)
            .method(if body.is_some() { "POST" } else { "GET" })
            .header("host", "smith.example.ts.net")
            .header("origin", origin);
        if let Some(t) = token {
            request = request.header("cookie", format!("smith_pocket={t}"));
        }
        if body.is_some() {
            request = request.header("content-type", "application/json");
        }
        router(c.clone())
            .oneshot(
                request
                    .body(Body::from(body.map(|v| v.to_string()).unwrap_or_default()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }
    fn authorize(c: &Context) -> String {
        let token = secret();
        c.pocket
            .access
            .lock()
            .unwrap()
            .sessions
            .insert(hash(&token), now() + SESSION_MS);
        token
    }
    fn run() -> Run {
        serde_json::from_value(json!({"id":"run-1","machineId":"m-1","instructions":"Open calculator","title":"Calculator","steps":[{"title":"Open","success":"Visible","status":"pending"}],"requireApproval":true,"status":"paused","log":[],"actionCount":0,"updatedAt":20})).unwrap()
    }
    #[test]
    fn only_private_https_tailnet_origins_are_accepted() {
        assert_eq!(
            validate_base("https://smith.example.ts.net/").unwrap(),
            "https://smith.example.ts.net"
        );
        for value in [
            "http://smith.example.ts.net",
            "https://example.com",
            "https://smith.ts.net.evil.com",
            "https://u:p@smith.ts.net",
            "https://smith.ts.net/path",
            "https://smith.ts.net?token=x",
            "https://smith.ts.net/#secret",
            "https://smith.ts.net:444",
        ] {
            assert!(validate_base(value).is_err(), "{value}");
        }
    }
    #[tokio::test]
    async fn pairing_is_one_time_expiring_and_cookie_based() {
        let c = context();
        let pair = c.pocket.pair().unwrap();
        assert!(pair.url.contains("/#pair="));
        assert!(!pair.url.contains('?'));
        let token = pair.url.split("#pair=").nth(1).unwrap();
        let response = request(
            &c,
            "/api/pair",
            Some(json!({"token":token})),
            None,
            "https://smith.example.ts.net",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
        assert!(cookie.contains("HttpOnly; Secure; SameSite=Strict"));
        let session = cookie
            .split(';')
            .next()
            .unwrap()
            .strip_prefix("smith_pocket=")
            .unwrap();
        assert_eq!(
            request(&c, "/api/state", None, Some(session), "")
                .await
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            request(
                &c,
                "/api/pair",
                Some(json!({"token":token})),
                None,
                "https://smith.example.ts.net"
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
        c.pocket
            .access
            .lock()
            .unwrap()
            .sessions
            .insert(hash(session), 0);
        assert_eq!(
            request(&c, "/api/state", None, Some(session), "")
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        c.pocket.access.lock().unwrap().pair = Some((hash("expired"), 0));
        assert_eq!(
            request(
                &c,
                "/api/pair",
                Some(json!({"token":"expired"})),
                None,
                "https://smith.example.ts.net"
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    #[tokio::test]
    async fn guards_private_data_mutations_and_logout() {
        let c = context();
        let token = authorize(&c);
        assert_eq!(
            request(&c, "/api/state", None, None, "").await.status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request(
                &c,
                "/api/command",
                Some(json!({"kind":"stop","id":"run-1"})),
                Some(&token),
                "https://attacker.example"
            )
            .await
            .status(),
            StatusCode::FORBIDDEN
        );
        let wrong_host = Request::builder()
            .uri("/api/state")
            .header("host", "attacker.example")
            .header("cookie", format!("smith_pocket={token}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            router(c.clone())
                .oneshot(wrong_host)
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        let response = request(&c, "/api/state", None, Some(&token), "").await;
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body = to_bytes(response.into_body(), 100000).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert!(value.get("profiles").is_none());
        assert_eq!(
            request(
                &c,
                "/api/logout",
                Some(json!({})),
                Some(&token),
                "https://smith.example.ts.net"
            )
            .await
            .status(),
            StatusCode::OK
        );
        assert_eq!(
            request(&c, "/api/state", None, Some(&token), "")
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    #[tokio::test]
    async fn guidance_preserves_steps_and_rejects_stale_or_busy_updates() {
        let c = context();
        let token = authorize(&c);
        c.store.put_run(&run()).unwrap();
        let guide =
            json!({"kind":"guide","id":"run-1","updated_at":20,"message":"Use the Start menu"});
        let stale = json!({"kind":"guide","id":"run-1","updated_at":19,"message":"Other"});
        assert_eq!(
            request(
                &c,
                "/api/command",
                Some(stale),
                Some(&token),
                "https://smith.example.ts.net"
            )
            .await
            .status(),
            StatusCode::CONFLICT
        );
        c.busy.store(true, Ordering::SeqCst);
        assert_eq!(
            request(
                &c,
                "/api/command",
                Some(guide.clone()),
                Some(&token),
                "https://smith.example.ts.net"
            )
            .await
            .status(),
            StatusCode::CONFLICT
        );
        c.busy.store(false, Ordering::SeqCst);
        assert_eq!(
            request(
                &c,
                "/api/command",
                Some(guide.clone()),
                Some(&token),
                "https://smith.example.ts.net"
            )
            .await
            .status(),
            StatusCode::OK
        );
        assert!(c
            .store
            .run("run-1")
            .unwrap()
            .instructions
            .contains("Use the Start menu"));
        assert_eq!(c.store.run("run-1").unwrap().steps[0].title, "Open");
        assert_eq!(
            request(
                &c,
                "/api/command",
                Some(guide),
                Some(&token),
                "https://smith.example.ts.net"
            )
            .await
            .status(),
            StatusCode::CONFLICT
        );
        let epoch = c.remote.epoch.load(Ordering::SeqCst);
        assert_eq!(
            request(
                &c,
                "/api/command",
                Some(json!({"kind":"pause","id":"wrong-run"})),
                Some(&token),
                "https://smith.example.ts.net"
            )
            .await
            .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(c.remote.epoch.load(Ordering::SeqCst), epoch);
    }
    #[tokio::test]
    async fn approval_is_exactly_once_and_cancellation_removes_it() {
        let c = context();
        let token = authorize(&c);
        let run = super::tests::run();
        let pocket = c.pocket.clone();
        let task = tokio::spawn(async move {
            pocket
                .before_input(&run, 0, &Action::Click { x: 10, y: 20 })
                .await
        });
        for _ in 0..100 {
            if c.pocket.access.lock().unwrap().pending.is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
        let id = c
            .pocket
            .access
            .lock()
            .unwrap()
            .pending
            .as_ref()
            .unwrap()
            .view
            .id
            .clone();
        let body = json!({"kind":"decide","id":id,"approve":true});
        assert_eq!(
            request(
                &c,
                "/api/command",
                Some(body.clone()),
                Some(&token),
                "https://smith.example.ts.net"
            )
            .await
            .status(),
            StatusCode::OK
        );
        assert!(task.await.unwrap().is_ok());
        assert_eq!(
            request(
                &c,
                "/api/command",
                Some(body),
                Some(&token),
                "https://smith.example.ts.net"
            )
            .await
            .status(),
            StatusCode::CONFLICT
        );
        let pocket = c.pocket.clone();
        let task = tokio::spawn(async move {
            pocket
                .before_input(&super::tests::run(), 0, &Action::Click { x: 1, y: 2 })
                .await
        });
        for _ in 0..100 {
            if c.pocket.access.lock().unwrap().pending.is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
        task.abort();
        let _ = task.await;
        assert!(c.pocket.access.lock().unwrap().pending.is_none());
        assert!(super::tests::run().require_approval);
    }
    #[tokio::test]
    async fn stop_revokes_sessions_and_keeps_required_approvals() {
        let c = context();
        let _token = authorize(&c);
        c.pocket.stop().await;
        assert!(c.pocket.access.lock().unwrap().sessions.is_empty());
        assert!(super::tests::run().require_approval);
        assert!(c
            .pocket
            .before_input(&run(), 0, &Action::Click { x: 1, y: 1 })
            .await
            .is_err());
    }
    #[tokio::test]
    async fn expired_and_rejected_actions_never_receive_approval() {
        let c = context();
        let token = authorize(&c);
        for expired in [true, false] {
            let pocket = c.pocket.clone();
            let task = tokio::spawn(async move {
                pocket
                    .before_input(
                        &run(),
                        0,
                        &Action::Key {
                            keys: vec!["enter".into()],
                        },
                    )
                    .await
            });
            for _ in 0..100 {
                if c.pocket.access.lock().unwrap().pending.is_some() {
                    break;
                }
                tokio::task::yield_now().await;
            }
            let id = {
                let mut a = c.pocket.access.lock().unwrap();
                let p = a.pending.as_mut().unwrap();
                if expired {
                    p.view.expires_at = 0;
                }
                p.view.id.clone()
            };
            let response = request(
                &c,
                "/api/command",
                Some(json!({"kind":"decide","id":id,"approve":expired})),
                Some(&token),
                "https://smith.example.ts.net",
            )
            .await;
            assert_eq!(
                response.status(),
                if expired {
                    StatusCode::CONFLICT
                } else {
                    StatusCode::OK
                }
            );
            if expired {
                c.pocket.cancel_pending("run-1");
            }
            assert!(task.await.unwrap().is_err());
        }
    }
    #[tokio::test]
    async fn reviewed_start_is_atomic_and_policy_is_persisted_with_run() {
        let c = context();
        c.store.put_run(&run()).unwrap();
        assert!(c.store.run("run-1").unwrap().require_approval);
        let result = executor::launch_reviewed(&c, "run-1".into(), 19, false).await;
        assert!(result.unwrap_err().contains("mudou"));
        assert!(!c.busy.load(Ordering::SeqCst));
        assert!(c.store.run("run-1").unwrap().require_approval);
    }
}
