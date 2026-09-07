# Troubleshooting

| Symptom | What to check |
| --- | --- |
| “Interface preview” / native features unavailable | Open the `.app` or use `npm run desktop`; a browser tab is only the UI preview |
| Stuck connecting or authenticating | Check VPN, reachability, RDP host settings, account/domain, and credentials |
| Unrecognized TLS certificate | Verify the fingerprint through a trusted channel, then save it in the machine profile |
| Password requested every time | Save it in the machine profile; confirm Keychain access and unchanged endpoint/account binding |
| Official client exits with code 1 | Inspect the categorized error; verify login, selected model, limits, and compatible client version |
| Gemini refuses `session/new` | Verify account login, model availability, quota, and client protocol/version |
| Invalid model structure | Run Test operator; use a model that follows the JSON contract; do not accept raw prose as an action |
| Truncated model output | Prefer the appropriate short-action path/model; truncated JSON is intentionally never executed |
| Text model requested for images | Keep it in text roles and choose a true image-capable profile for Visual assistance |
| No progress after three inputs | Inspect focus, loading state, target coordinates, display scale and expected result; increase settling time if needed |
| Goal looks complete but an intermediate step blocks | Inspect/edit the plan; the preview does not fully reconcile every already-satisfied overall goal |
| Earlier error remains visible | History preserves previous failures; distinguish the latest run and timestamp from older entries |
| Slow local model | Separate initial loading from inference; try a crop, smaller image, or another model and compare accuracy |
| Delete plan does not apply | Pause/stop active work first; confirm the selected run; refresh if another update changed its version |

When reporting a bug, include AgentSmith version, macOS/chip, connection type, authentication method, model identifier, a sanitized task, and expected versus observed behavior. Never attach tokens, passwords, the live SQLite database, or an unreviewed remote screenshot. See [Contributing](../CONTRIBUTING.md).
