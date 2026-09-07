# RustDesk transport

AgentSmith speaks the RustDesk protocol directly. A RustDesk destination is a session like an RDP one: frames reach OCR and the visual models, and the executor's mouse and keyboard actions reach the machine. The manual web client is still available, now as a separate convenience rather than the only option.

## Connect a machine

1. On the Windows machine, open RustDesk, note the ID, and set a permanent password. Without one, every connection needs someone to approve it on that machine.
2. In AgentSmith, choose **Add machine → RustDesk**, and enter the name, the ID, and that password. RustDesk shows the ID grouped in threes; pasting it that way is fine, the spaces are dropped. The password is stored in the macOS Keychain under the RustDesk ID, separate from any RDP password for the same host.
3. For a self-hosted server, set it once under **Machines → RustDesk → Default server**: the ID server address and its base64 public key, exactly as RustDesk shows them under Settings → Network → ID/Relay server. A machine may override both in its own form; with neither set, RustDesk's public server is used. The key is what authenticates the machine, so a self-hosted server without one cannot open a session.
4. Select the machine and choose **Connect**. **Manual client** still opens the official web client in its own window.

## What the transport does

The rendezvous server is asked for the machine; AgentSmith connects directly when the network allows it and falls back to a relay when it does not. Two signatures are checked before anything else: the rendezvous server signs the machine's signing key, and the machine then signs its own session key. Both must verify, and both must name the ID you asked for.

**AgentSmith refuses a session it cannot authenticate.** RustDesk itself falls back to an unencrypted connection when no signed identity is available; AgentSmith does not, because an AI loop typing and clicking through a session must not run over one that a machine on the path could read or redirect. A destination without a verifiable identity fails to connect and says why.

The password never travels. The machine sends a salt and a per-connection challenge, and AgentSmith answers with `sha256(sha256(password + salt) + challenge)`.

## Video and input

AgentSmith announces VP8 and VP9 only, the codecs it decodes, so a machine cannot answer with a stream that would arrive as a blank screen. Frames are decoded with libvpx, linked statically, and converted to the same RGBA images the RDP transport produces — the OCR, vision, verification and repetition paths are unchanged.

Actions cross unchanged too. Clicks move the pointer first. Shortcuts travel as layout keys with their modifiers, so the machine applies Ctrl+S as a shortcut. Typed text travels as Unicode, so the machine's keyboard layout cannot change which characters arrive. Pausing releases every button and modifier. A test asserts that the RDP and RustDesk transports accept and refuse exactly the same actions, since the executor does not know which one it is driving.

Audio, clipboard and file transfer are disabled at login. AgentSmith reads the screen; the rest is surface it does not need.

## Second factor

A machine with two-factor authentication answers the password with a challenge. The code is time-based and never stored; it is supplied per connection. AgentSmith sends no hardware id with it, so the machine is not asked to trust this Mac and **every** connection asks for a fresh code. That makes unattended runs impractical on such a machine; asking the machine to remember this one is a persistent change to its security and is deliberately left to a later, explicit choice.

## Current limits

- **Verified once, against one machine.** A session reached a Windows 11 host running RustDesk 1.4.9 through a self-hosted rendezvous server: relay path, both signature layers, the cipher, a second-factor challenge, VP9 negotiation, and a key frame followed by delta frames decoded to a 1800×1130 opaque image. Not yet exercised: the direct (non-relay) path, input actually reaching the machine, a machine offering more than one Windows session, the public rendezvous server, and sessions longer than a few seconds.
- One display: the machine's current display sets the session resolution. Switching displays mid-session is not implemented.
- A Windows machine running several sessions is attached to the one it marks active, or otherwise the first it offers. RustDesk asks a person; AgentSmith cannot, so the choice is fixed and not yet configurable per machine.
- The interface has nowhere to enter a second-factor code, so a machine that requires one connects only from the live check, not from the application.
- Direct connections and relays over TCP only. The UDP, KCP and WebRTC transports newer RustDesk builds can negotiate are not implemented; a machine reachable only that way will not connect.
- No file transfer, clipboard, audio, or mouse dragging.
- Resolution and scale settings apply to RDP; a RustDesk machine reports its own.

## Licensing

No RustDesk source is copied into AgentSmith, and none is linked. The wire format is described independently in `src-tauri/protos/rustdesk.proto` from the project's public protocol definitions, and implemented here. AgentSmith stays MIT; RustDesk is AGPL-3.0 and remains a separate program. libvpx is BSD-3-Clause. The manual web client still loads a third-party website under its own terms.

## Privacy

A RustDesk session is a remote screen like any other: what the AI sees is what the configured models receive, under the routing you set. See [privacy](privacy.md). The manual client window remains isolated, without AgentSmith IPC or Keychain access.
