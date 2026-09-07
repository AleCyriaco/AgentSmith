# AI routing and performance

The four roles are separate responsibilities. They may share a profile, but each profile must support the kind of input it receives.

| Role | Input and responsibility | Practical starting point |
| --- | --- | --- |
| Plan | Goal and constraints → verifiable steps | A competent, economical instruction-following text model |
| Operate | OCR text, positions, current step, recent inputs → next action | A low-latency model that passes the structured action test |
| Verify | Current evidence → propose completion or continue | The same text profile as Operate permits a combined call |
| Visual assistance | Screenshot or crop → visual decision/confirmation | A tested image-capable model with useful UI localization |

**OCR itself is Apple Vision, running locally.** It is not an additional LLM profile. Text-proposed completion still receives visual confirmation in this preview, unless an explicit OCR criterion is checked directly by the engine.

## Hybrid setup

Use an economical text profile for Plan, Operate, and Verify, and a proven multimodal profile for Visual assistance. For example, the current adapter supports DeepSeek V4 Flash for text and a separate Claude profile for images. This illustrates role separation; it is not a price ranking or a guarantee of model accuracy.

Known DeepSeek text-only models are rejected when asked to accept images, even if their vision checkbox is selected. The experimental vision variant has not passed the project's synthetic localization check and should not be assumed equivalent to the text model plus OCR.

## Local setup

1. In **Providers and models → Local vision**, download a preset, test it, and add its profile.
2. Assign local profiles to every role, including Plan.
3. Enable **Local only** to reject cloud inference and cloud fallback.
4. Start with a short task and compare accuracy as well as latency.

The built-in engine is llama.cpp with Metal acceleration. It loads one model at a time and releases memory after approximately two minutes idle. Model downloads are pinned and hash-checked. The catalog includes SmolVLM 500M, Qwen2.5 VL 3B, and Qwen3-VL 2B; it does not currently include a dedicated small text-only preset. Existing multimodal models can answer text-only requests, or you can connect a compatible local text server.

Small does not automatically mean suitable for controlling a desktop. SmolVLM is lightweight but has significant reading and coordinate limitations. Qwen3-VL is a candidate for interface work, especially crops; Qwen2.5 VL may give better detail at greater memory and latency cost. See [measured results](local-vision-validation.md), which are transcription tests, not a model leaderboard.

## Providers

The catalog contains OpenAI, Anthropic, Google, xAI, Amazon Bedrock, Alibaba Cloud, DeepSeek, Moonshot AI, Z.ai, and ByteDance/BytePlus. It also accepts Ollama, LM Studio, and compatible local servers. This is a supported configuration catalog, not a market-share ranking or certification of every model/account.

API adapters cover Responses, Chat Completions, Anthropic Messages, and Bedrock Converse. Browser-login profiles use separate official clients; their model access and limits may differ from API accounts. Keep model IDs configurable and test them rather than assuming a chat subscription enables every API model.

## Tune speed without hiding failures

| Setting | Allowed range | Effect |
| --- | --- | --- |
| Capture interval | 20–2,000 ms | Target frame update interval; shorter intervals cost more CPU |
| Additional delay after input | 0–3,000 ms | Extra settling time before observing the result |
| Turbo preset | 50 ms capture, 0 ms additional delay, 1,280 px image width | A starting point for responsive applications |
| Balanced preset | 300 ms capture, 650 ms delay, 1,600 px image width | More time for slower application transitions |

Capture frequency is not inference frequency. The loop still waits for an observation and checks whether an action is based on current pixels. OCR, network latency, model loading, generation, and Windows rendering can dominate total time. Zero additional delay does not eliminate verification.

Keep OCR and crops enabled where useful, avoid unnecessarily long plans, and compare the same task from the same starting screen. Report time to completion, incorrect actions, interruptions, and provider usage, not just tokens per second.

## Tests and fallbacks

**Test connection** confirms a text response. **Test operator** checks a synthetic OCR target and JSON action contract without operating Windows. Neither certifies vision or complete task execution.

Invalid structures receive one bounded repair attempt per profile. Only configured alternatives are eligible; genuine task restrictions remain blocking conditions. Visual escalation handles insufficient OCR or missing progress. Repeated unchanged screens stop execution for review rather than increasing the action rate indefinitely.
