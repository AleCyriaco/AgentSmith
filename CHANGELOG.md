# Changelog

## Licensing update

- Publish AgentSmith source and documentation under the MIT License, including version 0.12.7. Third-party components keep their own licenses.

## 0.12.7 — Public documentation release

- Reorganize the project documentation in English: goals, benefits, setup, routing, architecture, privacy, troubleshooting, contribution, and distribution notes.
- Add English screenshots from the real UI with fictional demonstration data.
- Review reachable Git history and publication files for secrets and sensitive information.
- Correct the translated preview-version label and update release metadata.
- Document current limits, including intermediate-step blocking when the overall goal is already visible.

## 0.12.6

- Configure official DeepSeek V4 reasoning explicitly for planning versus short actions.
- Request structured JSON for harness calls and reject known text-only models on image input.
- Add profile/model/output-budget context to truncation errors; partial output remains non-executable.

## 0.12.5

- Preserve and classify Claude client failure diagnostics without exposing raw output.
- Retain final process errors when stdin closes early.

## 0.12.4

- Fix Claude Code input/output streaming compatibility.
- Consume only validated final result events.

## 0.12.3

- Allow capture intervals down to 20 ms and additional post-action delay down to zero.
- Add a 50 ms / zero-delay / 1,280 px Turbo preset.

## 0.12.2

- Use native structured-output schemas and final metadata for Grok Build browser-login requests.

## 0.12.0–0.12.1

- Introduce compact provider-independent action contracts, operator conformance testing, bounded repair, and configured fallback.
- Improve xAI JSON output handling.

## 0.11.x and earlier

- OCR-first operation with visual assistance and visual confirmation of proposed completion.
- Native FreeRDP session, detachable/focus view, display and pacing controls.
- Plan editing/deletion, pause/resume/stop/restart, repetition schedules, and persistent history.
- Multi-provider API profiles, official-client browser login, local-only routing, built-in local vision, and native Apple Vision OCR.
- English, Brazilian Portuguese, and Spanish UI with collapsible navigation and lavender branding.
