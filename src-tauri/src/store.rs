use crate::model::*;
use rusqlite::{params, Connection};
use std::{path::Path, sync::Mutex};
pub struct Store {
    db: Mutex<Connection>,
}
impl Store {
    pub fn new(path: &Path) -> Result<Self, String> {
        let db = Connection::open(path).map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS config(id INTEGER PRIMARY KEY, data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS runs(id TEXT PRIMARY KEY, data TEXT NOT NULL, updated INTEGER NOT NULL);").map_err(|e|e.to_string())?;
        let store = Self { db: Mutex::new(db) };
        for mut run in store.runs()? {
            if ["running", "verifying", "waiting"].contains(&run.status.as_str()) {
                run.status = "paused".into();
                run.progress = None;
                run.log.push("Aplicativo reiniciado. Reconecte e retome para verificar o estado antes de continuar.".into());
                store.put_run(&run)?;
            }
        }
        Ok(store)
    }
    pub fn settings(&self) -> Result<Settings, String> {
        let db = self.db.lock().map_err(|_| "Banco ocupado")?;
        let mut stmt = db
            .prepare("SELECT data FROM config WHERE id=1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(row) => serde_json::from_str(&row.get::<_, String>(0).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string()),
            None => Ok(Settings::default()),
        }
    }
    pub fn save_settings(&self, s: &Settings) -> Result<(), String> {
        validate_settings(s)?;
        self.db.lock().map_err(|_|"Banco ocupado")?.execute("INSERT INTO config(id,data) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET data=excluded.data",[serde_json::to_string(s).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn put_run(&self, r: &Run) -> Result<(), String> {
        self.db.lock().map_err(|_|"Banco ocupado")?.execute("INSERT INTO runs(id,data,updated) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET data=excluded.data,updated=excluded.updated",params![r.id,serde_json::to_string(r).map_err(|e|e.to_string())?,r.updated_at]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn delete_run(&self, id: &str) -> Result<(), String> {
        let count = self
            .db
            .lock()
            .map_err(|_| "Banco ocupado")?
            .execute("DELETE FROM runs WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        if count == 0 {
            return Err("Tarefa não encontrada.".into());
        }
        Ok(())
    }
    pub fn runs(&self) -> Result<Vec<Run>, String> {
        let db = self.db.lock().map_err(|_| "Banco ocupado")?;
        let mut stmt = db
            .prepare("SELECT data FROM runs ORDER BY updated DESC LIMIT 100")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|r| {
            serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
    }
    pub fn run(&self, id: &str) -> Result<Run, String> {
        self.runs()?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or("Tarefa não encontrada.".into())
    }
}
pub fn secret_account(id: &str, binding: &str) -> String {
    format!("{id}|{binding}")
}
#[cfg(target_os = "macos")]
pub fn save_secret(id: &str, binding: &str, secret: &str) -> Result<(), String> {
    valid_id(id)?;
    security_framework::passwords::set_generic_password(
        "AgentSmith",
        &secret_account(id, binding),
        secret.as_bytes(),
    )
    .map_err(|_| "Não foi possível salvar no Chaves do macOS.".into())
}
#[cfg(target_os = "macos")]
pub fn get_secret(id: &str, binding: &str) -> Result<String, String> {
    optional_secret(id, binding)?.ok_or("Nenhuma credencial salva para este destino.".into())
}
#[cfg(target_os = "macos")]
pub fn optional_secret(id: &str, binding: &str) -> Result<Option<String>, String> {
    valid_id(id)?;
    let bytes = match security_framework::passwords::get_generic_password(
        "AgentSmith",
        &secret_account(id, binding),
    ) {
        Ok(bytes)=>bytes,
        Err(e) if e.code()==-25300=>return Ok(None),
        Err(_)=>return Err("O macOS não liberou acesso à credencial salva. Autorize o AgentSmith no Chaves e tente conectar novamente.".into())
    };
    let value = String::from_utf8(bytes).map_err(|_| "Credencial inválida.")?;
    Ok((!value.is_empty()).then_some(value))
}
#[cfg(target_os = "macos")]
pub fn has_secret(id: &str, binding: &str) -> Result<bool, String> {
    valid_id(id)?;
    // Only query metadata; never send the password to the webview or read it on form open.
    use security_framework::item::{ItemClass, ItemSearchOptions};
    match ItemSearchOptions::new()
        .class(ItemClass::generic_password())
        .service("AgentSmith")
        .account(&secret_account(id, binding))
        .load_attributes(true)
        .load_data(false)
        .search()
    {
        Ok(items) => Ok(!items.is_empty()),
        Err(e) if e.code() == -25300 => Ok(false),
        Err(_) => Err("Não foi possível verificar o cadastro da senha no Chaves.".into()),
    }
}
#[cfg(not(target_os = "macos"))]
pub fn optional_secret(_: &str, _: &str) -> Result<Option<String>, String> {
    Err("Esta versão utiliza o Chaves do macOS.".into())
}
#[cfg(not(target_os = "macos"))]
pub fn has_secret(_: &str, _: &str) -> Result<bool, String> {
    Err("Esta versão utiliza o Chaves do macOS.".into())
}
#[cfg(not(target_os = "macos"))]
pub fn save_secret(_: &str, _: &str, _: &str) -> Result<(), String> {
    Err("Esta versão utiliza o Chaves do macOS.".into())
}
#[cfg(not(target_os = "macos"))]
pub fn get_secret(_: &str, _: &str) -> Result<String, String> {
    Err("Esta versão utiliza o Chaves do macOS.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scheduled_repetition_is_paused_on_reopen_without_extending_deadline() {
        let path = std::env::temp_dir().join(format!(
            "agentsmith-loop-test-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let repetition = crate::repetition::RepeatOptions {
            schedule: crate::repetition::Schedule::Duration { minutes: 30 },
            interval_seconds: 5,
        }
        .resolve(now())
        .unwrap();
        let ends_at = repetition.ends_at;
        {
            let store = Store::new(&path).unwrap();
            let run = Run {
                repetition: Some(repetition),
                progress: None,
                id: "scheduled".into(),
                title: "Test".into(),
                machine_id: "machine".into(),
                instructions: "Test".into(),
                steps: vec![],
                status: "waiting".into(),
                log: vec![],
                action_count: 0,
                updated_at: now(),
            };
            store.put_run(&run).unwrap();
        }
        {
            let store = Store::new(&path).unwrap();
            let run = store.run("scheduled").unwrap();
            assert_eq!(run.status, "paused");
            assert_eq!(run.repetition.unwrap().ends_at, ends_at);
        }
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn restart_preserves_verified_steps_and_pauses_interrupted_work() {
        let path =
            std::env::temp_dir().join(format!("agentsmith-test-{}.sqlite", uuid::Uuid::new_v4()));
        {
            let store = Store::new(&path).unwrap();
            let run = Run {
                repetition: None,
                progress: None,
                id: "fixture".into(),
                title: "Fixture".into(),
                machine_id: "machine".into(),
                instructions: "fixture".into(),
                steps: vec![Step {
                    text_check: None,
                    title: "Done".into(),
                    success: "Visible".into(),
                    status: "done".into(),
                    evidence: Some("Confirmed".into()),
                }],
                status: "running".into(),
                log: vec![],
                action_count: 2,
                updated_at: now(),
            };
            store.put_run(&run).unwrap();
        }
        {
            let store = Store::new(&path).unwrap();
            let r = store.run("fixture").unwrap();
            assert_eq!(r.status, "paused");
            assert_eq!(r.steps[0].status, "done");
            assert_eq!(r.action_count, 2);
        }
        let _ = std::fs::remove_file(path);
    }
}
