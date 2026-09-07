use crate::{
    model::{now, Run, Step},
    store::Store,
};
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Edit {
    pub title: String,
    pub instructions: String,
    pub steps: Vec<EditStep>,
    pub updated_at: u64,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditStep {
    pub title: String,
    pub success: String,
}
pub fn editable(run: &Run, updated_at: u64) -> Result<(), String> {
    if ["running", "verifying", "waiting"].contains(&run.status.as_str()) {
        return Err("Pause ou pare o plano antes de editar ou excluir.".into());
    }
    if run.updated_at != updated_at {
        return Err("O plano mudou. Feche esta janela e abra novamente.".into());
    }
    Ok(())
}
pub fn save(store: &Store, id: &str, edit: Edit) -> Result<Run, String> {
    let mut run = store.run(id)?;
    editable(&run, edit.updated_at)?;
    if edit.title.trim().is_empty()
        || edit.title.chars().count() > 200
        || edit.instructions.trim().is_empty()
        || edit.instructions.len() > 30000
        || edit.steps.is_empty()
        || edit.steps.len() > 30
        || edit.steps.iter().any(|s| {
            s.title.trim().is_empty()
                || s.success.trim().is_empty()
                || s.title.len() > 4000
                || s.success.len() > 4000
        })
    {
        return Err(
            "Informe título, instruções e de 1 a 30 etapas com ação e resultado esperado.".into(),
        );
    }
    let previous = run.id.clone();
    if run.status != "ready" {
        run.id = uuid::Uuid::new_v4().to_string();
    }
    run.title = edit.title.trim().into();
    run.instructions = edit.instructions;
    run.steps = edit
        .steps
        .into_iter()
        .map(|s| Step {
            title: s.title.trim().into(),
            success: s.success.trim().into(),
            status: "pending".into(),
            evidence: None,
            text_check: None,
        })
        .collect();
    run.status = "ready".into();
    run.action_count = 0;
    run.progress = None;
    run.repetition = None;
    run.updated_at = now().max(run.updated_at.saturating_add(1));
    run.log = vec![if previous == run.id {
        "Plano editado. Revise antes de executar.".into()
    } else {
        "Nova versão do plano criada. A execução anterior foi preservada no histórico.".into()
    }];
    store.put_run(&run)?;
    Ok(run)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn run(status: &str) -> Run {
        serde_json::from_value(serde_json::json!({"id":"original","title":"Plano","machineId":"machine","instructions":"Instruções","steps":[{"title":"Ação","success":"Resultado","status":"done","evidence":"Antes"}],"status":status,"log":["histórico original"],"actionCount":3,"updatedAt":10})).unwrap()
    }
    fn edit() -> Edit {
        Edit {
            title: "Editado".into(),
            instructions: "Novo roteiro".into(),
            steps: vec![EditStep {
                title: "Nova ação".into(),
                success: "Novo resultado".into(),
            }],
            updated_at: 10,
        }
    }
    #[test]
    fn edits_ready_in_place_and_versions_executed_plans() {
        for status in [
            "ready",
            "paused",
            "cancelled",
            "completed",
            "blocked",
            "expired",
        ] {
            let store = Store::new(std::path::Path::new(":memory:")).unwrap();
            let old = run(status);
            store.put_run(&old).unwrap();
            let new = save(&store, "original", edit()).unwrap();
            assert_eq!(new.id == old.id, status == "ready");
            assert_eq!(new.status, "ready");
            assert_eq!(new.action_count, 0);
            assert_eq!(new.steps[0].status, "pending");
            assert!(
                new.steps[0].evidence.is_none()
                    && new.steps[0].text_check.is_none()
                    && new.repetition.is_none()
            );
            if status != "ready" {
                assert_eq!(store.run("original").unwrap().log, old.log);
            }
        }
    }
    #[test]
    fn rejects_active_stale_or_invalid_edits_and_deletes_only_target() {
        let store = Store::new(std::path::Path::new(":memory:")).unwrap();
        for status in ["running", "verifying", "waiting"] {
            store.put_run(&run(status)).unwrap();
            assert!(save(&store, "original", edit()).is_err());
        }
        store.put_run(&run("ready")).unwrap();
        let mut e = edit();
        e.updated_at = 9;
        assert!(save(&store, "original", e).is_err());
        let mut e = edit();
        e.steps.clear();
        assert!(save(&store, "original", e).is_err());
        let mut other = run("ready");
        other.id = "other".into();
        store.put_run(&other).unwrap();
        store.delete_run("original").unwrap();
        assert!(store.run("original").is_err());
        assert!(store.run("other").is_ok());
    }
}
