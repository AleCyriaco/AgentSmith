# Privacy and security

## Data flow

| Data | Storage or destination |
| --- | --- |
| Saved Windows password / API key | macOS Keychain, bound to the associated endpoint/profile |
| Machine metadata and model profiles | Local SQLite database |
| Goal, plan, progress, evidence, logs | Local SQLite database; task text may contain sensitive content |
| OCR | Extracted locally from the RDP frame; sent as text to configured text models |
| Visual requests | Remote screenshot or crop sent to the selected visual model/client |
| Built-in local inference | Loopback llama.cpp server on this Mac |
| Model weights | Local application data until removed by the user |

The application data directory is `~/Library/Application Support/com.agentsmith.desktop`. The SQLite database is not additionally encrypted by AgentSmith. Protect Mac access and backups accordingly. Do not put passwords into natural-language plans when a dedicated credential field is available.

## Screen retention

RDP frames and OCR caches are transient in memory; AgentSmith does not continuously record video. The built-in OCR/local-inference path does not write screenshots to files. The Codex client integration temporarily writes an image into a private temporary directory and removes it on normal request completion. Abrupt crashes can prevent normal cleanup.

Other official clients can maintain their own logs and sessions, and cloud providers apply their own retention policies. AgentSmith does not control those systems. Task instructions and textual evidence remain in local history until removed. Avoid assuming that every piece of task data disappears after inference.

## Local-only mode

Local-only routing rejects cloud profiles and non-loopback inference endpoints and avoids environment proxies/redirects for the local inference client. Configure all roles locally. This setting applies to AI inference: remote RDP still uses the network, and installing components or downloading models requires downloads.

## Input and trust boundaries

Only authorized plans should control Windows. Screens, pages, and documents are untrusted evidence, not new instructions. Structured action validation, bounded coordinates, observation freshness, cancellation, and action limits reduce risk. They do not make a model immune to prompt injection or guarantee correct decisions.

TLS validation remains enabled. Confirm unfamiliar certificate fingerprints through a trusted administrator/channel before saving them. Pause or stop before manual intervention and inspect the remote state before resuming interrupted work.

## Public repository review

The publication review checked all reachable Git history with Gitleaks and additional checks for personal paths, emails, private network addresses, runtime databases, credentials, and task-specific data. Screenshot fixtures contain fictional names and reserved example domains; no live machine, provider account, credential, or remote desktop was used for the documentation captures.

Generated dependencies, native bundles, model weights, local databases, logs, and credentials are excluded from source control. A scan is not proof that no possible sensitive information exists. Keep future commits and uploaded issue attachments subject to the same review.

## RustDesk

Opening the client loads the official RustDesk website, or the HTTPS web-client URL explicitly saved for that machine. That site and its network services apply their own data and telemetry policies. AgentSmith does not send the saved ID or any Keychain password to the page; enter the ID and password directly in the client. The remote ID is visible in the window title for reference.

The window uses nonpersistent WebView storage, disallows navigation to other origins and new popup windows, and has no AgentSmith Tauri capability. Nonpersistent storage does not prevent server-side retention. Closing the window ends that client view; the site owns the RustDesk connection lifecycle. No frame from that window enters AgentSmith OCR or AI.

A RustDesk **session**, unlike that window, is a remote screen AgentSmith operates: its frames reach OCR and, under your routing, the configured models, exactly as an RDP session does. Its password is held in the Keychain and answered as a salted challenge, never sent as text. The rendezvous server learns which ID you connect to; a relay, when one is needed, carries the encrypted session without the key to read it.
