//! Manual web-client integration. This window is never an AI Remote session.
use crate::model::Machine;
use reqwest::Url;
use sha2::{Digest, Sha256};
use tauri::Manager;

pub const DEFAULT_WEB_URL: &str = "https://rustdesk.com/web/";

pub fn web_url(value: &str) -> Result<Url, String> {
    let value = if value.trim().is_empty() {
        DEFAULT_WEB_URL
    } else {
        value.trim()
    };
    let url = Url::parse(value)
        .map_err(|_| "Informe um endereço HTTPS válido para o cliente web RustDesk.")?;
    if value.len() > 2048
        || value.chars().any(char::is_control)
        || url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Use uma URL HTTPS sem usuário, senha, parâmetros ou fragmento para o cliente web RustDesk.".into());
    }
    Ok(url)
}

/// The RustDesk ID a destination points at, rejected early if it could not be
/// one. Shared by the manual client and the session transport.
pub fn peer_id(machine: &Machine) -> Result<&str, String> {
    let id = machine.host.trim();
    if id.is_empty()
        || id.len() > 64
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "Informe o ID RustDesk, usando letras, números, hífen ou sublinhado, sem espaços."
                .into(),
        );
    }
    Ok(id)
}

pub fn validate(machine: &Machine) -> Result<Url, String> {
    peer_id(machine)?;
    web_url(&machine.rustdesk_web_url)
}

pub fn navigation_allowed(base: &Url, next: &Url) -> bool {
    next.scheme() == "https"
        && next.origin() == base.origin()
        && next.username().is_empty()
        && next.password().is_none()
}

/// Whether a destination can carry a plan. The manual web-client window is
/// never one of them: it stays outside AgentSmith IPC and has no frame bridge,
/// so only a machine reachable by the session transport qualifies.
pub fn require_automation(machine: &Machine) -> Result<(), String> {
    match machine.protocol.as_str() {
        "rdp" => Ok(()),
        "rustdesk" => peer_id(machine).map(|_| ()),
        _ => Err("Este conector ainda não permite execução de planos.".into()),
    }
}

pub fn open(app: &tauri::AppHandle, machine: Option<&Machine>) -> Result<(), String> {
    let (url, title, identity) = if let Some(machine) = machine {
        if machine.protocol != "rustdesk" {
            return Err("Selecione uma máquina RustDesk.".into());
        }
        (
            validate(machine)?,
            format!(
                "AgentSmith — RustDesk · ID {} · {}",
                machine.host, machine.name
            ),
            format!("{}|{}", machine.id, machine.host),
        )
    } else {
        (
            web_url(DEFAULT_WEB_URL)?,
            "AgentSmith — RustDesk · Controle manual".into(),
            "preview".into(),
        )
    };
    let digest = Sha256::digest(format!("{}|{}", identity, url).as_bytes());
    let label = format!("rustdesk-{:x}", digest)[..25].to_string();
    if let Some(window) = app.get_webview_window(&label) {
        window
            .show()
            .map_err(|_| "Não foi possível mostrar o cliente RustDesk.")?;
        return window
            .set_focus()
            .map_err(|_| "Não foi possível ativar o cliente RustDesk.".into());
    }
    let navigation_origin = url.clone();
    // External origin has no Tauri capability, credentials, scripts, or Remote bridge.
    tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::External(url))
        .title(title)
        .inner_size(1280.0, 850.0)
        .min_inner_size(800.0, 600.0)
        .incognito(true)
        .on_navigation(move |next| navigation_allowed(&navigation_origin, next))
        .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
        .build()
        .map_err(|_| "Não foi possível abrir o cliente web RustDesk.")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_client_urls_without_exposing_credentials() {
        assert_eq!(web_url("").unwrap().as_str(), DEFAULT_WEB_URL);
        assert!(web_url("https://remote.example.com:8443/web/").is_ok());
        for value in [
            "http://remote.example.com/web",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://user:secret@example.com/web",
            "https://example.com/web?pw=secret",
            "https://example.com/#/connect?id=123&pw=secret",
            "https://example.com/\nweb",
        ] {
            let error = web_url(value).unwrap_err();
            assert!(!error.contains("secret"));
        }
    }
    #[test]
    fn navigation_stays_on_configured_https_origin() {
        let base = web_url(DEFAULT_WEB_URL).unwrap();
        assert!(navigation_allowed(
            &base,
            &web_url("https://rustdesk.com/web/other").unwrap()
        ));
        for url in [
            "https://rustdesk.com.evil.example/web",
            "http://rustdesk.com/web/",
            "https://rustdesk.com:8443/web/",
            "file:///tmp/a",
            "https://user:secret@rustdesk.com/web/",
        ] {
            assert!(!navigation_allowed(&base, &Url::parse(url).unwrap()));
        }
    }
    #[test]
    fn old_machine_data_migrates_and_only_addressable_destinations_run_ai() {
        let mut machine: Machine = serde_json::from_str(r#"{"id":"demo","name":"demo","protocol":"rdp","host":"localhost","port":3389,"username":"demo","domain":"","fingerprint":""}"#).unwrap();
        assert!(machine.rustdesk_web_url.is_empty());
        assert!(require_automation(&machine).is_ok());
        machine.protocol = "rustdesk".into();
        machine.host = "123456789".into();
        assert_eq!(validate(&machine).unwrap().as_str(), DEFAULT_WEB_URL);
        // A RustDesk destination now carries plans, but only once it names a machine.
        assert!(require_automation(&machine).is_ok());
        for id in ["bad id", "host.example.com", "123\n456", "x/../y", ""] {
            machine.host = id.into();
            assert!(validate(&machine).is_err());
            assert!(require_automation(&machine).is_err());
        }
        machine.protocol = "nanokvm".into();
        assert!(require_automation(&machine).is_err());
    }
    #[test]
    fn external_client_has_no_tauri_capability() {
        let value: serde_json::Value =
            serde_json::from_str(include_str!("../../capabilities/default.json")).unwrap();
        assert!(value.get("remote").is_none());
        assert_eq!(value["windows"], serde_json::json!(["main", "remote-rdp"]));
    }
}
