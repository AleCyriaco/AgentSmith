# Browser login through official clients

AgentSmith offers browser-login profiles for four providers. It delegates account authentication and inference to their official client programs installed on the Mac. This is not a general conversion of a chat subscription into an API key.

| Provider | Client used by AgentSmith | Integration |
| --- | --- | --- |
| OpenAI | Codex | ChatGPT login through app-server; inference through Codex exec |
| Anthropic | Claude Code | Claude account login; streaming input/output through the CLI |
| Google | Gemini CLI | ACP personal OAuth authentication and session prompts |
| xAI | Grok Build | OAuth login and ACP; structured output for harness requests |

## Connect

Open **Providers and models**, select the provider, then **Login through browser**. Install the component if it is missing, start login, and complete authentication on the official page. Installing a component requires Node.js/npm and does not purchase or enable a subscription.

Use `default` for the official client's default model, or enter an identifier actually available to the account. Run **Test connection** before saving. Opening the browser alone does not prove authentication succeeded. A successful text test does not prove image support or operator accuracy.

Sessions belong to the official client and are shared between profiles for that provider on this Mac. Creating two profiles does not create isolated accounts. AgentSmith does not copy account passwords or token stores. API-key profiles are separate and use their configured endpoint credentials.

## Compatibility and limits

Model availability, quotas, permissions, client versions, and provider policies apply. API access and subscription-client access are distinct. The interface's vision checkbox is a capability declaration, not an upgrade: the selected model and client transport must accept images. Local-only mode rejects these cloud profiles.

Provider agent tools are restricted by the integration; Windows inputs continue through AgentSmith's executor. Cancellation terminates the associated request process group. Official clients may have their own persistent logs or session policies; see [privacy](privacy.md).

## Troubleshooting

A nonzero exit code alone does not identify the cause. Check client installation, authenticated account, model access, quota, and version. Claude errors are categorized when recognizable; unknown failures remain unknown instead of being labeled as an account failure without evidence. Raw client output is not copied into task history.

For `session/new` refusal, confirm that the installed client supports the expected protocol and that its session can use the selected model. Try its default model before selecting a more specific identifier. See [troubleshooting](troubleshooting.md).
