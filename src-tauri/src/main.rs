#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod browser_auth;
mod client_log;
mod executor;
mod harness;
mod llm;
mod local_engine;
mod model;
mod observation;
mod ocr;
mod plan_edit;
mod reading_test;
mod remote;
mod repetition;
mod rustdesk;
mod store;
mod vision;
use model::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::Manager;
const RDP_WINDOW: &str = "remote-rdp";

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionViewInfo {
    #[serde(flatten)]
    session: SessionInfo,
    detached: bool,
}

#[tauri::command]
async fn open_rustdesk_client(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
    id: Option<String>,
) -> Result<(), String> {
    if !["main", RDP_WINDOW].contains(&window.label()) {
        return Err("Abra o cliente pela Central do AgentSmith.".into());
    }
    if state.busy.load(Ordering::SeqCst) {
        return Err("Pause a tarefa antes de abrir o cliente RustDesk.".into());
    }
    let settings = state.store.settings()?;
    let machine = match id {
        Some(id) => Some(
            settings
                .machines
                .iter()
                .find(|m| m.id == id)
                .ok_or("Máquina não encontrada.")?,
        ),
        None => None,
    };
    rustdesk::open(&app, machine)
}

fn require_machine_automation(state: &AppState, machine_id: &str) -> Result<(), String> {
    let settings = state.store.settings()?;
    let machine = settings
        .machines
        .iter()
        .find(|m| m.id == machine_id)
        .ok_or("Máquina não encontrada.")?;
    rustdesk::require_automation(machine)
}
fn require_run_automation(state: &AppState, id: &str) -> Result<(), String> {
    require_machine_automation(state, &state.store.run(id)?.machine_id)
}

#[tauri::command]
async fn open_prodigy_site() -> Result<(), String> {
    let status = tokio::process::Command::new("/usr/bin/open")
        .arg("https://prodigy-lab.com")
        .status()
        .await
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("Não foi possível abrir o navegador.".into())
    }
}

#[tauri::command]
async fn detach_rdp(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(RDP_WINDOW) {
        window
            .show()
            .map_err(|_| "Não foi possível mostrar a janela RDP.")?;
        return window
            .set_focus()
            .map_err(|_| "Não foi possível ativar a janela RDP.".into());
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        RDP_WINDOW,
        tauri::WebviewUrl::App("index.html?view=rdp".into()),
    )
    .title("AgentSmith — Windows remoto")
    .inner_size(1440.0, 950.0)
    .min_inner_size(720.0, 480.0)
    .resizable(true)
    .build()
    .map_err(|_| "Não foi possível abrir a janela RDP.")?;
    Ok(())
}

#[tauri::command]
async fn attach_rdp(app: tauri::AppHandle) -> Result<(), String> {
    let main = app
        .get_webview_window("main")
        .ok_or("A janela principal não está disponível.")?;
    main.show()
        .map_err(|_| "Não foi possível mostrar a Central.")?;
    main.set_focus()
        .map_err(|_| "Não foi possível ativar a Central.")?;
    if let Some(window) = app.get_webview_window(RDP_WINDOW) {
        window
            .close()
            .map_err(|_| "Não foi possível fechar a janela destacada.")?;
    }
    Ok(())
}

