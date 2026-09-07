# Contributing

Start with [Getting started](docs/getting-started.md) and [Architecture](docs/architecture.md). Keep decisions, validation, and remote input in separate components. Model output must remain a proposal validated by the engine.

## Development checks

```sh
npm ci
npm test
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
npm run build
```

Some Rust tests bind an ephemeral loopback server and need permission to listen locally. Common tests use synthetic fixtures rather than real provider keys. The ignored local-reading integration test additionally requires Apple Vision, Metal, downloaded weights, and a supplied fixture; see [local vision validation](docs/local-vision-validation.md).

Rebuild the RDP helper after native RDP changes (`npm run helper`), OCR after Swift changes (`npm run ocr`), and prepare the pinned local engine (`npm run vision`) before bundling.

## Changes and reports

Describe the concrete problem, expected behavior, change, and relevant verification. For provider changes, distinguish mocked protocol checks, synthetic live checks, and real Windows task validation. Do not infer broad compatibility from one response.

Use fictional hosts such as `windows.example.com` and synthetic screenshots. Keep secrets, machine addresses, personal paths, live task data, application databases, and downloaded binaries out of commits. Review the whole diff and newly added images before pushing. AgentSmith uses the MIT License; preserve notices and ensure contributions are compatible with it. See the distribution notes for third-party components.
