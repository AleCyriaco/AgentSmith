# RustDesk transport

AgentSmith speaks the RustDesk protocol directly. A RustDesk destination is a session like an RDP one: frames reach OCR and the visual models, and the executor's mouse and keyboard actions reach the machine. The manual web client is still available, now as a separate convenience rather than the only option.

## Connect a machine

1. On the Windows machine, open RustDesk, note the ID, and set a permanent password. Without one, every connection needs someone to approve it on that machine.
2. In AgentSmith, choose **Add machine → RustDesk**, and enter the name, the ID, and that password. The password is stored in the macOS Keychain under the RustDesk ID, separate from any RDP password for the same host.
3. Leave **Self-hosted server** empty to use RustDesk's public rendezvous server. For your own server, enter its address and its base64 public key; the key is what authenticates the machine.
4. Select the machine and choose **Connect**. **Manual client** still opens the official web client in its own window.

## What the transport does

The rendezvous server is asked for the machine; AgentSmith connects directly when the network allows it and falls back to a relay when it does not. Two signatures are checked before anything else: the rendezvous server signs the machine's signing key, and the machine then signs its own session key. Both must verify, and both must name the ID you asked for.

**AgentSmith refuses a session it cannot authenticate.** RustDesk itself falls back to an unencrypted connection when no signed identity is available; AgentSmith does not, because an AI loop typing and clicking through a session must not run over one that a machine on the path could read or redirect. A destination without a verifiable identity fails to connect and says why.

The password never travels. The machine sends a salt and a per-connection challenge, and AgentSmith answers with `sha256(sha256(password + salt) + challenge)`.

## Video and input

AgentSmith announces VP8 and VP9 only, the codecs it decodes, so a machine cannot answer with a stream that would arrive as a blank screen. Frames are decoded with libvpx, linked statically, and converted to the same RGBA images the RDP transport produces — the OCR, vision, verification and repetition paths are unchanged.

Actions cross unchanged too. Clicks move the pointer first. Shortcuts travel as layout keys with their modifiers, so the machine applies Ctrl+S as a shortcut. Typed text travels as Unicode, so the machine's keyboard layout cannot change which characters arrive. Pausing releases every button and modifier. A test asserts that the RDP and RustDesk transports accept and refuse exactly the same actions, since the executor does not know which one it is driving.

Audio, clipboard and file transfer are disabled at login. AgentSmith reads the screen; the rest is surface it does not need.

## Current limits

- **Not yet verified against a live RustDesk machine.** The protocol, cipher, address handling, colour conversion and action translation are covered by unit tests, and the identity checks are tested against wrong and forged signatures. An end-to-end session with a real Windows host is not recorded as verified.
- One display: the machine's current display sets the session resolution. Switching displays mid-session is not implemented.
- Direct connections and relays over TCP only. The UDP, KCP and WebRTC transports newer RustDesk builds can negotiate are not implemented; a machine reachable only that way will not connect.
- No file transfer, clipboard, audio, or mouse dragging.
- Resolution and scale settings apply to RDP; a RustDesk machine reports its own.

## Licensing

No RustDesk source is copied into AgentSmith, and none is linked. The wire format is described independently in `src-tauri/protos/rustdesk.proto` from the project's public protocol definitions, and implemented here. AgentSmith stays MIT; RustDesk is AGPL-3.0 and remains a separate program. libvpx is BSD-3-Clause. The manual web client still loads a third-party website under its own terms.

## Privacy

A RustDesk session is a remote screen like any other: what the AI sees is what the configured models receive, under the routing you set. See [privacy](privacy.md). The manual client window remains isolated, without AgentSmith IPC or Keychain access.
