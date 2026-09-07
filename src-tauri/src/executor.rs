use crate::{
    llm,
    model::*,
    remote::{Action, Remote},
    store::Store,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Deserialize;
use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

// Keep the baseline until the next input: Windows may repaint after the first capture.
#[derive(Default)]
struct InputProgress {
    baseline: Option<Snapshot>,
    counted: bool,
    stagnant: u32,
}
impl InputProgress {
    fn observe(&mut self, frame: &Snapshot) -> bool {
        let Some(before) = self.baseline.as_ref() else {
            return false;
        };
        let unchanged =
            crate::vision::region_unchanged(before, frame, crate::vision::Region::full(frame));
        if unchanged {
            if !self.counted {
                self.stagnant += 1;
                self.counted = true;
            }
        } else {
            self.stagnant = 0;
            self.baseline = None;
            self.counted = false;
        }
        unchanged
    }
    fn sent(&mut self, before: Snapshot) {
        self.observe(&before);
        self.baseline = Some(before);
        self.counted = false;
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    title: String,
    steps: Vec<PlannedStep>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlannedStep {
    title: String,
    success: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Verdict {
    verified: bool,
    evidence: String,
}
pub async fn plan(store: &Store, machine_id: String, instructions: String) -> Result<Run, String> {
    if instructions.trim().is_empty() || instructions.len() > 30000 {
        return Err("Informe um roteiro com até 30 mil caracteres.".into());
    }
    let s = store.settings()?;
    if !s.machines.iter().any(|m| m.id == machine_id) {
        return Err("Selecione uma máquina cadastrada.".into());
    }
    let prompt=format!("Divida o roteiro em 1 a 30 etapas curtas operáveis por mouse e teclado. Cada etapa precisa de um critério de sucesso que possa ser conferido visualmente. Preserve números, nomes e restrições do usuário. Não acrescente trabalho não solicitado. Formato: {{\"title\":\"Título curto\",\"steps\":[{{\"title\":\"Ação\",\"success\":\"Condição observável\"}}]}}. Roteiro do usuário:\n{instructions}");
    let (p, provider) = llm::routed_validated(
        &s,
        "planner",
        &crate::harness::system("plan"),
        &prompt,
        None,
        |text, _| {
            let p: Plan = llm::parse_json(text)?;
            if p.title.trim().is_empty()
                || p.steps.is_empty()
                || p.steps.len() > 30
                || p.steps
                    .iter()
                    .any(|s| s.title.trim().is_empty() || s.success.trim().is_empty())
            {
                return Err("Retorne title e 1..30 steps com title/success não vazios.".into());
            }
            Ok(p)
        },
    )
    .await?;
    if p.title.trim().is_empty()
        || p.steps.is_empty()
        || p.steps.len() > 30
        || p.steps
            .iter()
            .any(|s| s.title.trim().is_empty() || s.success.trim().is_empty())
    {
        return Err("O modelo não gerou um roteiro verificável.".into());
    }
    let run = Run {
        repetition: None,
        progress: None,
        id: uuid::Uuid::new_v4().to_string(),
        title: p.title,
        machine_id,
        instructions,
        steps: p
            .steps
            .into_iter()
            .map(|s| Step {
                text_check: None,
                title: s.title,
                success: s.success,
                status: "pending".into(),
                evidence: None,
            })
            .collect(),
        status: "ready".into(),
        log: vec![format!(
            "Roteiro criado por {provider}. Revise as etapas antes de executar."
        )],
        action_count: 0,
        updated_at: now(),
    };
    store.put_run(&run)?;
    Ok(run)
}
async fn checked<T>(
    remote: &Remote,
    epoch: u64,
    future: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::pin!(future);
    loop {
        if remote.epoch.load(Ordering::SeqCst) != epoch {
            return Err("Execução pausada pelo operador.".into());
        }
        tokio::select! {result=&mut future=>return result,_=tokio::time::sleep(std::time::Duration::from_millis(50))=>{}}
    }
}
fn checkpoint(store: &Store, run: &mut Run) -> Result<(), String> {
    run.updated_at = now();
    if run.log.len() > 300 {
        run.log.drain(..run.log.len() - 300);
    }
    store.put_run(run)
}
fn report(store: &Store, run: &mut Run, message: impl Into<String>) -> Result<(), String> {
    run.progress = Some(RunProgress {
        message: message.into(),
        started_at: now(),
    });
    checkpoint(store, run)
}
#[cfg(test)]
fn operator_response<T: serde::de::DeserializeOwned>(
    text: &str,
    provider: &str,
    role: &str,
) -> Result<T, String> {
    llm::parse_json(text).map_err(|_|format!("{provider} não forneceu uma resposta válida para {role}. Troque o modelo em Roteamento de IA antes de retomar. Nenhuma nova entrada foi enviada ao Windows."))
}
pub fn frame_compatible(a: &Snapshot, b: &Snapshot) -> bool {
    if a.width != b.width || a.height != b.height {
        return false;
    }
    if a.data_url == b.data_url {
        return true;
    }
    let decode = |s: &Snapshot| {
        let data = STANDARD
            .decode(s.data_url.strip_prefix("data:image/png;base64,")?)
            .ok()?;
        Some(
            image::load_from_memory(&data)
                .ok()?
                .thumbnail(160, 100)
                .to_rgb8(),
        )
    };
    let (Some(a), Some(b)) = (decode(a), decode(b)) else {
        return false;
    };
    if a.dimensions() != b.dimensions() {
        return false;
    }
    let changed = a
        .pixels()
        .zip(b.pixels())
        .filter(|(a, b)| a.0.iter().zip(b.0.iter()).any(|(x, y)| x.abs_diff(*y) > 35))
        .count();
    changed * 100 < a.width() as usize * a.height() as usize * 8
}
enum TextChoice {
    Done(String, String),
    Action(Action, String),
    Vision(String),
}
fn validated_text_choice(
    text: &str,
    provider: &str,
    read: &crate::ocr::Reading,
) -> Result<TextChoice, String> {
    use crate::observation::Decision;
    let decision: Decision = llm::parse_json(text)?;
    if let Decision::NeedVision { ref reason } = decision {
        if reason.trim().is_empty() {
            return Err("need_vision exige reason.".into());
        }
        return Ok(TextChoice::Vision(reason.clone()));
    }
    let action = match decision.action(read) {
        Ok(action) => action,
        Err(reason) if reason.contains("Alvo OCR") => return Ok(TextChoice::Vision(reason)),
        Err(reason) => return Err(reason),
    };
    match &action {
        Action::Wait { seconds } if !(1..=10).contains(seconds) => {
            return Err("wait exige seconds entre 1 e 10.".into())
        }
        Action::Wait { .. } | Action::Blocked { .. } => {}
        Action::TypeText { text } if text.trim().is_empty() => {
            return Err("type_text exige texto não vazio.".into())
        }
        _ => {
            crate::remote::action_commands(&action, read.width, read.height)?;
        }
    }
    Ok(TextChoice::Action(action, provider.into()))
}
async fn text_choice(
    store: &Store,
    run: &mut Run,
    s: &Settings,
    context: &str,
    read: &crate::ocr::Reading,
    explicit_rule: bool,
) -> Result<TextChoice, String> {
    use crate::observation::VerdictStatus;
    let system = crate::harness::system("text-action");
    let context = format!("{context}{}", crate::observation::context(read));
    let combined = s
        .routes
        .get("operator")
        .is_some_and(|ids| !ids.is_empty() && Some(ids) == s.routes.get("verifier"));
    if combined {
        run.status = "running".into();
        report(store, run, "Observando e decidindo em uma chamada")?;
        let rule = if explicit_rule {
            "O motor ainda não confirmou o texto exato: não proponha sucesso."
        } else {
            ""
        };
        let prompt = format!(
            "{context}\n{rule}\n{}",
            crate::harness::actions(false, !explicit_rule)
        );
        let start = std::time::Instant::now();
        let (choice, provider) = llm::routed_validated(s,"operator",&crate::harness::system(if explicit_rule {"text-action"} else {"text-combined"}),&prompt,None,|text,provider| {
            if let Ok(verdict) = llm::parse_json::<crate::observation::Verdict>(text) {
                if explicit_rule || verdict.status != VerdictStatus::Verified { return Err("Escolha uma ação ou need_vision; o motor ainda não confirmou esta etapa.".into()); }
                if !verdict.supported(read) { return Ok(TextChoice::Vision("Evidência OCR insuficiente ou incerta.".into())); }
                return Ok(TextChoice::Done(verdict.evidence,provider.into()));
            }
            validated_text_choice(text,provider,read)
        }).await?;
        run.log.push(format!(
            "{provider} · observação + ação: {:.1}s · uma chamada, sem imagem.",
            start.elapsed().as_secs_f64()
        ));
        return Ok(choice);
    }
    let evidence = if explicit_rule {
        "O motor ainda não confirmou o texto exato na região. Não declare sucesso e não altere o critério.".to_string()
    } else {
        run.status = "verifying".into();
        report(store, run, "Verificando com OCR e modelo de texto")?;
        let prompt = format!("{context}\n{}", crate::harness::TEXT_VERIFY);
        let started = std::time::Instant::now();
        let (verdict, provider) = llm::routed_validated(
            s,
            "verifier",
            &crate::harness::system("text-verify"),
            &prompt,
            None,
            |text, _| {
                let v: crate::observation::Verdict = llm::parse_json(text)?;
                if v.evidence.trim().is_empty() || v.evidence.chars().count() > 300 {
                    return Err("evidence deve conter um fato curto.".into());
                }
                Ok(v)
            },
        )
        .await?;
        run.log.push(format!(
            "{provider} · verificação por texto: {:.1}s · sem imagem.",
            started.elapsed().as_secs_f64()
        ));

        if !verdict.supported(read) {
            return Ok(TextChoice::Vision(
                "Evidência OCR insuficiente ou incerta.".into(),
            ));
        }
        match verdict.status {
            VerdictStatus::Verified => return Ok(TextChoice::Done(verdict.evidence, provider)),
            VerdictStatus::NeedVision => return Ok(TextChoice::Vision(verdict.evidence)),
            VerdictStatus::NotVerified => verdict.evidence,
        }
    };
    run.status = "running".into();
    report(store, run, "Escolhendo ação com OCR e modelo de texto")?;
    let prompt = format!(
        "{context}\nVerificação: {evidence}\n{}",
        crate::harness::actions(false, false)
    );
    let started = std::time::Instant::now();
    let (choice, provider) =
        llm::routed_validated(s, "operator", &system, &prompt, None, |text, provider| {
            validated_text_choice(text, provider, read)
        })
        .await?;
    run.log.push(format!(
        "{provider} · próxima ação por texto: {:.1}s · sem imagem.",
        started.elapsed().as_secs_f64()
    ));
    Ok(choice)
}
async fn execute(
    store: &Store,
    remote: &Remote,
    run: &mut Run,
    s: &Settings,
    epoch: u64,
) -> Result<(), String> {
    execute_with_observations(
        store,
        remote,
        run,
        s,
        epoch,
        crate::observation::Cache::default(),
    )
    .await
}
async fn execute_with_observations(
    store: &Store,
    remote: &Remote,
    run: &mut Run,
    s: &Settings,
    epoch: u64,
    mut observations: crate::observation::Cache,
) -> Result<(), String> {
    for role in ["operator", "verifier"] {
        if !s.routes.get(role).is_some_and(|ids| {
            ids.iter().any(|id| {
                s.profiles
                    .iter()
                    .any(|p| &p.id == id && p.enabled && llm::endpoint(p, s.local_only).is_ok())
            })
        }) {
            return Err(format!("Configure um modelo para {role}."));
        }
    }
    let visual_settings = llm::visual_settings(s);
    for index in 0..run.steps.len() {
        if run.steps[index].status == "done" {
            continue;
        }
        run.steps[index].status = "active".into();
        checkpoint(store, run)?;
        let mut waits = 0;
        let mut focus: Option<crate::vision::Region> = None;
        let mut crop_age = 0;
        let mut saw_ocr_nonmatch = false;
        let mut input_progress = InputProgress::default();
        let mut recent_inputs: Vec<serde_json::Value> = Vec::new();
        loop {
            if remote.epoch.load(Ordering::SeqCst) != epoch {
                return Err("Execução pausada pelo operador.".into());
            }
            if run.repetition.as_ref().is_some_and(|r| now() >= r.ends_at) {
                return Err("Período de repetição encerrado.".into());
            }
            if remote.info.lock().unwrap().machine_id != run.machine_id {
                return Err("A sessão ativa não pertence à máquina da tarefa.".into());
            }
            if run.action_count >= s.max_actions {
                return Err("Limite de ações atingido. Revise o histórico e ajuste o limite antes de retomar.".into());
            }
            let mut frame = remote.snapshot()?;
            let mut unchanged_after_input = input_progress.observe(&frame);
            if unchanged_after_input {
                // A new captured frame need not yet contain the Windows response.
                // Give slow repaints a bounded opportunity, without repeating input.
                report(store, run, "Aguardando resposta visual do Windows")?;
                for _ in 0..6 {
                    checked(remote, epoch, async {
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        Ok(())
                    })
                    .await?;
                    frame = remote.snapshot()?;
                    unchanged_after_input = input_progress.observe(&frame);
                    if !unchanged_after_input {
                        break;
                    }
                }
            }
            if input_progress.stagnant >= 3 {
                return Err("A tela não apresentou progresso após três entradas. Revise a tarefa antes de retomar.".into());
            }
            // A narrow explicit criterion can be checked without reading the entire desktop.
            let rule_reading = if let Some(rule) = &run.steps[index].text_check {
                if rule.screen_width != frame.width || rule.screen_height != frame.height {
                    return Err(
                        "A resolução mudou. Ajuste a região da verificação OCR antes de retomar."
                            .into(),
                    );
                }
                let region = rule.region;
                report(store, run, "Conferindo texto esperado na região")?;
                let (read, reused) =
                    checked(remote, epoch, observations.read(&frame, region)).await?;
                run.log.push(if reused {
                    "Observação OCR reutilizada: região sem alterações.".into()
                } else {
                    format!(
                        "OCR da região: {} ms · {} textos.",
                        read.elapsed_ms,
                        read.lines.len()
                    )
                });
                Some(read)
            } else {
                None
            };
            let text_check = run.steps[index].text_check.clone();
            if let Some(rule) = &text_check {
                if rule.screen_width != frame.width || rule.screen_height != frame.height {
                    return Err(
                        "A resolução mudou. Ajuste a região da verificação OCR antes de retomar."
                            .into(),
                    );
                }
                let matched = rule_reading.as_ref().is_some_and(|r| rule.matches(r));
                if !matched {
                    saw_ocr_nonmatch = true;
                }
                if matched
                    && (run.repetition.is_none() || saw_ocr_nonmatch)
                    && crate::vision::region_unchanged(&frame, &remote.snapshot()?, rule.region)
                {
                    run.steps[index].status = "done".into();
                    run.steps[index].evidence = Some(format!(
                        "OCR nativo: texto exato confirmado na região definida: {}",
                        rule.expected
                    ));
                    run.log.push(format!(
                        "Etapa {} confirmada por OCR, sem consulta ao LLM.",
                        index + 1
                    ));
                    checkpoint(store, run)?;
                    break;
                }
            }
            if let Some(last) = recent_inputs.last_mut() {
                last["screen_changed"] = serde_json::json!(!unchanged_after_input);
            }
            let context = crate::harness::context(run, index, &recent_inputs);
            let reading = if s.performance.native_ocr {
                report(store, run, "Lendo textos com OCR nativo")?;
                match checked(
                    remote,
                    epoch,
                    observations.read(&frame, crate::vision::Region::full(&frame)),
                )
                .await
                {
                    Ok((read, reused)) => {
                        run.log.push(if reused {
                            "Observação OCR reutilizada: tela sem alterações.".into()
                        } else {
                            format!(
                                "OCR nativo: {} ms · {} textos.",
                                read.elapsed_ms,
                                read.lines.len()
                            )
                        });
                        Some(read)
                    }
                    Err(error) => {
                        if remote.epoch.load(Ordering::SeqCst) != epoch {
                            return Err(error);
                        }
                        run.log
                            .push("OCR indisponível; solicitando apoio visual.".into());
                        None
                    }
                }
            } else {
                None
            };
            if crop_age >= 2 {
                focus = None;
            }
            let choice = if unchanged_after_input || waits >= 2 || focus.is_some() {
                TextChoice::Vision(
                    "Sem progresso após entrada/esperas ou há um recorte solicitado.".into(),
                )
            } else if let Some(read) = reading
                .as_ref()
                .filter(|read| read.lines.iter().any(|l| l.confidence >= 0.8))
            {
                match checked(
                    remote,
                    epoch,
                    text_choice(store, run, s, &context, read, text_check.is_some()),
                )
                .await
                {
                    Ok(choice) => choice,
                    Err(error) => {
                        if remote.epoch.load(Ordering::SeqCst) != epoch {
                            return Err(error);
                        }
                        run.log.push(format!(
                            "O contrato de texto foi rejeitado: {error} Solicitando apoio visual."
                        ));
                        TextChoice::Vision("Resposta de texto indisponível ou inválida.".into())
                    }
                }
            } else {
                TextChoice::Vision("OCR sem informação suficiente.".into())
            };
            // Textual success is a proposal, not proof of a screen state. Explicit
            // OCR rules above are the only completion path that skips visual review.
            let (choice, confirmation_settings) = match choice {
                TextChoice::Done(evidence, provider) => {
                    drop(evidence); // Discard the unconfirmed claim; do not prime the visual reviewer.
                    run.log.push(format!(
                        "{provider} propôs concluir a etapa; aguardando confirmação visual."
                    ));
                    (
                        TextChoice::Vision(
                            "Confirmar o critério na tela atual antes de concluir a etapa.".into(),
                        ),
                        llm::confirmation_settings(&visual_settings, &provider),
                    )
                }
                other => (other, visual_settings.clone()),
            };
            let (action, provider, prepared, current, text_path) = match choice {
                TextChoice::Done(_, _) => unreachable!("Text proposals must pass visual review"),
                TextChoice::Action(action, provider) => {
                    let prepared = crate::vision::Prepared {
                        frame: frame.clone(),
                        region: crate::vision::Region::full(&frame),
                    };
                    (action, provider, prepared, frame.clone(), true)
                }
                TextChoice::Vision(reason) => {
                    run.log.push(format!("Apoio visual solicitado: {reason}"));
                    if visual_settings
                        .routes
                        .get("vision")
                        .is_none_or(|ids| ids.is_empty())
                    {
                        return Err("Esta etapa precisa de apoio visual. Selecione um modelo com visão em Roteamento de IA → Apoio visual.".into());
                    }
                    report(store, run, "Consultando apoio visual")?;
                    let prepared = crate::vision::prepare(
                        &frame,
                        s.performance.vision_max_width,
                        if s.performance.allow_crops {
                            focus.filter(|_| crop_age < 2)
                        } else {
                            None
                        },
                    )?;
                    let sent_frame = &prepared.frame;
                    let ocr_context = reading
                        .as_ref()
                        .map(|r| crate::ocr::context(r, &prepared))
                        .unwrap_or_default();
                    run.status = "verifying".into();
                    report(
                        store,
                        run,
                        format!(
                            "Conferindo a tela · etapa {} de {}",
                            index + 1,
                            run.steps.len()
                        ),
                    )?;
                    let prompt =
                        format!("{context}{ocr_context}\n{}", crate::harness::VISUAL_VERIFY);
                    let (verdict, provider) = if let Some(rule) = &text_check {
                        (Verdict{verified:false,evidence:format!("A regra exige o texto exato {:?} na região definida. OCR ainda não confirmou uma nova ocorrência. No loop, faça o resultado mudar antes de conferi-lo novamente.",rule.expected)},"OCR nativo".to_string())
                    } else {
                        let started = std::time::Instant::now();
                        let (verdict, provider) = checked(
                            remote,
                            epoch,
                            llm::routed_validated(
                                &confirmation_settings,
                                "vision",
                                &crate::harness::system("visual-verify"),
                                &prompt,
                                Some(&sent_frame.data_url),
                                |text, _| {
                                    let v: Verdict = llm::parse_json(text)?;
                                    if v.evidence.trim().is_empty()
                                        || v.evidence.chars().count() > 300
                                    {
                                        return Err("evidence deve conter um fato curto.".into());
                                    }
                                    Ok(v)
                                },
                            ),
                        )
                        .await?;
                        run.log.push(format!(
                            "{provider} · verificação: {:.1}s · imagem {} × {}.",
                            started.elapsed().as_secs_f64(),
                            sent_frame.width,
                            sent_frame.height
                        ));

                        (verdict, provider)
                    };
                    if verdict.verified {
                        if !crate::vision::region_unchanged(
                            &frame,
                            &remote.snapshot()?,
                            prepared.region,
                        ) {
                            continue;
                        }
                        if verdict.evidence.trim().is_empty() {
                            return Err("O verificador não forneceu evidência.".into());
                        }
                        run.steps[index].status = "done".into();
                        run.steps[index].evidence = Some(verdict.evidence.clone());
                        run.log.push(format!(
                            "Etapa {} verificada por {}: {}",
                            index + 1,
                            provider,
                            verdict.evidence
                        ));
                        checkpoint(store, run)?;
                        break;
                    }
                    run.status = "running".into();
                    report(
                        store,
                        run,
                        format!(
                            "Escolhendo a próxima ação · etapa {} de {}",
                            index + 1,
                            run.steps.len()
                        ),
                    )?;
                    let current = remote.snapshot()?;
                    let prepared = crate::vision::prepare(
                        &current,
                        s.performance.vision_max_width,
                        if s.performance.allow_crops {
                            focus.filter(|_| crop_age < 2)
                        } else {
                            None
                        },
                    )?;
                    let sent_frame = &prepared.frame;
                    let crop_hint = if s.performance.allow_crops {
                        "Se precisar de detalhe, use {\"kind\":\"inspect\",\"x\":0,\"y\":0,\"width\":300,\"height\":200} para ampliar uma região da imagem; não envia entrada ao Windows. A visão geral retorna após duas ações ou após teclado/rolagem. Coordenadas em pixels da imagem enviada, NÃO normalizadas em 0–1000."
                    } else {
                        "Não solicite recortes."
                    };
                    let current_ocr = if frame.data_url == current.data_url {
                        reading
                            .as_ref()
                            .map(|r| crate::ocr::context(r, &prepared))
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    let context=format!("{context}{current_ocr}\n{crop_hint}\nA imagem cobre a região x={}, y={}, largura={}, altura={} da sessão, redimensionada para {}x{}. Use SOMENTE coordenadas na imagem enviada. Não some o deslocamento da região.",prepared.region.x,prepared.region.y,prepared.region.width,prepared.region.height,sent_frame.width,sent_frame.height);
                    let prompt = format!(
                        "{context}\nVerificação: {}\n{}",
                        verdict.evidence,
                        crate::harness::actions(true, false)
                    );
                    let started = std::time::Instant::now();
                    let (action, provider) = checked(
                        remote, epoch,
                        llm::routed_validated(&visual_settings, "vision", &crate::harness::system("visual-action"), &prompt, Some(&sent_frame.data_url), |text, provider| {
                            let result = crate::observation::validated_visual_action(text, &prepared, &current);
                            if let Err(error) = &result {
                                run.log.push(format!("{provider}: resposta visual rejeitada: {error} Nenhuma entrada enviada; solicitando nova decisão."));
                                report(store, run, "Corrigindo resposta do apoio visual")?;
                            }
                            result
                        }),
                    ).await.map_err(|error| format!("O apoio visual não forneceu uma ação válida para a etapa {}. {error}", index + 1))?;
                    run.log.push(format!(
                        "{provider} · próxima ação: {:.1}s · imagem {} × {}.",
                        started.elapsed().as_secs_f64(),
                        sent_frame.width,
                        sent_frame.height
                    ));
                    (action, provider, prepared, current, false)
                }
            };
            run.action_count += 1;
            if let Some(repetition) = &mut run.repetition {
                repetition.total_actions += 1;
            }
            let latest = remote.snapshot()?;
            if (text_path
                && !crate::vision::region_unchanged(
                    &current,
                    &latest,
                    crate::vision::Region::full(&current),
                ))
                || !frame_compatible(&current, &latest)
                || (prepared.region != crate::vision::Region::full(&current)
                    && !crate::vision::region_unchanged(&current, &latest, prepared.region))
            {
                run.log.push(
                    "A tela mudou enquanto o modelo respondia. Observando novamente antes de agir."
                        .into(),
                );
                checkpoint(store, run)?;
                continue;
            }
            crop_age += 1;
            match action {
                Action::Inspect {
                    x,
                    y,
                    width,
                    height,
                } => {
                    if !s.performance.allow_crops {
                        return Err("Recortes estão desativados.".into());
                    }
                    let region = crate::vision::Region {
                        x,
                        y,
                        width,
                        height,
                    };
                    region.validate(current.width, current.height)?;
                    focus = Some(region);
                    crop_age = 0;
                    run.log.push(format!("Recorte solicitado: {x},{y} · {width} × {height}. Nenhuma entrada enviada."));
                    checkpoint(store, run)?;
                }
                Action::Blocked { reason } => {
                    return Err(format!("{provider} · etapa {}: {reason}", index + 1))
                }
                Action::Wait { seconds } => {
                    focus = None;
                    waits += 1;
                    if waits > 8 {
                        return Err(
                            "A tela não apresentou progresso após várias observações.".into()
                        );
                    }
                    let seconds = seconds.clamp(1, 10);
                    report(
                        store,
                        run,
                        format!("Aguardando o Windows por {seconds} segundos"),
                    )?;
                    run.log.push(format!("{provider}: aguardando {seconds}s."));
                    checkpoint(store, run)?;
                    checked(remote, epoch, async {
                        tokio::time::sleep(std::time::Duration::from_secs(seconds as u64)).await;
                        Ok(())
                    })
                    .await?;
                }
                Action::StepDone { .. } => {
                    run.log
                        .push("O operador solicitou nova verificação.".into());
                }
                ref input => {
                    waits = 0;
                    run.log.push("Próxima entrada preparada. Em uma retomada, conferir o resultado antes de repetir.".into());
                    checkpoint(store, run)?;
                    report(store, run, "Enviando mouse ou teclado ao Windows")?;
                    if matches!(
                        action,
                        Action::Key { .. } | Action::Scroll { .. } | Action::TypeText { .. }
                    ) {
                        focus = None;
                    }
                    let before_input = remote.snapshot()?.sequence;
                    if run.repetition.as_ref().is_some_and(|r| now() >= r.ends_at) {
                        return Err("Período de repetição encerrado.".into());
                    }
                    remote.act(input, epoch).await?;
                    input_progress.sent(current.clone());
                    recent_inputs.push(serde_json::json!({"action":crate::harness::input_summary(input),"screen_changed":null}));
                    if recent_inputs.len() > 4 {
                        recent_inputs.remove(0);
                    }
                    let label = match input {
                        Action::TypeText { text } => {
                            format!("digitação de {} caracteres", text.chars().count())
                        }
                        Action::Click { x, y } => format!("clique em {x},{y}"),
                        Action::DoubleClick { .. } => "clique duplo".into(),
                        Action::RightClick { .. } => "clique direito".into(),
                        Action::Key { keys } => format!("teclas {}", keys.join("+")),
                        _ => "rolagem".into(),
                    };
                    run.log
                        .push(format!("{provider}: {label}. Aguardando verificação."));
                    report(
                        store,
                        run,
                        "Entrada enviada · aguardando atualização da tela",
                    )?;
                    checked(remote, epoch, async {
                        tokio::time::sleep(std::time::Duration::from_millis(
                            s.performance.post_action_delay_ms as u64,
                        ))
                        .await;
                        remote.wait_for_new_frame(before_input).await
                    })
                    .await?;
                }
            }
        }
    }
    run.status = "completed".into();
    run.progress = Some(RunProgress {
        message: "Todas as etapas foram concluídas e verificadas.".into(),
        started_at: now(),
    });
    run.log
        .push("Todas as etapas tiveram suas condições de sucesso verificadas.".into());
    Ok(())
}
fn expire(run: &mut Run) {
    run.status = "expired".into();
    let count = run.repetition.as_ref().map_or(0, |r| r.completed_cycles);
    let message =
        format!("Período encerrado · {count} ciclos concluídos. Nenhuma nova ação será enviada.");
    run.progress = Some(RunProgress {
        message: message.clone(),
        started_at: now(),
    });
    run.log.push(message);
}
async fn until_deadline<T>(
    remote: &Remote,
    ends_at: u64,
    invalidate: bool,
    future: impl Future<Output = Result<T, String>>,
) -> Option<Result<T, String>> {
    let deadline = async {
        while now() < ends_at {
            tokio::time::sleep(std::time::Duration::from_millis(
                ends_at.saturating_sub(now()).min(1000),
            ))
            .await;
        }
    };
    tokio::select! {
        biased;
        _ = deadline => { if invalidate { remote.epoch.fetch_add(1, Ordering::SeqCst); } None }
        result = future => Some(result),
    }
}
fn advance_weekly_window(store: &Store, run: &mut Run) -> Result<bool, String> {
    if !run
        .repetition
        .as_mut()
        .unwrap()
        .advance_weekly_window(now())?
    {
        return Ok(false);
    }
    run.status = "waiting".into();
    run.log.push("Horário encerrado. Aguardando o próximo dia selecionado; ciclos incompletos não contam como concluídos.".into());
    report(
        store,
        run,
        "Aguardando o próximo dia e horário selecionados",
    )?;
    Ok(true)
}
async fn execute_repeated(
    store: &Store,
    remote: &Remote,
    run: &mut Run,
    s: &Settings,
    epoch: u64,
) -> Result<(), String> {
    use crate::repetition::Next;
    if run.repetition.is_none() {
        return execute(store, remote, run, s, epoch).await;
    }
    loop {
        if remote.epoch.load(Ordering::SeqCst) != epoch {
            return Err("Execução pausada pelo operador.".into());
        }
        match run.repetition.as_ref().unwrap().next(now()) {
            Next::Expired => {
                if advance_weekly_window(store, run)? {
                    continue;
                }
                expire(run);
                return Ok(());
            }
            Next::Wait(until) => {
                run.status = "waiting".into();
                let message = if run.repetition.as_ref().unwrap().weekly.is_some()
                    && now() < run.repetition.as_ref().unwrap().starts_at
                {
                    "Aguardando o próximo dia e horário selecionados"
                } else if run.repetition.as_ref().unwrap().cycle == 0 {
                    "Aguardando o horário de início"
                } else {
                    "Aguardando o próximo ciclo"
                };
                report(store, run, message)?;
                // Re-check wall time each second, including after the Mac wakes from sleep.
                checked(remote, epoch, async {
                    while now() < until {
                        tokio::time::sleep(std::time::Duration::from_millis(
                            until.saturating_sub(now()).min(1000),
                        ))
                        .await;
                    }
                    Ok(())
                })
                .await?;
                continue;
            }
            Next::Start => {
                let repetition = run.repetition.as_mut().unwrap();
                repetition.cycle += 1;
                repetition.between_cycles = false;
                run.log.push(format!(
                    "Ciclo {} iniciado desde a primeira etapa.",
                    repetition.cycle
                ));
                run.action_count = 0;
                for step in &mut run.steps {
                    step.status = "pending".into();
                    step.evidence = None;
                }
                checkpoint(store, run)?;
            }
            Next::Continue => {}
        }
        let ends_at = run.repetition.as_ref().unwrap().ends_at;
        let weekly = run.repetition.as_ref().unwrap().weekly.is_some();
        match until_deadline(
            remote,
            ends_at,
            !weekly,
            execute(store, remote, run, s, epoch),
        )
        .await
        {
            None => {
                remote.release().await?;
                if advance_weekly_window(store, run)? {
                    continue;
                }
                expire(run);
                return Ok(());
            }
            Some(Err(_)) if now() >= ends_at => {
                remote.release().await?;
                if advance_weekly_window(store, run)? {
                    continue;
                }
                expire(run);
                return Ok(());
            }
            Some(result) => result?,
        }
        let repetition = run.repetition.as_mut().unwrap();
        repetition.complete_cycle(now());
        run.log.push(format!(
            "Ciclo {} concluído e verificado.",
            repetition.cycle
        ));
        // Do not publish a transient completed status while the repetition still owns the session.
        run.status = "waiting".into();
        report(
            store,
            run,
            "Ciclo concluído · aguardando a próxima repetição",
        )?;
    }
}
// Serializes start, stop requests and final persistence across both app windows.
#[derive(Default)]
pub struct Control {
    active: Option<(String, bool)>,
}
const STOPPED: &str = "Tarefa parada pelo operador. Histórico preservado. Use Reiniciar para executar este roteiro desde a primeira etapa.";
fn mark_stopped(run: &mut Run) {
    run.status = "cancelled".into();
    run.progress = Some(RunProgress {
        message: STOPPED.into(),
        started_at: now(),
    });
    run.log.push(STOPPED.into());
}
pub async fn stop(
    store: &Store,
    remote: &Remote,
    control: &Mutex<Control>,
    id: &str,
) -> Result<(), String> {
    let active = {
        let mut guard = control.lock().unwrap();
        if let Some((_, requested)) = guard
            .active
            .as_mut()
            .filter(|(active_id, _)| active_id == id)
        {
            *requested = true;
            remote.epoch.fetch_add(1, Ordering::SeqCst);
            true
        } else {
            let mut run = store.run(id)?;
            if !["completed", "cancelled", "expired"].contains(&run.status.as_str()) {
                mark_stopped(&mut run);
                checkpoint(store, &mut run)?;
            }
            false
        }
    };
    if active {
        remote.release().await?;
    }
    Ok(())
}
/// What the operator is told when a task ends, and whether it is a question.
///
/// Only a blocked task asks anything: it is the one case where a person can
/// still change the outcome. The rest are notices.
pub fn outcome(run: &Run) -> (String, String) {
    let detail = run
        .progress
        .as_ref()
        .map(|p| p.message.clone())
        .unwrap_or_default();
    let confirmed = run.steps.iter().filter(|s| s.status == "done").count();
    let title = match run.status.as_str() {
        "completed" => format!("Tarefa concluída · {}", run.title),
        "blocked" => format!("Tarefa parada e precisa de atenção · {}", run.title),
        "cancelled" => format!("Tarefa parada · {}", run.title),
        "paused" => format!("Tarefa pausada · {}", run.title),
        "expired" => format!("Período encerrado · {}", run.title),
        other => format!("Tarefa {other} · {}", run.title),
    };
    let detail = format!(
        "{}/{} etapas confirmadas · {} ações.{}",
        confirmed,
        run.steps.len(),
        run.action_count,
        if detail.is_empty() {
            String::new()
        } else {
            format!("\n\n{detail}")
        }
    );
    (title, detail)
}

fn finish(
    store: &Store,
    run: &mut Run,
    control: &Mutex<Control>,
    busy: &AtomicBool,
    simplex: &Arc<crate::simplex::Simplex>,
) {
    let mut guard = control.lock().unwrap();
    if guard
        .active
        .as_ref()
        .is_some_and(|(id, stopped)| id == &run.id && *stopped)
    {
        mark_stopped(run);
    }
    let _ = checkpoint(store, run);
    guard.active = None;
    busy.store(false, Ordering::SeqCst);
    // Told after the task is settled and the interface is free, so a slow or
    // absent operator channel cannot hold up the run that just ended.
    let (title, detail) = outcome(run);
    let simplex = simplex.clone();
    tokio::spawn(async move { simplex.notify(&title, &detail).await });
}
fn restart_copy(source: &Run) -> Run {
    let mut run = source.clone();
    run.id = uuid::Uuid::new_v4().to_string();
    run.repetition = None;
    run.status = "ready".into();
    run.action_count = 0;
    run.progress = None;
    run.updated_at = now();
    run.log = vec!["Roteiro reiniciado desde a primeira etapa. A execução anterior foi preservada no histórico.".into()];
    for step in &mut run.steps {
        step.status = "pending".into();
        step.evidence = None;
    }
    run
}
pub async fn restart(
    store: Arc<Store>,
    remote: Arc<Remote>,
    busy: Arc<AtomicBool>,
    control: Arc<Mutex<Control>>,
    simplex: Arc<crate::simplex::Simplex>,
    id: String,
) -> Result<Run, String> {
    launch_mode(store, remote, busy, control, simplex, id, true, None).await
}
pub async fn launch(
    store: Arc<Store>,
    remote: Arc<Remote>,
    busy: Arc<AtomicBool>,
    control: Arc<Mutex<Control>>,
    simplex: Arc<crate::simplex::Simplex>,
    id: String,
) -> Result<(), String> {
    launch_mode(store, remote, busy, control, simplex, id, false, None)
        .await
        .map(|_| ())
}
pub async fn repeat(
    store: Arc<Store>,
    remote: Arc<Remote>,
    busy: Arc<AtomicBool>,
    control: Arc<Mutex<Control>>,
    simplex: Arc<crate::simplex::Simplex>,
    id: String,
    options: crate::repetition::RepeatOptions,
) -> Result<Run, String> {
    let repetition = options.resolve(now())?;
    launch_mode(store, remote, busy, control, simplex, id, true, Some(repetition)).await
}
/// Breaks the type recursion: a resume re-enters the same launch, and an
/// `async fn` that awaits itself cannot be proven to be `Send`.
type Launch = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Run, String>> + Send>>;

fn relaunch(
    store: Arc<Store>,
    remote: Arc<Remote>,
    busy: Arc<AtomicBool>,
    control: Arc<Mutex<Control>>,
    simplex: Arc<crate::simplex::Simplex>,
    id: String,
) -> Launch {
    Box::pin(launch_mode(
        store, remote, busy, control, simplex, id, false, None,
    ))
}

async fn launch_mode(
    store: Arc<Store>,
    remote: Arc<Remote>,
    busy: Arc<AtomicBool>,
    control: Arc<Mutex<Control>>,
    simplex: Arc<crate::simplex::Simplex>,
    id: String,
    restart: bool,
    repetition: Option<crate::repetition::RepeatState>,
) -> Result<Run, String> {
    let mut guard = control.lock().unwrap();
    if busy
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("Já existe uma tarefa em execução.".into());
    }
    let prepared = (|| {
        let run = store.run(&id)?;
        if !restart && ["completed", "cancelled", "expired"].contains(&run.status.as_str()) {
            return Err("Esta tarefa já foi encerrada.".into());
        }
        remote.snapshot()?;
        if remote.info.lock().unwrap().machine_id != run.machine_id {
            return Err("Conecte a máquina desta tarefa.".into());
        }
        if run.steps.is_empty() {
            return Err("O roteiro não contém etapas.".into());
        }
        Ok((
            if restart { restart_copy(&run) } else { run },
            store.settings()?,
        ))
    })();
    let (mut run, s) = match prepared {
        Ok(v) => v,
        Err(e) => {
            busy.store(false, Ordering::SeqCst);
            return Err(e);
        }
    };
    if let Some(repetition) = repetition {
        run.repetition = Some(repetition);
    }
    let epoch = remote.epoch.fetch_add(1, Ordering::SeqCst) + 1;
    run.status = "running".into();
    run.progress = Some(RunProgress {
        message: "Iniciando · preparando a primeira observação".into(),
        started_at: now(),
    });
    run.log
        .push("Execução iniciada. Cada etapa será verificada antes de uma nova ação.".into());
    if let Err(e) = checkpoint(&store, &mut run) {
        busy.store(false, Ordering::SeqCst);
        return Err(e);
    }
    let started_run = run.clone();
    guard.active = Some((run.id.clone(), false));
    drop(guard);
    tokio::spawn(async move {
        if let Err(e) = execute_repeated(&store, &remote, &mut run, &s, epoch).await {
            run.status = if remote.epoch.load(Ordering::SeqCst) != epoch {
                "paused".into()
            } else {
                "blocked".into()
            };
            run.progress = Some(RunProgress {
                message: e.clone(),
                started_at: now(),
            });
            run.log.push(e);
        }
        let _ = remote.release().await;
        let blocked = run.status == "blocked";
        let question = crate::simplex::Question {
            run_id: run.id.clone(),
            title: run.title.clone(),
            detail: run
                .progress
                .as_ref()
                .map(|p| p.message.clone())
                .unwrap_or_default(),
        };
        finish(&store, &mut run, &control, &busy, &simplex);
        // A blocked task is the one case a person can still decide, so it is
        // put to the operator instead of only announced. Asking happens after
        // the task is settled, so a resume starts from a clean state.
        if blocked {
            tokio::spawn(async move {
                if simplex.ask(&question).await == Some(crate::simplex::Answer::Continue) {
                    let _ = relaunch(store, remote, busy, control, simplex, question.run_id)
                        .await;
                }
            });
        }
    });
    Ok(started_run)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_run(id: &str, status: &str) -> Run {
        Run {
            repetition: None,
            id: id.into(),
            title: "Teste".into(),
            machine_id: "machine".into(),
            instructions: "Teste".into(),
            steps: vec![Step {
                text_check: None,
                title: "Etapa".into(),
                success: "Visível".into(),
                status: "done".into(),
                evidence: Some("Confirmada".into()),
            }],
            status: status.into(),
            log: vec!["Histórico anterior".into()],
            action_count: 3,
            updated_at: now(),
            progress: None,
        }
    }
    #[tokio::test]
    async fn ocr_text_pipeline_sends_no_images_and_escalates_uncertainty() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let responses = vec![
            r#"{"status":"not_verified","evidence":"Salvar ainda visível","element_ids":[0]}"#,
            r#"{"kind":"click","target":0}"#,
            r#"{"status":"verified","evidence":"Salvar visível","element_ids":[0]}"#,
            r#"{"status":"need_vision","evidence":"Preciso verificar um ícone","element_ids":[]}"#,
            r#"{"kind":"key","keys":["win","r"]}"#,
            r#"{"status":"not_verified","evidence":"Campo não identificado","element_ids":[]}"#,
            r#"{"kind":"need_vision","reason":"Campo vazio não está no OCR"}"#,
            r#"{"status":"verified","evidence":"ID inventado","element_ids":[55]}"#,
        ];
        let server = tokio::spawn(async move {
            for (index, response) in responses.into_iter().enumerate() {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut data = Vec::new();
                let mut buf = [0; 4096];
                let body = loop {
                    let n = socket.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    data.extend_from_slice(&buf[..n]);
                    if let Some(at) = data.windows(4).position(|b| b == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&data[..at]).to_lowercase();
                        let length: usize = headers
                            .lines()
                            .find_map(|s| s.strip_prefix("content-length: "))
                            .unwrap()
                            .parse()
                            .unwrap();
                        if data.len() >= at + 4 + length {
                            break serde_json::from_slice::<serde_json::Value>(
                                &data[at + 4..at + 4 + length],
                            )
                            .unwrap();
                        }
                    }
                };
                assert_eq!(body["model"], "fixture-text-only");
                assert!(body["messages"][1]["content"].is_string());
                let prompt = body["messages"][1]["content"].as_str().unwrap();
                assert!(prompt.contains("Observação OCR JSON"));
                assert!(!prompt.contains("data:image"));
                assert_eq!(body["max_tokens"], 1024);
                if index == 4 {
                    assert!(prompt.contains("motor ainda não confirmou"));
                    assert!(prompt.contains("Escolha UMA ação"));
                }
                let result =
                    serde_json::json!({"choices":[{"message":{"content":response}}]}).to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{result}",result.len()).as_bytes()).await.unwrap();
            }
        });
        let store = Store::new(std::path::Path::new(":memory:")).unwrap();
        let mut run = test_run("text-fixture", "running");
        let mut settings = Settings::default();
        settings.local_only = true;
        let id = uuid::Uuid::new_v4().to_string();
        settings.profiles.push(Profile {
            id: id.clone(),
            name: "Texto fixture".into(),
            vendor: "local".into(),
            protocol: "chat".into(),
            base_url: format!("http://{address}"),
            model: "fixture-text-only".into(),
            vision: false,
            enabled: true,
            auth_method: "api_key".into(),
        });
        settings.routes.insert("operator".into(), vec![id.clone()]);
        let verifier_id = uuid::Uuid::new_v4().to_string();
        let mut verifier = settings.profiles[0].clone();
        verifier.id = verifier_id.clone();
        settings.profiles.push(verifier);
        settings.routes.insert("verifier".into(), vec![verifier_id]);
        let read = crate::ocr::Reading {
            width: 100,
            height: 80,
            elapsed_ms: 1,
            lines: vec![crate::ocr::Line {
                text: "Salvar".into(),
                confidence: 1.,
                x: 10,
                y: 20,
                width: 40,
                height: 20,
            }],
        };
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            assert!(matches!(
                text_choice(
                    &store,
                    &mut run,
                    &settings,
                    "Roteiro autorizado: abrir Salvar",
                    &read,
                    false
                )
                .await
                .unwrap(),
                TextChoice::Action(Action::Click { x: 30, y: 30 }, _)
            ));
            assert!(matches!(
                text_choice(
                    &store,
                    &mut run,
                    &settings,
                    "Roteiro autorizado: conferir texto Salvar",
                    &read,
                    false
                )
                .await
                .unwrap(),
                TextChoice::Done(_, _)
            ));
            assert!(matches!(
                text_choice(&store, &mut run, &settings, "Conferir ícone", &read, false)
                    .await
                    .unwrap(),
                TextChoice::Vision(_)
            ));
            assert!(matches!(
                text_choice(
                    &store,
                    &mut run,
                    &settings,
                    "Regra explícita pendente",
                    &read,
                    true
                )
                .await
                .unwrap(),
                TextChoice::Action(Action::Key { .. }, _)
            ));
            assert!(matches!(
                text_choice(&store, &mut run, &settings, "Preencher campo", &read, false)
                    .await
                    .unwrap(),
                TextChoice::Vision(_)
            ));
            assert!(matches!(
                text_choice(
                    &store,
                    &mut run,
                    &settings,
                    "Conferir resultado",
                    &read,
                    false
                )
                .await
                .unwrap(),
                TextChoice::Vision(_)
            ));
            server.await.unwrap();
        })
        .await
        .unwrap();
        assert_eq!(run.action_count, 3); // choosing an action alone never executes it
    }
    #[tokio::test]
    async fn invented_textual_success_cannot_complete_a_step_when_visual_review_disagrees() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for (index,response) in [
                r#"{"status":"verified","evidence":"afirmação inventada de sucesso","element_ids":[0]}"#,
                r#"{"verified":false,"evidence":"A tela mostra apenas o desktop, não o GitHub."}"#,
                r#"{"kind":"blocked","reason":"Falta senha."}"#,
            ].into_iter().enumerate() {
                let (mut socket,_)=listener.accept().await.unwrap();let mut bytes=Vec::new();let mut buf=[0;4096];
                let body=loop {
                    let n=socket.read(&mut buf).await.unwrap();assert!(n>0);bytes.extend_from_slice(&buf[..n]);
                    if let Some(at)=bytes.windows(4).position(|b|b==b"\r\n\r\n") {
                        let h=String::from_utf8_lossy(&bytes[..at]).to_lowercase();
                        let len:usize=h.lines().find_map(|l|l.strip_prefix("content-length: ")).unwrap().parse().unwrap();
                        if bytes.len()>=at+4+len {break serde_json::from_slice::<serde_json::Value>(&bytes[at+4..at+4+len]).unwrap();}
                    }
                };
                assert_eq!(body["model"],if index==1 {"secondary"} else {"primary"});
                assert_eq!(body.to_string().contains("data:image"),index>0);
                assert!(!body.to_string().contains("afirmação inventada de sucesso"));
                let result=serde_json::json!({"choices":[{"message":{"content":response}}]}).to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{result}",result.len()).as_bytes()).await.unwrap();
            }
        });
        let store = Store::new(std::path::Path::new(":memory:")).unwrap();
        let mut run = test_run("false-success", "running");
        run.steps[0].status = "pending".into();
        run.steps[0].evidence = None;
        run.steps[0].success = "Perfil GitHub visível".into();
        run.action_count = 0;
        let mut settings = Settings::default();
        settings.local_only = true;
        for name in ["primary", "secondary"] {
            let id = uuid::Uuid::new_v4().to_string();
            settings.profiles.push(Profile {
                id: id.clone(),
                name: name.into(),
                vendor: "local".into(),
                protocol: "chat".into(),
                base_url: format!("http://{address}"),
                model: name.into(),
                vision: true,
                enabled: true,
                auth_method: "api_key".into(),
            });
            settings
                .routes
                .entry("vision".into())
                .or_default()
                .push(id.clone());
            if name == "primary" {
                for role in ["operator", "verifier"] {
                    settings.routes.insert(role.into(), vec![id.clone()]);
                }
            }
        }
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(100, 80)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let frame = Snapshot {
            width: 100,
            height: 80,
            data_url: format!(
                "data:image/png;base64,{}",
                STANDARD.encode(png.into_inner())
            ),
            sequence: 1,
            captured_at: now(),
        };
        let read = crate::ocr::Reading {
            width: 100,
            height: 80,
            elapsed_ms: 1,
            lines: vec![crate::ocr::Line {
                text: "Lixeira".into(),
                confidence: 1.,
                x: 10,
                y: 20,
                width: 40,
                height: 20,
            }],
        };
        let mut observations = crate::observation::Cache::default();
        observations.remember(&frame, crate::vision::Region::full(&frame), read);
        let remote = Remote::observed_fixture(frame, "machine");
        let error = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            execute_with_observations(
                &store,
                &remote,
                &mut run,
                &settings,
                remote.epoch.load(Ordering::SeqCst),
                observations,
            ),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert!(error.contains("Falta senha"));
        assert_ne!(run.steps[0].status, "done");
        assert!(run.steps[0].evidence.is_none());
        assert!(!run.log.iter().any(|l| l.contains("Etapa 1 verificada")));
        server.await.unwrap();
    }
    #[tokio::test]
    async fn weekly_boundary_keeps_executor_epoch_and_stop_cancels_the_next_window() {
        use chrono::Datelike;
        let store = Store::new(std::path::Path::new(":memory:")).unwrap();
        let remote = Remote::new();
        let epoch = remote.epoch.load(Ordering::SeqCst);
        assert!(until_deadline(
            &remote,
            now() + 10,
            false,
            std::future::pending::<Result<(), String>>()
        )
        .await
        .is_none());
        assert_eq!(remote.epoch.load(Ordering::SeqCst), epoch);
        let mut run = test_run("weekly", "waiting");
        let tomorrow = chrono::Local::now().weekday().number_from_monday() % 7 + 1;
        run.repetition = Some(
            crate::repetition::RepeatOptions {
                schedule: crate::repetition::Schedule::Weekly {
                    weekdays: vec![tomorrow],
                    start_minute: 540,
                    end_minute: 1080,
                    start_date: None,
                    end_date: None,
                },
                interval_seconds: 5,
            }
            .resolve(now())
            .unwrap(),
        );
        run.repetition.as_mut().unwrap().ends_at = now() - 1;
        let control = Mutex::new(Control {
            active: Some((run.id.clone(), false)),
        });
        let busy = AtomicBool::new(true);
        let settings = Settings::default();
        let stopping = async {
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            stop(&store, &remote, &control, "weekly").await.unwrap();
        };
        let (result, _) = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            tokio::join!(
                execute_repeated(&store, &remote, &mut run, &settings, epoch),
                stopping
            )
        })
        .await
        .unwrap();
        assert!(result.is_err());
        assert!(run.repetition.as_ref().unwrap().starts_at > now());
        assert_eq!(run.repetition.as_ref().unwrap().cycle, 0);
        finish(&store, &mut run, &control, &busy, &Arc::new(crate::simplex::Simplex::new()));
        assert_eq!(store.run("weekly").unwrap().status, "cancelled");
        assert!(!busy.load(Ordering::SeqCst));
    }
    #[tokio::test]
    async fn weekly_final_date_expires_without_sending_actions_or_waiting_another_week() {
        let store = Store::new(std::path::Path::new(":memory:")).unwrap();
        let remote = Remote::new();
        let mut run = test_run("dated", "waiting");
        let options: crate::repetition::RepeatOptions = serde_json::from_str(r#"{"schedule":{"mode":"weekly","weekdays":[1,2,3,4,5,6,7],"startMinute":540,"endMinute":1080},"intervalSeconds":5}"#).unwrap();
        let mut state = options.resolve(now()).unwrap();
        state.weekly.as_mut().unwrap().end_date =
            Some((chrono::Local::now().date_naive() - chrono::Duration::days(1)).to_string());
        state.ends_at = now() - 1;
        run.repetition = Some(state);
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            execute_repeated(
                &store,
                &remote,
                &mut run,
                &Settings::default(),
                remote.epoch.load(Ordering::SeqCst),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(run.status, "expired");
        assert_eq!(run.repetition.as_ref().unwrap().cycle, 0);
        assert_eq!(run.repetition.as_ref().unwrap().total_actions, 0);
    }
    #[tokio::test]
    async fn repetition_deadline_cancels_pending_work_and_invalidates_input() {
        let remote = Remote::new();
        let epoch = remote.epoch.load(Ordering::SeqCst);
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            until_deadline(
                &remote,
                now() + 30,
                true,
                std::future::pending::<Result<(), String>>(),
            ),
        )
        .await
        .unwrap();
        assert!(result.is_none());
        assert_ne!(remote.epoch.load(Ordering::SeqCst), epoch);
    }
    #[tokio::test]
    async fn waiting_repetition_can_pause_and_expired_period_never_executes() {
        use crate::repetition::{RepeatOptions, Schedule};
        let store = Store::new(std::path::Path::new(":memory:")).unwrap();
        let remote = Remote::new();
        let mut run = test_run("loop", "ready");
        run.repetition = Some(
            RepeatOptions {
                schedule: Schedule::Window {
                    starts_at: now() + 60000,
                    ends_at: now() + 120000,
                },
                interval_seconds: 1,
            }
            .resolve(now())
            .unwrap(),
        );
        let epoch = remote.epoch.load(Ordering::SeqCst);
        let settings = Settings::default();
        let pause = async {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            remote.epoch.fetch_add(1, Ordering::SeqCst);
        };
        let (result, _) = tokio::join!(
            execute_repeated(&store, &remote, &mut run, &settings, epoch),
            pause
        );
        assert!(result.unwrap_err().contains("pausada"));
        assert_eq!(run.repetition.as_ref().unwrap().cycle, 0);
        run.repetition.as_mut().unwrap().ends_at = now() - 1;
        let epoch = remote.epoch.load(Ordering::SeqCst);
        execute_repeated(&store, &remote, &mut run, &settings, epoch)
            .await
            .unwrap();
        assert_eq!(run.status, "expired");
        assert_eq!(run.repetition.as_ref().unwrap().cycle, 0);
    }
    #[test]
    fn restarting_preserves_original_and_resets_every_step_and_counter() {
        let store = Store::new(std::path::Path::new(":memory:")).unwrap();
        for status in ["paused", "blocked", "cancelled", "completed"] {
            let mut original = test_run(status, status);
            original.steps[0].text_check = Some(crate::ocr::TextCheck {
                expected: "0".into(),
                region: crate::vision::Region {
                    x: 10,
                    y: 10,
                    width: 100,
                    height: 50,
                },
                screen_width: 800,
                screen_height: 600,
            });
            store.put_run(&original).unwrap();
            let restarted = restart_copy(&original);
            assert_eq!(
                serde_json::to_value(&restarted.steps[0].text_check).unwrap(),
                serde_json::to_value(&original.steps[0].text_check).unwrap()
            );
            store.put_run(&restarted).unwrap();
            assert_ne!(restarted.id, original.id);
            assert_eq!(restarted.instructions, original.instructions);
            assert_eq!(restarted.machine_id, original.machine_id);
            assert_eq!(restarted.steps[0].title, original.steps[0].title);
            assert_eq!(restarted.steps[0].success, original.steps[0].success);
            assert!(restarted
                .steps
                .iter()
                .all(|s| s.status == "pending" && s.evidence.is_none()));
            assert_eq!(restarted.action_count, 0);
            assert!(restarted.progress.is_none());
            assert_eq!(
                serde_json::to_value(store.run(&original.id).unwrap()).unwrap(),
                serde_json::to_value(&original).unwrap()
            );
        }
    }
    #[tokio::test]
    async fn restart_without_connection_or_while_busy_creates_no_execution() {
        let store = Arc::new(Store::new(std::path::Path::new(":memory:")).unwrap());
        store.put_run(&test_run("original", "cancelled")).unwrap();
        let remote = Arc::new(Remote::new());
        let busy = Arc::new(AtomicBool::new(true));
        let control = Arc::new(Mutex::new(Control::default()));
        assert!(restart(
            store.clone(),
            remote.clone(),
            busy.clone(),
            control.clone(),
            Arc::new(crate::simplex::Simplex::new()),
            "original".into()
        )
        .await
        .unwrap_err()
        .contains("execução"));
        assert!(busy.load(Ordering::SeqCst));
        busy.store(false, Ordering::SeqCst);
        assert!(restart(
            store.clone(),
            remote,
            busy.clone(),
            control,
            Arc::new(crate::simplex::Simplex::new()),
            "original".into()
        )
        .await
        .is_err());
        assert!(!busy.load(Ordering::SeqCst));
        assert_eq!(store.runs().unwrap().len(), 1);
        assert_eq!(store.run("original").unwrap().status, "cancelled");
    }
    #[tokio::test]
    async fn stop_interrupts_pending_response_and_wins_over_late_completion() {
        let store = Store::new(std::path::Path::new(":memory:")).unwrap();
        let remote = Remote::new();
        let mut run = test_run("active", "running");
        store.put_run(&run).unwrap();
        let control = Mutex::new(Control {
            active: Some((run.id.clone(), false)),
        });
        let busy = AtomicBool::new(true);
        let epoch = remote.epoch.load(Ordering::SeqCst);
        let waiting = checked(&remote, epoch, std::future::pending::<Result<(), String>>());
        let stopping = async {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            stop(&store, &remote, &control, &run.id).await.unwrap();
        };
        let (result, _) = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            tokio::join!(waiting, stopping)
        })
        .await
        .unwrap();
        assert!(result.is_err());
        run.status = "completed".into(); // A last verification may have finished concurrently.
        finish(&store, &mut run, &control, &busy, &Arc::new(crate::simplex::Simplex::new()));
        let saved = store.run(&run.id).unwrap();
        assert_eq!(saved.status, "cancelled");
        assert_eq!(saved.action_count, 3);
        assert_eq!(saved.steps[0].evidence.as_deref(), Some("Confirmada"));
        assert_eq!(saved.log[0], "Histórico anterior");
        assert!(!busy.load(Ordering::SeqCst));
        assert!(launch(
            Arc::new(store),
            Arc::new(remote),
            Arc::new(busy),
            Arc::new(control),
            Arc::new(crate::simplex::Simplex::new()),
            run.id
        )
        .await
        .unwrap_err()
        .contains("encerrada"));
    }
    #[tokio::test]
    async fn stop_paused_task_is_idempotent_and_does_not_interrupt_another_task() {
        let store = Store::new(std::path::Path::new(":memory:")).unwrap();
        let remote = Remote::new();
        let control = Mutex::new(Control {
            active: Some(("other".into(), false)),
        });
        store.put_run(&test_run("paused", "paused")).unwrap();
        let epoch = remote.epoch.load(Ordering::SeqCst);
        stop(&store, &remote, &control, "paused").await.unwrap();
        stop(&store, &remote, &control, "paused").await.unwrap();
        let saved = store.run("paused").unwrap();
        assert_eq!(saved.status, "cancelled");
        assert_eq!(saved.log.len(), 2);
        assert_eq!(remote.epoch.load(Ordering::SeqCst), epoch);
        assert_eq!(
            control.lock().unwrap().active,
            Some(("other".into(), false))
        );
    }
    #[tokio::test]
    async fn stop_does_not_relabel_completed_tasks() {
        let store = Store::new(std::path::Path::new(":memory:")).unwrap();
        store.put_run(&test_run("done", "completed")).unwrap();
        stop(
            &store,
            &Remote::new(),
            &Mutex::new(Control::default()),
            "done",
        )
        .await
        .unwrap();
        assert_eq!(store.run("done").unwrap().status, "completed");
    }
    #[test]
    fn the_operator_is_told_what_happened_and_how_far_it_got() {
        let mut run = test_run("r1", "completed");
        run.title = "Abrir Calculadora".into();
        run.action_count = 5;
        let step = |status: &str| Step {
            text_check: None,
            title: "etapa".into(),
            success: "critério".into(),
            status: status.into(),
            evidence: None,
        };
        run.steps = vec![step("done"), step("pending")];
        let (title, detail) = outcome(&run);
        assert!(title.contains("concluída") && title.contains("Abrir Calculadora"));
        assert!(detail.contains("1/2 etapas confirmadas"));
        assert!(detail.contains("5 ações"));
        // A blocked task says why, since that is what a decision rests on.
        run.status = "blocked".into();
        run.progress = Some(RunProgress { message: "A etapa 2 não encontrou a caixa.".into(), started_at: 0 });
        let (title, detail) = outcome(&run);
        assert!(title.contains("precisa de atenção"));
        assert!(detail.contains("não encontrou a caixa"));
    }

    #[tokio::test]
    async fn pause_cancels_pending_model_request() {
        let r = Remote::new();
        let epoch = r.epoch.load(Ordering::SeqCst);
        let result = async {
            checked(&r, epoch, async {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                Ok(())
            })
            .await
        };
        let cancel = async {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            r.epoch.fetch_add(1, Ordering::SeqCst);
        };
        let (result, _) = tokio::join!(result, cancel);
        assert!(result.is_err());
    }
    #[test]
    fn resized_screen_is_rejected() {
        let a = Snapshot {
            data_url: "same".into(),
            width: 1280,
            height: 800,
            sequence: 1,
            captured_at: now(),
        };
        let mut b = a.clone();
        b.width = 1920;
        assert!(!frame_compatible(&a, &b));
        assert!(frame_compatible(&a, &a));
    }
    #[test]
    fn invalid_operator_output_identifies_the_profile_and_cannot_become_an_action() {
        let error =
            operator_response::<Action>("The screen shows a desktop.", "SmolVLM", "operação")
                .unwrap_err();
        assert!(error.contains("SmolVLM"));
        assert!(error.contains("Roteamento de IA"));
        assert!(operator_response::<Action>(
            r#"{"kind":"key","keys":["win","r"]}"#,
            "OpenAI",
            "operação"
        )
        .is_ok());
    }
}

