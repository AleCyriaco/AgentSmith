# RustDesk client — manual preview

Version 0.13.0 introduces a RustDesk web-client window inside the AgentSmith desktop application. It is a manual connection milestone, not yet an OCR/AI transport.

## Try it

1. Open **Machines → Try RustDesk client**. The official client opens in its own window.
2. In RustDesk on the target Windows machine, obtain its remote ID and authorize access.
3. Enter the ID in the web client and provide its password or obtain approval on Windows.
4. Use the mouse and keyboard in that window. Verify the connection on the remote screen itself.

Opening the client does not prove that Windows is connected. No attempt is made to bypass host approval or authentication. A real session needs the user's authorized RustDesk host.

## Save a destination

Use **Add machine**, select **RustDesk · manual preview**, and enter a name and RustDesk ID. The **Web client URL** can remain empty to use `https://rustdesk.com/web/`, or point to your own HTTPS client. Supply a client page URL, not a raw ID/relay address. URLs with embedded credentials, query parameters, or fragments are rejected.

Select the saved machine and choose **Open RustDesk client**. The ID is shown in the window title and in a selectable field in the Operations center. Enter credentials directly in the RustDesk client; this integration does not read or forward the RDP password or other Keychain secrets. Existing RDP configuration is preserved.

A nonpersistent browser session is used. Client preferences and authentication may need to be entered again after the window closes. Third-party account login popups and navigation to another origin are intentionally unavailable in this initial integration.

## Servers and compatibility

The official client can use RustDesk's public infrastructure. For a self-hosted server, configure the client's ID/relay settings and the server's WSS/CORS support. The official documentation describes WebSocket endpoints on ports 21118/21119. Hosting the current official web client yourself has separate RustDesk Server Pro requirements. A custom URL does not configure your relay automatically. [Official web-client guide](https://www.rustdesk.com/blog/rustdesk-web-client-v2-preview/)

The page is rendered by macOS WebKit. Rendering the start page is not certification of every codec, keyboard layout, server, or Windows version. If authentication, video, or input fails, compare the same host with RustDesk's supported standalone/browser client and check the server configuration.

## What remains to implement

AgentSmith cannot capture RustDesk frames, feed them into OCR/vision, or execute AI mouse/keyboard proposals over this client yet. Plan generation and execution for RustDesk destinations are blocked with a specific explanation. The RDP executor remains separate.

The next transport milestone needs a stable, licensed bridge for decoded frames, input, actual connection state, target identity, cancellation, and permission changes. It must validate observations and coordinates before input and pass the same executor tests as RDP. DOM injection or simply opening the website is not such a bridge.

## Licensing and privacy

No RustDesk or community-fork source is copied into AgentSmith by this integration. AgentSmith remains MIT; the loaded website, service, and any future incorporated component retain their own terms. The community web-client fork is a separate project, not bundled here. [Community fork](https://github.com/MonsieurBiche/rustdesk-web-client)

The website sees the browser connection and the data you enter there. It may use third-party services or telemetry. The isolated window has no AgentSmith IPC permissions or access to its credentials; no RustDesk frame is sent to an LLM by AgentSmith. See [privacy](privacy.md).

## Validation for 0.13.0

80 Rust tests and 27 frontend tests passed; one environment-dependent local-vision test remains ignored. The native macOS build and local signature verification passed. The RustDesk site rendered in the native WebKit window, and a saved destination opened its own client window. Authenticated video/input to the Windows host is not yet recorded as verified; loading and navigating the web UI is not an end-to-end connection test.
