# Changelog

## 0.14.0 — RustDesk as a session transport

- Add **Alerts and decisions**: alerts on the operator's own server, over ntfy or SimpleX, reporting how every task ended and asking what to do when one blocks, answered from a phone. A question carries a one-time ticket its answer must repeat, and answers arrive on a topic separate from the alerts.

- Speak the RustDesk protocol directly, so a RustDesk destination carries plans like an RDP one: authenticated, encrypted, decoded to the same frames, and driven by the same actions.
- Verify both signature layers before opening a session, and refuse a session that cannot be authenticated instead of falling back to plaintext as RustDesk does.
- Decode VP8 and VP9 with libvpx, linked statically so the bundle gains no dylib; announce only those codecs so a peer cannot answer with an undecodable stream.
- Reach a machine by hole punch and fall back to a relay; accept an optional self-hosted rendezvous server and its key per machine.
- Store the RustDesk password in the Keychain under its ID, separate from any RDP password for the same host, and answer the peer's challenge rather than sending it.
- Assert that both transports accept and refuse exactly the same actions.
- Implement the wire format independently, with no RustDesk source copied or linked; AgentSmith stays MIT.
- Answer a second-factor challenge: the interface asks for the current code, and offers to have the machine trust this Mac so later runs need none. Trusting is unticked by default and says plainly what it gives up, since it is a lasting reduction of that machine's protection.
- Tell an undecodable frame from an invisible one. VP9 sends reference frames that decode to nothing all the time, and each was being answered with a request for a key frame, which restarts the machine's encoder; the picture kept dropping to a key frame and recovering. Ask for one only when a frame truly fails, and at most once every two seconds.
- Take the version shown in the sidebar from `package.json` at build time; it was a fixed string and had fallen behind. Show the server a RustDesk machine will actually use on its card, the default one included.
- Add screenshots of the Machines page, the RustDesk machine form and the two-factor dialog, and bring every version reference in the documentation to 0.14.0.
- Record why an official client failed in `~/Library/Logs/AgentSmith/client-diagnostics.log` — exit code, final result fields and the tail of its error stream, never the prompt or an image — since the interface shows only a classified message and the reason was otherwise lost.
- Accept a modifier on its own as a key press over RustDesk — "press the Windows key" opens many plans — pressed and released the way the RDP transport does it. The conformance test now covers that case.
- Echo the machine's latency probe at every stage, including while the login is still pending, where its first one lands. A dropped probe is never followed by another, and the picture was expiring for silence a few seconds into every session.
- Try the machine's own address for three seconds, not twelve, before falling back to a relay.
- Echo the machine's latency probe untouched, and present the device identity on every login. The first keeps the session alive — the machine sends one probe at a time and closes a silent connection after thirty seconds — and the second is what lets a machine that was asked to trust this Mac actually skip the second factor.
- Convert and encode one frame per capture interval instead of every frame the machine sends, and optimise the development profile, since a two-million-pixel frame is unusably slow to convert in an unoptimised build.
- Attach to a Windows session on a machine running more than one, since that choice cannot be put to a person during unattended work.
- Verified end to end against a Windows 11 machine running RustDesk 1.4.9 through a self-hosted server: relay path, both signature layers, the cipher, the second factor, VP9, key plus delta frames decoded to a correct image, and pointer moves after which the picture changed. The direct path, a multi-session machine, the public server, and clicks and typing are not yet exercised.

## 0.13.0 — RustDesk manual web client

- Open the official RustDesk client in its own native window from Machines, without creating a machine first.
- Save a RustDesk ID and optional custom HTTPS web-client URL; enter authentication directly in the client.
- Restrict external navigation and keep the client outside AgentSmith IPC, Keychain, and AI execution.
- Explicitly distinguish manual RustDesk access from the native RDP session and reject unsupported AI planning/execution for RustDesk.
- Add URL, origin, migration, and capability regression tests; translate new controls into English and Spanish.

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
