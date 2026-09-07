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

The machine can stream far faster than the interface reads. Every frame is decoded, because a delta frame needs the ones before it, but only one per **Ritmo** interval is converted to an image and encoded for display; the rest cost only the decode. The machine sends one latency probe at a time and waits for it to come back before sending the next; AgentSmith echoes each one untouched. That echo is also what keeps the session alive: the machine closes a connection it has not heard from for thirty seconds. A RustDesk machine sends nothing while its screen is still, unlike the RDP connector which pushes a frame on a fixed interval. A picture is therefore held as current for as long as the machine is heard from at all, and expires only when it goes silent — the age of the last frame measures how long the desktop has been unchanged, not whether the session is alive. A frame that fails to decode costs that one picture; if it was a delta frame, a key frame is requested rather than waited for.

The session line names the machine's host name, the account it is signed in as, its platform, its RustDesk version, and the resolution, because what a plan can reach and how the machine behaves depend on all of them.

Actions cross unchanged too. Clicks move the pointer first. Shortcuts travel as layout keys with their modifiers, so the machine applies Ctrl+S as a shortcut. Typed text travels as Unicode, so the machine's keyboard layout cannot change which characters arrive. Pausing releases every button and modifier. A test asserts that the RDP and RustDesk transports accept and refuse exactly the same actions, since the executor does not know which one it is driving.

Audio, clipboard and file transfer are disabled at login. AgentSmith reads the screen; the rest is surface it does not need.

## Second factor

A machine with two-factor authentication answers the password with a challenge. AgentSmith asks for the current six-digit code and connects again with it. The code is time-based and is never stored.

The machine remembers a trusted device by four things together: the hashed identity, the controller id, the controller name, and the platform. AgentSmith presents all four on every login, so a machine that was asked to trust this Mac recognises it and skips the second factor; one that was not asked ignores them, as it already knows the controller id. Both ids derive from the same stable seed, so neither changes between the connection that trusted and the ones after it.

**Trust this Mac** is offered beside that field, unticked. It is available only when the machine keeps trusted devices; when it does not, the box is disabled and says so, rather than being ticked to no effect. Ticking it sends a hashed, stable identity for this installation, and the machine stops asking this computer for a code. That is what makes unattended runs possible on such a machine, and it is a lasting reduction of that machine's protection: from then on, anyone with the password, from this Mac, gets in without a second factor. The identity is a hash, so the machine can recognise this Mac again without learning anything about it. AgentSmith never sends it unless the box is ticked.

## Current limits

- **Verified once, against one machine.** A session reached a Windows 11 host running RustDesk 1.4.9 through a self-hosted rendezvous server: relay path, both signature layers, the cipher, a second-factor challenge, VP9 negotiation, and a key frame followed by delta frames decoded to a 1800×1130 opaque image. Pointer moves were then accepted and the picture changed after them, which is the evidence available that input reached the machine short of watching its screen. Not yet exercised: the direct (non-relay) path, a machine offering more than one Windows session, the public rendezvous server, clicks and typing, and sessions longer than half a minute.
- One display: the machine's current display sets the session resolution. Switching displays mid-session is not implemented.
- A Windows machine running several sessions is attached to the one it marks active, or otherwise the first it offers. RustDesk asks a person; AgentSmith cannot, so the choice is fixed and not yet configurable per machine.
- Trusting a Mac cannot be undone from AgentSmith; revoke it in RustDesk on the machine itself.
- Direct connections and relays over TCP only. The UDP, KCP and WebRTC transports newer RustDesk builds can negotiate are not implemented; a machine reachable only that way will not connect.
- No file transfer, clipboard, audio, or mouse dragging.
- Resolution and scale settings apply to RDP; a RustDesk machine reports its own.

## Licensing

No RustDesk source is copied into AgentSmith, and none is linked. The wire format is described independently in `src-tauri/protos/rustdesk.proto` from the project's public protocol definitions, and implemented here. AgentSmith stays MIT; RustDesk is AGPL-3.0 and remains a separate program. libvpx is BSD-3-Clause. The manual web client still loads a third-party website under its own terms.

## Privacy

A RustDesk session is a remote screen like any other: what the AI sees is what the configured models receive, under the routing you set. See [privacy](privacy.md). The manual client window remains isolated, without AgentSmith IPC or Keychain access.
