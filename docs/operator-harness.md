# Operator harness — 0.12.7

The same action meaning applies across API adapters, official clients, and local models. Protocol compatibility does not establish visual accuracy. Test each profile with the synthetic operator check and a small Windows task.

## Compact observe–act–verify loop

1. Read a current frame; reuse OCR only when the relevant pixels and resolution match.
2. Send the authorized plan, current step and criterion, prior steps, and up to four recent inputs with screen-change information.
3. Combine operation and verification into one text decision when their routes match.
4. Validate JSON, fields, target IDs, coordinates, text length, and shortcuts. Allow one format repair per profile; use only configured alternatives afterward.
5. Recheck observation freshness before transmitting one input through the single RDP executor.
6. Observe again, allowing delayed screen response. Three unchanged inputs stop for review.

Text-proposed success still requires visual confirmation; explicit OCR criteria are checked directly. Real blocking restrictions are preserved.

## Action semantics

| Proposal | Meaning |
| --- | --- |
| `{"kind":"key","keys":["ctrl","l"]}` | One simultaneous shortcut; assumes appropriate application focus |
| `{"kind":"type_text","text":"hello"}` | Type up to 400 characters; does not press Enter |
| `{"kind":"click","target":0}` | Click a current OCR target with sufficient confidence |
| `{"kind":"click","x":120,"y":80}` | Visual-image coordinates; the engine maps them to the remote session |
| `need_vision` | Request image assistance; no Windows input |
| `inspect` | Request a bounded crop if enabled; no Windows input |
| `wait` | Wait for 1–10 seconds |
| `blocked` | A concrete impediment respecting task restrictions |

Additional validated remote actions include double click, right click, and scrolling. Separate sequential shortcuts into separate proposals. A syntactically valid object is still subject to semantic checks.

## Context and cost

OCR context is capped at 120 elements and a 12 KB serialized-line budget. Long recognized strings are truncated to 200 characters and omissions are marked. Authorized task instructions remain intact so constraints are not lost. Recent-input context does not copy typed content.

Combined text decisions can remove one model call for an incomplete step. Visual confirmation, repairs, and alternatives can add calls. No percentage cost saving has been established; account billing and hidden reasoning depend on the provider/model.

## Provider-specific handling

**xAI API:** harness requests through its Chat Completions path request JSON mode, with local validation retained.

**Grok Build browser login:** `session/prompt` includes `_meta.outputSchema`; final `_meta.structuredOutput` is authoritative when a contract was requested. Intermediate prose, missing metadata, cancellation, or malformed output cannot become an input. Plan, OCR action, combined decision, OCR verification, visual action, and visual verification have separate closed schemas.

**Claude Code browser login:** input and output use `stream-json`, with `--verbose`. Only the final `result` event is consumed; duplicate, incomplete, malformed, or failed responses are rejected. Nonzero exits are classified from bounded output without copying raw provider messages into the user history.

**Official DeepSeek V4 endpoint:** planning explicitly uses low reasoning with an 8,192-token output budget; short operation/verification requests disable thinking and retain a compact 1,024-token budget. Harness calls request JSON mode. Known text-only models reject image input locally, even if their profile has vision selected. Third-party compatible endpoints are not given DeepSeek-specific parameters automatically. Truncated output is rejected in full.

## Validation scope

Automated checks cover invented IDs, invalid JSON, format repair, configured fallback, out-of-screen clicks, genuine impediments, false text completion rejected by vision, image-free text requests, delayed frame updates, cancellation, and key release.

Selected live synthetic checks observed a DeepSeek Flash OCR action in 1.3 s and a Claude connection response in 2.9 s. These were different tests, not a comparative benchmark. DeepSeek's experimental vision variant returned complete JSON but missed the synthetic target in both attempts; it was not certified for visual control. No claim is made that all catalog providers or models pass real Windows tasks.

Implementation details live in `harness.rs`, `llm.rs`, `browser_auth.rs`, `observation.rs`, and `executor.rs`.
