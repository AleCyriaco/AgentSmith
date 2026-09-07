# Source and distribution

The public repository contains AgentSmith source, documentation, and scripts that prepare native dependencies. It excludes downloaded model weights, personal configuration, runtime databases, and generated application bundles.

The 0.12.7 GitHub release publishes source and documentation. It does not include a notarized macOS installer. Existing locally built application bundles are development artifacts.

## Third-party components

| Component | Purpose | Distribution consideration |
| --- | --- | --- |
| Tauri / React / Rust crates / npm packages | Desktop runtime and interface | Retain applicable package licenses and notices |
| FreeRDP | Native RDP connection | Apache-2.0; retain its license/notices |
| Transitive native libraries | Media, networking, cryptography | Individual licenses apply; audit the exact linked build |
| llama.cpp | Built-in inference engine | MIT; the build script copies its license |
| Downloadable model weights | Local vision | Separate model licenses; inspect the pinned catalog and upstream model cards |
| Official AI clients | Browser login and inference | Provider/client terms and account restrictions apply |

The FreeRDP build script recursively copies native libraries from the local Homebrew installation. Their exact set and licensing can vary. The helper's copied FreeRDP license alone is not a complete notice set for every transitive dependency. Review those dependencies and any corresponding-source obligations before distributing a compiled bundle.

Publishing source does not imply that every third-party service, model, or remote protocol is endorsed by its owner or available without a paid account.
