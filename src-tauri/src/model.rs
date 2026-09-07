use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub vendor: String,
    pub name: String,
    pub protocol: String,
    pub base_url: String,
    pub model: String,
    pub vision: bool,
    pub enabled: bool,
    #[serde(default = "default_auth")]
    pub auth_method: String,
}
fn default_auth() -> String {
    "api_key".into()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Machine {
    #[serde(default)]
    pub rustdesk_web_url: String,
    /// Rendezvous host for this destination. Empty falls back to the shared
    /// setting, and then to the public server.
    #[serde(default)]
    pub rustdesk_server: String,
    /// Base64 signing key of that server, on the same fallback.
    #[serde(default)]
    pub rustdesk_key: String,
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub domain: String,
    pub fingerprint: String,
    #[serde(default)]
    pub display: DisplaySettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DisplaySettings {
    pub width: u32,
    pub height: u32,
    pub scale: u32,
}
impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            width: 1600,
            height: 900,
            scale: 100,
        }
    }
}
impl DisplaySettings {
    pub fn validate(&self) -> Result<(), String> {
        if !(800..=2560).contains(&self.width)
            || !(600..=1440).contains(&self.height)
            || ![100, 125, 150, 200].contains(&self.scale)
        {
            return Err("Resolução ou escala fora dos limites permitidos.".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub profiles: Vec<Profile>,
    pub machines: Vec<Machine>,
    pub routes: BTreeMap<String, Vec<String>>,
    pub local_only: bool,
    pub max_actions: u32,
    #[serde(default)]
    pub performance: PerformanceSettings,
    /// Default RustDesk rendezvous server and its key, as RustDesk itself
    /// configures them once for every destination. A machine may override both.
    #[serde(default)]
    pub rustdesk: RustdeskServer,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RustdeskServer {
    #[serde(default)]
    pub server: String,
    #[serde(default)]
    pub key: String,
}
impl Settings {
    pub fn remove_profile(&mut self, id: &str) -> Result<Profile, String> {
        let index = self
            .profiles
            .iter()
            .position(|p| p.id == id)
            .ok_or("Perfil não encontrado.")?;
        let profile = self.profiles.remove(index);
        for route in self.routes.values_mut() {
            route.retain(|entry| entry != id);
        }
        Ok(profile)
    }
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            profiles: vec![],
            machines: vec![],
            routes: BTreeMap::from([
                ("planner".into(), vec![]),
                ("operator".into(), vec![]),
                ("verifier".into(), vec![]),
                ("vision".into(), vec![]),
            ]),
            local_only: false,
            max_actions: 60,
            performance: PerformanceSettings::default(),
            rustdesk: RustdeskServer::default(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PerformanceSettings {
    pub capture_interval_ms: u32,
    pub post_action_delay_ms: u32,
    pub vision_max_width: u32,
    #[serde(default = "enabled_by_default")]
    pub native_ocr: bool,
    #[serde(default = "enabled_by_default")]
    pub allow_crops: bool,
}
fn enabled_by_default() -> bool {
    true
}
impl Default for PerformanceSettings {
    fn default() -> Self {
        Self {
            capture_interval_ms: 300,
            post_action_delay_ms: 650,
            vision_max_width: 1600,
            native_ocr: true,
            allow_crops: true,
        }
    }
}
impl PerformanceSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !(20..=2000).contains(&self.capture_interval_ms)
            || self.post_action_delay_ms > 3000
            || ![0, 1280, 1600, 1920, 2560].contains(&self.vision_max_width)
        {
            return Err("Ritmo de captura, pausa ou tamanho de imagem inválido.".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Step {
    #[serde(default, rename = "textCheck")]
    pub text_check: Option<crate::ocr::TextCheck>,
    pub title: String,
    pub success: String,
    pub status: String,
    pub evidence: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    #[serde(default)]
    pub repetition: Option<crate::repetition::RepeatState>,
    #[serde(default)]
    pub progress: Option<RunProgress>,
    pub id: String,
    pub title: String,
    pub machine_id: String,
    pub instructions: String,
    pub steps: Vec<Step>,
    pub status: String,
    pub log: Vec<String>,
    pub action_count: u32,
    pub updated_at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunProgress {
    pub message: String,
    pub started_at: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub data_url: String,
    pub width: u32,
    pub height: u32,
    pub sequence: u64,
    pub captured_at: u64,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub machine_id: String,
    pub status: String,
    pub message: String,
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub fn valid_id(s: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(s)
        .map(|_| ())
        .map_err(|_| "Identificador inválido.".into())
}
pub fn validate_settings(s: &Settings) -> Result<(), String> {
    s.performance.validate()?;
    if s.profiles.len() > 100 || s.machines.len() > 500 || !(1..=500).contains(&s.max_actions) {
        return Err("Configuração fora dos limites permitidos.".into());
    }
    let mut ids = std::collections::HashSet::new();
    for p in &s.profiles {
        valid_id(&p.id)?;
        if !ids.insert(&p.id) {
            return Err("Perfis duplicados.".into());
        }
        super::llm::endpoint(p, false)?;
        if p.name.trim().is_empty() || p.model.trim().is_empty() {
            return Err("Informe nome e modelo para cada perfil.".into());
        }
    }
    ids.clear();
    for m in &s.machines {
        m.display.validate()?;
        if m.protocol == "rustdesk" {
            super::rustdesk::validate(m)?;
        }
        valid_id(&m.id)?;
        if !ids.insert(&m.id) {
            return Err("Máquinas duplicadas.".into());
        }
        if m.name.trim().is_empty() || m.host.trim().is_empty() || m.port == 0 {
            return Err("Preencha nome, endereço e porta da máquina.".into());
        }
        if [&m.host, &m.username, &m.domain, &m.fingerprint]
            .iter()
            .any(|s| s.contains(['\r', '\n', '\0']))
        {
            return Err("Campo de conexão inválido.".into());
        }
    }
    for role in ["planner", "operator", "verifier", "vision"] {
        if let Some(route) = s.routes.get(role) {
            let mut unique = std::collections::HashSet::new();
            for id in route {
                if !unique.insert(id) || !s.profiles.iter().any(|p| &p.id == id) {
                    return Err("Rota contém perfil ausente ou repetido.".into());
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod display_tests {
    use super::*;
    #[test]
    fn old_machine_gets_display_defaults_and_invalid_sizes_are_rejected() {
        let m: Machine = serde_json::from_str(r#"{"id":"test","name":"test","protocol":"rdp","host":"localhost","port":3389,"username":"test","domain":"","fingerprint":""}"#).unwrap();
        assert_eq!(
            (m.display.width, m.display.height, m.display.scale),
            (1600, 900, 100)
        );
        assert!(m.display.validate().is_ok());
        for (width, height, scale) in [
            (0, 900, 100),
            (4096, 2160, 100),
            (1600, 900, 0),
            (1600, 900, 175),
        ] {
            assert!(DisplaySettings {
                width,
                height,
                scale
            }
            .validate()
            .is_err());
        }
        for scale in [100, 125, 150, 200] {
            assert!(DisplaySettings {
                width: 1920,
                height: 1080,
                scale
            }
            .validate()
            .is_ok());
        }
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    #[test]
    fn legacy_settings_migrate_and_performance_limits_are_enforced() {
        let s: Settings = serde_json::from_str(
            r#"{"profiles":[],"machines":[],"routes":{},"localOnly":false,"maxActions":60}"#,
        )
        .unwrap();
        assert_eq!(s.performance.capture_interval_ms, 300);
        assert!(s.performance.native_ocr && s.performance.allow_crops);
        let old: PerformanceSettings = serde_json::from_str(
            r#"{"captureIntervalMs":150,"postActionDelayMs":250,"visionMaxWidth":1280}"#,
        )
        .unwrap();
        assert!(old.native_ocr && old.allow_crops);
        let step: Step =
            serde_json::from_str(r#"{"title":"A","success":"B","status":"pending"}"#).unwrap();
        assert!(step.text_check.is_none());
        assert!(s.performance.validate().is_ok());
        for (capture, pause) in [(20, 0), (50, 25), (99, 99), (2000, 3000)] {
            let p = PerformanceSettings {
                capture_interval_ms: capture,
                post_action_delay_ms: pause,
                ..PerformanceSettings::default()
            };
            assert!(p.validate().is_ok());
            let restored: PerformanceSettings =
                serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
            assert_eq!(restored.post_action_delay_ms, pause);
            assert_eq!(restored.capture_interval_ms, capture);
        }

        for (capture_interval_ms, post_action_delay_ms, vision_max_width) in [
            (0, 650, 1600),
            (300, 3001, 1600),
            (19, 0, 1600),
            (2001, 650, 1600),
            (300, 650, 999),
        ] {
            assert!(PerformanceSettings {
                capture_interval_ms,
                post_action_delay_ms,
                vision_max_width,
                ..PerformanceSettings::default()
            }
            .validate()
            .is_err());
        }
    }
}

#[cfg(test)]
mod profile_removal_tests {
    use super::*;
    #[test]
    fn removal_cleans_all_routes_and_preserves_other_profiles() {
        let profile = |id: &str| Profile {
            id: id.into(),
            vendor: "openai".into(),
            name: id.into(),
            protocol: "openai".into(),
            base_url: "official://openai".into(),
            model: "default".into(),
            vision: true,
            enabled: true,
            auth_method: "browser".into(),
        };
        let mut settings = Settings::default();
        settings.profiles = vec![profile("a"), profile("b")];
        for route in settings.routes.values_mut() {
            *route = vec!["a".into(), "b".into()];
        }
        assert_eq!(settings.remove_profile("a").unwrap().id, "a");
        assert_eq!(settings.profiles.len(), 1);
        assert!(settings
            .routes
            .values()
            .all(|r| r == &vec!["b".to_string()]));
        let before = serde_json::to_value(&settings).unwrap();
        assert!(settings.remove_profile("missing").is_err());
        assert_eq!(serde_json::to_value(&settings).unwrap(), before);
        settings.remove_profile("b").unwrap();
        assert!(settings.profiles.is_empty());
        assert!(settings.routes.values().all(Vec::is_empty));
    }
}
