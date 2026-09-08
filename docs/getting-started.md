# Getting started

## Requirements

The current native build targets Apple Silicon and macOS 26+. Earlier macOS releases and Intel Macs have not been validated. The target computer must provide Microsoft RDP, allow the selected account to connect, and be reachable through your network or VPN.

Build prerequisites: Node.js compatible with Vite 7, npm, Rust, Xcode Command Line Tools, Homebrew FreeRDP 3, and Python 3.12+ for the archive extraction script. Network access is needed to obtain dependencies, the pinned llama.cpp runtime, and any model weights you choose to download.

## Build and open

```sh
npm ci
brew install freerdp
npm run helper
npm run vision
npm run ocr
npm run desktop
```

`npm run desktop` launches the native development application. `npm run dev` opens only the browser interface preview; it cannot connect to Windows or run native AI features.

For a bundle, run `npm run bundle` after preparing the native helpers. Open `src-tauri/target/release/bundle/macos/AgentSmith.app` in Finder. This preview is locally signed, not notarized. Build from a trusted checkout; do not disable system-wide macOS security settings.

## Configure a first task

1. Choose **English** from the language menu if desired.
2. Open **Machines → Add machine**. Enter a name, hostname, port, Windows account and optional domain. RDP and RustDesk both carry AI execution; a RustDesk destination needs its ID and permanent password instead. See the [RustDesk transport guide](rustdesk.md).
3. Optionally save the password. It is stored in macOS Keychain and bound to the machine and connection details. An empty password field while editing preserves the saved credential.
4. Open **Providers and models**, add a profile, and run **Test connection** followed by **Test operator** where available. A connection test only checks a text response.
5. Open **AI routing**. Select a planner, operator, verifier, and image-capable visual assistant. See [routing guidance](ai-routing.md).
6. Return to **Operations center**, select the machine, and connect. If the certificate is unrecognized, verify its fingerprint through a trusted channel before saving it. TLS validation is not disabled.
7. Describe the desired result and any restrictions. Click **Prepare plan**, review the actions and expected outcomes, then **Run**.

## Everyday controls

| Control | Behavior |
| --- | --- |
| Pause / Resume | Stop new automation inputs; resume by observing current state |
| Stop / Esc during execution | End the run and preserve history; transmitted inputs remain applied |
| Restart | Create a run from the first step while retaining the earlier history |
| Edit plan | Change title, instructions, order, actions, and expected results; executed plans produce a new version |
| Delete plan | Permanently delete the selected run/history after confirmation; does not undo Windows actions |
| Repeat | Create a repeated run using a duration, time window, or weekdays and dates |
| Take control | Use the remote keyboard/mouse after automation has paused |
| Display / Zoom | Change remote resolution and scaling, or fit the current image in the viewport |
| Pace | Tune frame capture, post-action delay, image width, OCR, and crops |
| Collapse / Expand RDP / Detach | Allocate more space or move the remote view to a separate window |

## Repeated work

Use **Repeat** to select a duration or time window. The weekday option also accepts start/end dates and uses the Mac's local time. Overnight windows belong to their starting day. End dates are inclusive; execution does not extend past midnight at the end of the selected day.

Keep the Mac awake, AgentSmith open, and the Windows session connected. Errors suspend repetition for review. Pauses do not extend the configured period. Each cycle starts at the first step; use tasks that are appropriate to repeat.

## After a restart or interruption

Reconnect manually, inspect the Windows state, and resume only when the task is still appropriate. Pending operations are not guaranteed to have executed exactly once. If earlier steps were incorrectly confirmed, restart or edit the plan instead of trusting stale progress.

## Alerts and decisions

Unattended work has nobody at the screen, so AgentSmith can reach you over SimpleX instead. Open **Alerts and decisions** and follow the three steps: paste the `smp://` address of a SimpleX server you run, pair your own SimpleX with the address AgentSmith shows, and send a test alert to prove the path. The address is handed over as an `https://simplex.chat/` link, which pastes into any SimpleX client; the app-scheme form the client answers with is not accepted pasted and macOS does not open it.

Every task that ends produces a notice saying what happened and how far it got. A task that blocks is put as a question instead: answer **1** to resume it or **2** to stop it, from wherever you are. Only an unambiguous reply counts — anything else is left unanswered rather than guessed.

This needs the official `simplex-chat` client at `~/.local/bin/simplex-chat`; there is no Homebrew formula for it, and the desktop app exposes no API. The server address carries a password that allows creating queues on that relay, so it is kept in the Keychain and never written to settings.

Setting a server decides where the queues AgentSmith creates live. The reply queue is chosen by the client on the other side, so configure the same server in your own SimpleX — under its network and servers settings — to keep both directions on your relay, and test it there before leaving the screen. Changing a server later affects new contacts only; existing ones stay where they were.

Reach depends on whatever network the server's certificate was issued for. If it names a private address, every device needs to be on that network, which is the most common reason an alert does not arrive.