#[cfg(test)]
mod input_progress_tests {
    use super::*;
    fn frame(value: &str) -> Snapshot {
        let img = image::RgbaImage::from_pixel(1, 1, image::Rgba([value.as_bytes()[0], 0, 0, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        Snapshot {
            data_url: format!(
                "data:image/png;base64,{}",
                STANDARD.encode(bytes.into_inner())
            ),
            width: 1,
            height: 1,
            sequence: 1,
            captured_at: 0,
        }
    }
    #[test]
    fn delayed_repaint_clears_stagnation_without_recounting_observations() {
        let mut p = InputProgress::default();
        let a = frame("a");
        let b = frame("b");
        p.sent(a.clone());
        for _ in 0..10 {
            assert!(p.observe(&a));
        }
        assert_eq!(p.stagnant, 1);
        assert!(!p.observe(&b));
        assert_eq!(p.stagnant, 0);
        p.sent(b.clone());
        p.observe(&b);
        assert_eq!(p.stagnant, 1);
    }
    #[test]
    fn three_distinct_ineffective_inputs_still_stop_and_late_change_before_send_resets() {
        let mut p = InputProgress::default();
        let a = frame("a");
        for _ in 0..3 {
            p.sent(a.clone());
            p.observe(&a);
        }
        assert_eq!(p.stagnant, 3);
        p.sent(frame("changed"));
        assert_eq!(p.stagnant, 0);
    }
}

/// Uses synthetic OCR only. No remote connection or input execution occurs.
pub async fn test_operator_profile(settings: &Settings, id: &str) -> Result<String, String> {
    let mut s = settings.clone();
    if !s.profiles.iter().any(|p| p.id == id && p.enabled) {
        return Err("Perfil não encontrado ou desativado.".into());
    }
    s.routes.insert("operator".into(), vec![id.into()]);
    let read = crate::ocr::Reading {
        width: 800,
        height: 600,
        elapsed_ms: 0,
        lines: vec![crate::ocr::Line {
            text: "Confirmar".into(),
            confidence: 1.0,
            x: 100,
            y: 200,
            width: 100,
            height: 40,
        }],
    };
    let prompt=format!("Teste simulado do contrato, sem execução. Objetivo autorizado: clicar uma vez no botão Confirmar. Não marque a tarefa concluída. {}\n{}",crate::observation::context(&read),crate::harness::actions(false,false));
    let start = std::time::Instant::now();
    let (_, provider) = llm::routed_validated(
        &s,
        "operator",
        &crate::harness::system("text-action"),
        &prompt,
        None,
        |text, p| match validated_text_choice(text, p, &read)? {
            TextChoice::Action(Action::Click { x: 150, y: 220 }, _) => Ok(()),
            _ => Err(
                "Neste teste, o botão Confirmar está no OCR com ID 0; retorne click com target 0."
                    .into(),
            ),
        },
    )
    .await?;
    Ok(format!("{provider}: contrato de operador validado em {:.1}s. Teste simulado; nenhuma entrada enviada ao Windows.",start.elapsed().as_secs_f64()))
}