#[tauri::command]
async fn toggle_rdp_fullscreen(window: tauri::WebviewWindow) -> Result<bool, String> {
    let full = !window
        .is_fullscreen()
        .map_err(|_| "Estado da janela indisponível.")?;
    window
        .set_fullscreen(full)
        .map_err(|_| "Não foi possível alterar a tela cheia.")?;
    Ok(full)
}
struct AppState {
    store: Arc<store::Store>,
    remote: Arc<remote::Remote>,
    busy: Arc<AtomicBool>,
    execution: Arc<Mutex<executor::Control>>,
    auth: browser_auth::AuthManager,
}
#[tauri::command]
fn browser_auth_status(
    state: tauri::State<AppState>,
    vendor: String,
) -> Result<browser_auth::Status, String> {
    state.auth.status(&vendor)
}
#[tauri::command]
fn browser_auth_start(
    state: tauri::State<AppState>,
    vendor: String,
    install: bool,
) -> Result<browser_auth::Status, String> {
    if state.busy.load(Ordering::SeqCst) {
        return Err("Pause a tarefa antes de alterar a conexão de IA.".into());
    }
    if state.store.settings()?.local_only {
        return Err("Desative o modo somente local para conectar um provedor na nuvem.".into());
    }
    state.auth.start(vendor, install)
}
#[tauri::command]
fn browser_auth_cancel(state: tauri::State<AppState>, vendor: String) -> Result<(), String> {
    state.auth.cancel(&vendor)
}
#[tauri::command]
async fn test_browser_profile(
    state: tauri::State<'_, AppState>,
    profile: Profile,
) -> Result<String, String> {
    if profile.auth_method != "browser" {
        return Err("Escolha login pelo navegador.".into());
    }
    let s = state.store.settings()?;
    llm::call(
        &profile,
        "",
        s.local_only,
        "Responda brevemente.",
        "Responda apenas: AgentSmith conectado.",
        None,
    )
    .await
    .map_err(|e| e.message)?;
    Ok("Conexão confirmada: o modelo respondeu usando o cliente oficial. Você já pode salvar o perfil.".into())
}
#[tauri::command]
fn local_engine_status() -> Result<local_engine::Status, String> {
    local_engine::status()
}
#[tauri::command]
fn local_model_download(id: String) -> Result<(), String> {
    local_engine::download(id)
}
#[tauri::command]
fn local_download_cancel() -> Result<(), String> {
    local_engine::cancel_download()
}
fn require_idle(state: &AppState) -> Result<(), String> {
    if state.busy.load(Ordering::SeqCst) {
        Err("Pause a tarefa antes de alterar ou testar o modelo local.".into())
    } else {
        Ok(())
    }
}
#[tauri::command]
fn local_engine_stop(state: tauri::State<AppState>) -> Result<(), String> {
    require_idle(&state)?;
    local_engine::stop()
}
#[tauri::command]
async fn local_model_remove(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    require_idle(&state)?;
    local_engine::remove(&id).await
}
#[tauri::command]
async fn local_vision_test(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    require_idle(&state)?;
    local_engine::test_vision(&id).await
}
#[tauri::command]
fn local_model_profile(state: tauri::State<AppState>, id: String) -> Result<Settings, String> {
    require_idle(&state)?;
    let p = local_engine::profile(&id)?;
    let mut s = state.store.settings()?;
    if !s
        .profiles
        .iter()
        .any(|old| old.vendor == "builtin" && old.model == id)
    {
        s.profiles.push(p);
        state.store.save_settings(&s)?;
    }
    Ok(s)
}
#[tauri::command]
fn load_settings(state: tauri::State<AppState>) -> Result<Settings, String> {
    state.store.settings()
}
#[tauri::command]
fn save_settings(state: tauri::State<AppState>, settings: Settings) -> Result<(), String> {
    if state.busy.load(Ordering::SeqCst) {
        return Err("Pause a tarefa antes de alterar a configuração.".into());
    }
    state.store.save_settings(&settings)
}
#[tauri::command]
fn remove_profile(state: tauri::State<AppState>, id: String) -> Result<Settings, String> {
    if state.busy.load(Ordering::SeqCst) {
        return Err("Pause ou pare a tarefa antes de remover um perfil.".into());
    }
    let mut settings = state.store.settings()?;
    let profile = settings.remove_profile(&id)?;
    if profile.auth_method != "browser" && profile.vendor != "builtin" {
        store::delete_secret(&profile.id, &profile.base_url)?;
    }
    state.store.save_settings(&settings)?;
    Ok(settings)
}
#[tauri::command]
async fn save_performance(
    state: tauri::State<'_, AppState>,
    performance: PerformanceSettings,
) -> Result<(), String> {
    if state.busy.load(Ordering::SeqCst) {
        return Err("Pause a tarefa antes de ajustar o ritmo.".into());
    }
    performance.validate()?;
    let mut settings = state.store.settings()?;
    settings.performance = performance;
    state.store.save_settings(&settings)?;
    state
        .remote
        .configure_capture(settings.performance.capture_interval_ms)
        .await
}
#[tauri::command]
fn snapshot_if_new(
    state: tauri::State<AppState>,
    sequence: u64,
) -> Result<Option<Snapshot>, String> {
    state.remote.snapshot_if_new(sequence)
}
#[tauri::command]
fn save_credential(
    state: tauri::State<AppState>,
    id: String,
    binding: String,
    secret: String,
) -> Result<(), String> {
    let s = state.store.settings()?;
    if !s
        .profiles
        .iter()
        .any(|p| p.id == id && p.base_url == binding)
        && !s
            .machines
            .iter()
            .any(|m| m.id == id && machine_binding(m) == binding)
    {
        return Err("Salve o cadastro antes da credencial.".into());
    }
    store::save_secret(&id, &binding, &secret)
}
fn machine_binding(m: &Machine) -> String {
    match m.protocol.as_str() {
        // A RustDesk destination is identified by its ID alone, and its
        // password is the peer's, not a Windows account's.
        "rustdesk" => format!("rustdesk://{}", m.host),
        _ => format!("rdp://{}:{}|{}|{}", m.host, m.port, m.domain, m.username),
    }
}
#[tauri::command]
async fn test_operator_profile(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    let _guard = reading_test::Guard::acquire(state.busy.clone())?;
    executor::test_operator_profile(&state.store.settings()?, &id).await
}
#[tauri::command]
async fn test_profile(state: tauri::State<'_, AppState>, id: String) -> Result<String, String> {
    let s = state.store.settings()?;
    let p = s
        .profiles
        .iter()
        .find(|p| p.id == id)
        .ok_or("Perfil não encontrado")?;
    let key = if ["browser", "local_engine"].contains(&p.auth_method.as_str()) {
        String::new()
    } else {
        store::get_secret(&id, &p.base_url).or_else(|e| {
            if ["local", "ollama", "lmstudio"].contains(&p.vendor.as_str()) {
                Ok(String::new())
            } else {
                Err(e)
            }
        })?
    };
    let start = now();
    llm::call(
        p,
        &key,
        s.local_only,
        "Responda brevemente.",
        "Responda apenas: AgentSmith conectado.",
        None,
    )
    .await
    .map_err(|e| e.message)?;
    Ok(format!(
        "Resposta de texto recebida em {:.1}s. Visão depende do modelo selecionado.",
        (now() - start) as f64 / 1000.
    ))
}
#[tauri::command]
async fn list_models(state: tauri::State<'_, AppState>, id: String) -> Result<Vec<String>, String> {
    let s = state.store.settings()?;
    let p = s
        .profiles
        .iter()
        .find(|p| p.id == id)
        .ok_or("Perfil não encontrado")?;
    llm::endpoint(p, s.local_only)?;
    if p.auth_method == "local_engine" {
        return Ok(vec![p.model.clone()]);
    }
    if p.auth_method == "browser" {
        return Ok(vec!["default".into()]);
    }
    if p.protocol == "bedrock" {
        return Err(
            "Informe o ID do modelo ou perfil de inferência disponível na sua região Bedrock."
                .into(),
        );
    }
    let key = store::get_secret(&id, &p.base_url).unwrap_or_default();
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|_| "Falha HTTP")?;
    let mut req = client.get(format!("{}/models", p.base_url.trim_end_matches('/')));
    if p.protocol == "anthropic" {
        req = req
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01");
    } else if !key.is_empty() {
        req = req.bearer_auth(key);
    }
    let res = req
        .send()
        .await
        .map_err(|_| "Não foi possível consultar modelos.")?;
    if !res.status().is_success() {
        return Err(format!(
            "Consulta de modelos retornou HTTP {}. Você pode informar o ID manualmente.",
            res.status().as_u16()
        ));
    }
    let v: serde_json::Value = res.json().await.map_err(|_| "Resposta inválida")?;
    Ok(v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["id"].as_str().map(String::from))
        .collect())
}
#[tauri::command]
async fn machine_credential_status(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<bool, String> {
    let settings = state.store.settings()?;
    let machine = settings
        .machines
        .iter()
        .find(|m| m.id == id)
        .ok_or("Máquina não encontrada.")?;
    let binding = machine_binding(machine);
    tauri::async_runtime::spawn_blocking(move || store::has_secret(&id, &binding))
        .await
        .map_err(|_| "Não foi possível consultar o Chaves.")?
}
#[tauri::command]
async fn connect_saved_machine(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    if state.busy.load(Ordering::SeqCst) {
        return Err("Pause a tarefa antes de trocar de conexão.".into());
    }
    let settings = state.store.settings()?;
    let machine = settings
        .machines
        .iter()
        .find(|m| m.id == id)
        .ok_or("Máquina não encontrada.")?;
    rustdesk::require_automation(machine)?;
    let binding = machine_binding(machine);
    let account = id.clone();
    let password =
        tauri::async_runtime::spawn_blocking(move || store::optional_secret(&account, &binding))
            .await
            .map_err(|_| "Não foi possível acessar o Chaves.")??;
    match password {
        Some(password) => connection_outcome(
            start_machine_connection(
                app,
                &state,
                machine,
                password,
                settings.performance.capture_interval_ms,
                &Default::default(),
            )
            .await,
        ),
        None => Ok("password".into()),
    }
}
#[tauri::command]
async fn connect_machine(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
    password: Option<String>,
    second_factor: Option<remote::SecondFactor>,
) -> Result<String, String> {
    if state.busy.load(Ordering::SeqCst) {
        return Err("Pause a tarefa antes de trocar de conexão.".into());
    }
    let s = state.store.settings()?;
    let m = s
        .machines
        .iter()
        .find(|m| m.id == id)
        .ok_or("Máquina não encontrada")?;
    let password = match password {
        Some(p) if !p.is_empty() => p,
        _ => store::get_secret(&id, &machine_binding(m))?,
    };
    connection_outcome(
        start_machine_connection(
            app,
            &state,
            m,
            password,
            s.performance.capture_interval_ms,
            &second_factor.unwrap_or_default(),
        )
        .await,
    )
}

/// A machine that wants a second factor is not a failure: the interface asks
/// for a code and connects again.
fn connection_outcome(result: Result<(), String>) -> Result<String, String> {
    match result {
        Ok(()) => Ok("connected".into()),
        Err(error)
            if error == rustdesk::session::SECOND_FACTOR_REQUIRED
                || error == rustdesk::session::SECOND_FACTOR_WITHOUT_TRUST =>
        {
            Ok(error)
        }
        Err(error) => Err(error),
    }
}
async fn start_machine_connection(
    app: tauri::AppHandle,
    state: &AppState,
    m: &Machine,
    password: String,
    capture_interval_ms: u32,
    second_factor: &remote::SecondFactor,
) -> Result<(), String> {
    if state.busy.load(Ordering::SeqCst) {
        return Err("Pause a tarefa antes de trocar de conexão.".into());
    }
    let helper = app
        .path()
        .resource_dir()
        .map_err(|_| "Recursos indisponíveis")?
        .join("rdp/agentsmith-rdp");
    let helper = if helper.exists() {
        helper
    } else {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/rdp/agentsmith-rdp")
    };
    state
        .remote
        .connect(
            m,
            &password,
            &helper,
            capture_interval_ms,
            &state.store.settings()?.rustdesk,
            second_factor,
        )
        .await
}
#[tauri::command]
async fn disconnect_machine(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.remote.disconnect().await
}
#[tauri::command]
fn session_info(app: tauri::AppHandle, state: tauri::State<AppState>) -> SessionViewInfo {
    SessionViewInfo {
        session: state.remote.info.lock().unwrap().clone(),
        detached: app.get_webview_window(RDP_WINDOW).is_some(),
    }
}
#[tauri::command]
fn snapshot(state: tauri::State<AppState>) -> Result<Snapshot, String> {
    state.remote.snapshot()
}
#[tauri::command]
fn list_runs(state: tauri::State<AppState>) -> Result<Vec<Run>, String> {
    state.store.runs()
}
#[tauri::command]
async fn plan_run(
    state: tauri::State<'_, AppState>,
    machine_id: String,
    instructions: String,
) -> Result<Run, String> {
    require_machine_automation(&state, &machine_id)?;
    executor::plan(&state.store, machine_id, instructions).await
}
#[tauri::command]
async fn start_run(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    require_run_automation(&state, &id)?;
    executor::launch(
        state.store.clone(),
        state.remote.clone(),
        state.busy.clone(),
        state.execution.clone(),
        id,
    )
    .await
}
#[tauri::command]
async fn repeat_run(
    state: tauri::State<'_, AppState>,
    id: String,
    options: repetition::RepeatOptions,
) -> Result<Run, String> {
    require_run_automation(&state, &id)?;
    executor::repeat(
        state.store.clone(),
        state.remote.clone(),
        state.busy.clone(),
        state.execution.clone(),
        id,
        options,
    )
    .await
}
#[tauri::command]
async fn restart_run(state: tauri::State<'_, AppState>, id: String) -> Result<Run, String> {
    require_run_automation(&state, &id)?;
    executor::restart(
        state.store.clone(),
        state.remote.clone(),
        state.busy.clone(),
        state.execution.clone(),
        id,
    )
    .await
}
#[tauri::command]
async fn stop_run(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    executor::stop(&state.store, &state.remote, &state.execution, &id).await
}
#[tauri::command]
async fn pause_run(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.remote.epoch.fetch_add(1, Ordering::SeqCst);
    state.remote.release().await
}
#[tauri::command]
async fn manual_action(
    state: tauri::State<'_, AppState>,
    action: remote::Action,
) -> Result<(), String> {
    if state.busy.load(Ordering::SeqCst) {
        return Err("Assuma o controle antes de enviar entradas manuais.".into());
    }
    state
        .remote
        .act(&action, state.remote.epoch.load(Ordering::SeqCst))
        .await
}
#[tauri::command]
async fn set_step_text_check(
    state: tauri::State<'_, AppState>,
    id: String,
    index: usize,
    check: Option<ocr::TextCheck>,
) -> Result<(), String> {
    let _lock = state.execution.lock().unwrap();
    require_idle(&state)?;
    let mut run = state.store.run(&id)?;
    if index >= run.steps.len() {
        return Err("Etapa inválida.".into());
    }
    if let Some(rule) = &check {
        rule.validate()?;
        if state.remote.info.lock().unwrap().machine_id != run.machine_id {
            return Err("Conecte a máquina desta tarefa.".into());
        }
        let frame = state.remote.snapshot()?;
        if frame.width != rule.screen_width || frame.height != rule.screen_height {
            return Err("A resolução mudou. Selecione a região novamente.".into());
        }
    }
    run.steps[index].text_check = check;
    state.store.put_run(&run)
}
#[tauri::command]
async fn compare_reading(
    state: tauri::State<'_, AppState>,
    frame: Snapshot,
    check: ocr::TextCheck,
    model_id: String,
) -> Result<Vec<reading_test::ResultRow>, String> {
    let _guard = reading_test::Guard::acquire(state.busy.clone())?;
    reading_test::compare(frame, check, model_id).await
}
#[tauri::command]
fn cancel_reading_test() {
    reading_test::cancel();
}
#[tauri::command]
async fn read_screen_text(
    state: tauri::State<'_, AppState>,
) -> Result<(Snapshot, ocr::Reading), String> {
    require_idle(&state)?;
    let frame = state.remote.snapshot()?;
    let reading = ocr::read(&frame).await?;
    Ok((frame, reading))
}
#[tauri::command]
fn edit_plan(
    state: tauri::State<AppState>,
    id: String,
    edit: plan_edit::Edit,
) -> Result<Run, String> {
    let _guard = state.execution.lock().unwrap();
    require_idle(&state)?;
    plan_edit::save(&state.store, &id, edit)
}
#[tauri::command]
fn delete_plan(state: tauri::State<AppState>, id: String, updated_at: u64) -> Result<(), String> {
    let _guard = state.execution.lock().unwrap();
    require_idle(&state)?;
    let run = state.store.run(&id)?;
    plan_edit::editable(&run, updated_at)?;
    state.store.delete_run(&id)
}
#[tauri::command]
fn update_steps(state: tauri::State<AppState>, id: String, steps: Vec<Step>) -> Result<(), String> {
    if state.busy.load(Ordering::SeqCst) {
        return Err("Pause antes de editar.".into());
    }
    let mut r = state.store.run(&id)?;
    if r.status != "ready" {
        return Err("Só é possível editar etapas antes da primeira execução.".into());
    }
    if steps.is_empty()
        || steps.len() > 30
        || steps
            .iter()
            .any(|s| s.title.trim().is_empty() || s.success.trim().is_empty())
    {
        return Err("Informe etapas com ação e condição de sucesso.".into());
    }
    for step in &steps {
        if let Some(rule) = &step.text_check {
            rule.validate()?;
        }
    }
    r.steps = steps
        .into_iter()
        .map(|mut s| {
            s.status = "pending".into();
            s.evidence = None;
            s
        })
        .collect();
    state.store.put_run(&r)
}
fn main() {
    tauri::Builder::default()
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. }
                if window.label() == "main"
                    && window
                        .app_handle()
                        .webview_windows()
                        .keys()
                        .any(|label| label == RDP_WINDOW || label.starts_with("rustdesk-")) =>
            {
                api.prevent_close();
                let _ = window.hide();
            }
            tauri::WindowEvent::Destroyed
                if window.label() == RDP_WINDOW || window.label().starts_with("rustdesk-") =>
            {
                if let Some(main) = window.app_handle().get_webview_window("main") {
                    let _ = main.show();
                    let _ = main.set_focus();
                }
            }
            _ => {}
        })
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            browser_auth::init(dir.clone());
            let bundled = app.path().resource_dir()?.join("vision/llama-server");
            let binary = if bundled.is_file() {
                bundled
            } else {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("resources/vision/llama-server")
            };
            local_engine::init(dir.clone(), binary);
            let bundled_ocr = app.path().resource_dir()?.join("ocr/agentsmith-ocr");
            ocr::init(if bundled_ocr.is_file() {
                bundled_ocr
            } else {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("resources/ocr/agentsmith-ocr")
            });
            let store =
                store::Store::new(&dir.join("agentsmith.sqlite")).map_err(std::io::Error::other)?;
            app.manage(AppState {
                store: Arc::new(store),
                remote: Arc::new(remote::Remote::new()),
                busy: Arc::new(AtomicBool::new(false)),
                execution: Arc::new(Mutex::new(executor::Control::default())),
                auth: browser_auth::AuthManager::default(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_prodigy_site,
            open_rustdesk_client,
            load_settings,
            local_engine_status,
            local_model_download,
            local_download_cancel,
            local_engine_stop,
            local_model_remove,
            local_vision_test,
            local_model_profile,
            detach_rdp,
            attach_rdp,
            toggle_rdp_fullscreen,
            browser_auth_status,
            browser_auth_start,
            browser_auth_cancel,
            test_browser_profile,
            save_settings,
            remove_profile,
            save_performance,
            snapshot_if_new,
            save_credential,
            test_profile,
            test_operator_profile,
            list_models,
            connect_machine,
            connect_saved_machine,
            machine_credential_status,
            disconnect_machine,
            session_info,
            snapshot,
            list_runs,
            plan_run,
            start_run,
            pause_run,
            stop_run,
            restart_run,
            repeat_run,
            manual_action,
            update_steps,
            edit_plan,
            delete_plan,
            set_step_text_check,
            compare_reading,
            cancel_reading_test,
            read_screen_text
        ])
        .build(tauri::generate_context!())
        .expect("Não foi possível iniciar AgentSmith")
        .run(|_, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                local_engine::shutdown();
            }
        });
}
