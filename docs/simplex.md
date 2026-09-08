# SimpleX on your Mac

AgentSmith can prepare a private SimpleX messaging server on the same Mac that runs your Windows sessions. Your phone connects using QR codes. Messages can notify you when work needs attention and let you answer a pending request to continue or stop. Pocket remains the web panel for viewing sessions, submitting goals and approving individual inputs.

## Quick setup

1. Connect Tailscale on the Mac and your iPhone or Android. Install **SimpleX Chat** on your phone.
2. Open **Notifications and decisions → SimpleX** in AgentSmith and choose **Set up and enable on this Mac**.
3. On the phone, open SimpleX **Settings → Network & servers → Your servers → Add server → Scan server QR code**. Scan the first QR, test and save it. Do not scan the server QR in **New chat**; that scanner accepts contact links only. Labels may differ between SimpleX versions.
4. Use this server for new connections. To keep both directions on this Mac, disable the default message servers. A private Tailscale server requires a direct connection. Save the server before scanning a contact QR. Under **Network & servers → Advanced network settings → Private routing**, use the option for unknown servers rather than Always. Registered servers are known, so this preserves routing protection for unknown servers while allowing a direct connection to your Mac.
5. In AgentSmith, select **I tested and saved the server on my phone → Show QR to connect phone**. In SimpleX, add a contact by scanning the second QR.
6. Send a message to AgentSmith, then choose **Confirm → Send test notice** on the Mac.

The two QR codes serve different purposes: the first selects a messaging server; the second adds the AgentSmith contact. SimpleX does not offer an account-sync QR that combines these operations. Pairing does not copy your personal account or existing chat history.

Keep the Mac awake and AgentSmith open while using the channel. The Mac and phone must be connected to the same Tailscale network with access to TCP port 5223. An unavailable Mac cannot deliver new messages. Previously paired contacts remain in the local SimpleX database across application restarts. After reopening AgentSmith, activate the channel again; no new pairing is needed.

## What AgentSmith prepares

- A dedicated Podman machine named `agentsmith-simplex`, with 2 virtual CPUs, 2 GB of memory and a 10 GB virtual disk. Existing Podman environments and the default connection are preserved. If another Podman machine is running, AgentSmith asks you to stop it instead of interrupting it.
- The official SimpleX SMP server v6.5.0 image, pinned by digest. It is downloaded only when absent.
- Persistent private volumes for certificates, server configuration and queued encrypted messages.
- A loopback listener on `127.0.0.1:17423` and a Tailscale Serve TCP forward from port 5223. The existing Pocket HTTPS forward on port 443 is preserved. A conflicting port 5223 configuration produces an error instead of being overwritten.
- An independent official SimpleX CLI process, using an AgentSmith-specific profile and a newly allocated loopback API port. It never attaches to an old client left on a fixed port. If no existing CLI is installed, AgentSmith downloads v7.0.2 for the Mac's architecture and verifies its SHA-256 before running it.

**Prerequisites:** Tailscale and Podman must be installed on the Mac. AgentSmith automatically creates and starts its dedicated environment, server and messaging client; it does not install system software requiring administrator privileges. The first setup downloads the virtual-machine image, server and, when needed, client. Later starts reuse them.

The server uses SimpleX's pinned certificate identity. Tailscale transports its TLS connection unchanged. There is no public-internet listener or Tailscale Funnel configuration.

## Pairing and decisions

Choosing **Show QR to connect phone** enables contact acceptance for five minutes. Only show or share the contact QR with someone allowed to respond to AgentSmith's decisions. It is a temporary acceptance window, not a single-use QR. Established contacts retain access after the window ends. Turning off SimpleX stops the messaging client, managed server and its dedicated virtual machine, while preserving their data and existing contacts.

The server QR contains a queue-creation credential. It is shown only in the desktop configuration screen and must not be published. The server stores its credentials inside its private configuration volume. Advanced external-server addresses are stored in macOS Keychain.

SimpleX's incoming replies apply to the pending question. They do not provide arbitrary remote shell access or replace Pocket's individual-action approval controls. Use the Pocket panel for new goals and live session interaction.

## External server

Expand **Advanced: use an external server**, paste its `smp://` address and save. This preserves the existing external-server workflow. Configure the same server in the phone's SimpleX app if both directions should use that relay. External and managed-local clients use separate databases so an existing contact does not silently move to a different server.

## Components and licenses

AgentSmith remains MIT licensed. SimpleX programs run as separate processes and retain their own AGPL-3.0 licenses; their code is not linked into AgentSmith. Podman and Tailscale retain their respective licenses.

- [Official SimpleX server documentation](https://simplex.chat/docs/server.html)
- [SMP server v6.5.0 source and release](https://github.com/simplex-chat/simplexmq/releases/tag/v6.5.0)
- [Official SimpleX Chat v7.0.2 source and release](https://github.com/simplex-chat/simplex-chat/releases/tag/v7.0.2)

## Pairing errors

- **Invalid QR code** in New chat: this is the server QR. Scan it in **Your servers → Add server → Scan server QR code**. Then use the contact QR from step 2 in New chat.
- **Private routing error** mentioning a public server: the phone attempted to use a public forwarding server. Public servers cannot reach your private Tailscale Mac. Save the Mac server first and check the private-routing setting above. Verify Tailscale is connected on both devices, then retry with a valid contact pairing window.

See [SimpleX's explanation of known servers and private routing](https://simplex.chat/faq/#does-simplex-protect-my-ip-address).
