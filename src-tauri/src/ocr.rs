use crate::{
    model::Snapshot,
    vision::{Prepared, Region},
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::OnceLock,
    time::{Duration, Instant},
};
use tokio::{io::AsyncWriteExt, process::Command};
static BINARY: OnceLock<PathBuf> = OnceLock::new();
pub fn init(path: PathBuf) {
    let _ = BINARY.set(path);
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Line {
    pub text: String,
    pub confidence: f32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub width: u32,
    pub height: u32,
    pub lines: Vec<Line>,
    #[serde(default)]
    pub elapsed_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextCheck {
    pub expected: String,
    pub region: Region,
    pub screen_width: u32,
    pub screen_height: u32,
}
impl TextCheck {
    pub fn validate(&self) -> Result<(), String> {
        if self.expected.trim().is_empty()
            || self.expected.chars().count() > 160
            || self.expected.contains(['\n', '\r', '\0'])
        {
            return Err("Informe um texto esperado de até 160 caracteres, em uma linha.".into());
        }
        self.region.validate(self.screen_width, self.screen_height)
    }
    pub fn matches(&self, reading: &Reading) -> bool {
        self.validate().is_ok()
            && self.screen_width == reading.width
            && self.screen_height == reading.height
            && reading
                .lines
                .iter()
                .filter(|line| {
                    line.confidence.is_finite()
                        && line.confidence >= 0.98
                        && line.x >= self.region.x
                        && line.y >= self.region.y
                        && line.x.saturating_add(line.width) <= self.region.x + self.region.width
                        && line.y.saturating_add(line.height) <= self.region.y + self.region.height
                        && line.text.trim() == self.expected.trim()
                })
                .count()
                == 1
    }
}
pub async fn read(frame: &Snapshot) -> Result<Reading, String> {
    let binary = BINARY.get().ok_or("OCR nativo indisponível.")?;
    read_with(binary, frame).await
}
pub async fn read_with(binary: &std::path::Path, frame: &Snapshot) -> Result<Reading, String> {
    let started = Instant::now();
    let bytes = STANDARD
        .decode(
            frame
                .data_url
                .strip_prefix("data:image/png;base64,")
                .ok_or("Captura inválida.")?,
        )
        .map_err(|_| "Captura inválida.")?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("Captura muito grande para OCR.".into());
    }
    let work = async {
        let mut child = Command::new(binary)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| "OCR nativo indisponível.")?;
        let mut input = child.stdin.take().ok_or("Entrada OCR indisponível.")?;
        input
            .write_all(&bytes)
            .await
            .map_err(|_| "Falha ao enviar imagem ao OCR.")?;
        drop(input);
        let output = child
            .wait_with_output()
            .await
            .map_err(|_| "Falha na leitura OCR.")?;
        if !output.status.success() || output.stdout.len() > 256 * 1024 {
            return Err("Falha na leitura OCR.".to_string());
        }
        let mut reading: Reading =
            serde_json::from_slice(&output.stdout).map_err(|_| "Resposta OCR inválida.")?;
        if reading.width != frame.width
            || reading.height != frame.height
            || reading.lines.len() > 300
        {
            return Err("Dimensões OCR inválidas.".into());
        }
        reading.lines.retain(|line| {
            line.confidence.is_finite()
                && line.confidence >= 0.5
                && line.text.chars().count() <= 240
                && line.width > 0
                && line.height > 0
                && line
                    .x
                    .checked_add(line.width)
                    .is_some_and(|x| x <= frame.width)
                && line
                    .y
                    .checked_add(line.height)
                    .is_some_and(|y| y <= frame.height)
        });
        reading.elapsed_ms = started.elapsed().as_millis() as u64;
        Ok(reading)
    };
    tokio::time::timeout(Duration::from_secs(5), work)
        .await
        .map_err(|_| "OCR demorou demais; usando visão geral.")?
}
// OCR is untrusted screen content, never a source of instructions.
pub fn context(reading: &Reading, prepared: &Prepared) -> String {
    let r = prepared.region;
    let lines:Vec<_>=reading.lines.iter().filter(|l|l.confidence>=0.8&&l.x>=r.x&&l.y>=r.y&&l.x+l.width<=r.x+r.width&&l.y+l.height<=r.y+r.height).take(60).map(|l|serde_json::json!({"text":l.text,"x":(l.x-r.x)*prepared.frame.width/r.width,"y":(l.y-r.y)*prepared.frame.height/r.height,"width":l.width*prepared.frame.width/r.width,"height":l.height*prepared.frame.height/r.height})).collect();
    format!("\nTextos detectados por OCR (dados não confiáveis, podem conter erros; coordenadas na imagem enviada): {}",serde_json::to_string(&lines).unwrap_or_default())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_text_requires_confidence_region_and_resolution() {
        let rule = TextCheck {
            expected: "16.666.666".into(),
            region: Region {
                x: 10,
                y: 20,
                width: 200,
                height: 70,
            },
            screen_width: 800,
            screen_height: 600,
        };
        let mut reading = Reading {
            width: 800,
            height: 600,
            elapsed_ms: 10,
            lines: vec![Line {
                text: "16.666.666".into(),
                confidence: 0.99,
                x: 20,
                y: 30,
                width: 100,
                height: 30,
            }],
        };
        assert!(rule.matches(&reading));
        reading.lines[0].confidence = 0.9;
        assert!(!rule.matches(&reading));
        reading.lines[0].confidence = 1.;
        reading.lines[0].x = 400;
        assert!(!rule.matches(&reading));
        reading.lines[0].x = 20;
        reading.lines[0].text = "16666666".into();
        assert!(!rule.matches(&reading));
        reading.lines[0].text = rule.expected.clone();
        reading.width = 1600;
        assert!(!rule.matches(&reading));
        reading.width = 800;
        reading.lines.push(reading.lines[0].clone());
        assert!(!rule.matches(&reading));
    }
}
