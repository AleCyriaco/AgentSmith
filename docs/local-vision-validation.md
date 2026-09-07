# Local vision: limited synthetic measurements

These observations were recorded on 2026-09-06 on an Apple Silicon Mac using optimized Rust, llama.cpp b10830, and Apple Vision. They used one synthetic 1600 × 900 Calculator image with distracting expression/history text. Target region: x=260, y=270, width=370, height=80. Expected exact string: `16.666.666`, withheld from the model.

| Reader | Mode / pass | Seconds | Exact text correct |
| --- | --- | ---: | --- |
| Apple Vision | OCR | 0.829 | Yes |
| Qwen3-VL 2B | Full image / 1 | 18.263 | No |
| Qwen3-VL 2B | Crop / 1 | 0.651 | Yes |
| Qwen3-VL 2B | Full image / 2 | 5.257 | No |
| Qwen3-VL 2B | Crop / 2 | 0.655 | Yes |
| Apple Vision | OCR | 1.362 | Yes |
| Qwen2.5 VL 3B | Full image / 1 | 24.474 | Yes |
| Qwen2.5 VL 3B | Crop / 1 | 1.344 | Yes |
| Qwen2.5 VL 3B | Full image / 2 | 10.241 | Yes |
| Qwen2.5 VL 3B | Crop / 2 | 1.315 | Yes |

The first full-image calls include model verification/loading. Later calls reuse the loaded model. Model times include the request and answer, but exclude prior crop preparation; OCR times include helper startup and recognition. Other Mac applications were not isolated. There are no percentile measurements or speed guarantees.

Qwen3 returned the whole expression rather than only the requested number on both full-image attempts. It returned the exact display text when cropped. Qwen2.5 and OCR met the exact transcription criterion in these observations. One repeated synthetic screen does not establish general accuracy, UI reasoning quality, or click localization.

Use crops as a candidate optimization and evaluate real tasks separately. No automatic routing decision is justified by these timings alone. Moondream2 was not included in this measurement.

## Reproduction

Create the fixture with `scripts/make_vision_fixture.swift`. The ignored Rust integration test `same_image_local_comparison` accepts `AGENTSMITH_BENCH_ROOT` (containing local-vision), `AGENTSMITH_BENCH_IMAGE`, `AGENTSMITH_BENCH_RULE` (TextCheck JSON), and `AGENTSMITH_BENCH_RESULT` (output JSON). It needs the native OCR helper, Metal, model weights, and loopback access.

The app's **Expected text (OCR) → Compare reading** uses a frozen frame and an expected value withheld from the model. It compares full-frame/crop reading and does not send mouse/keyboard input. Review sensitive contents before sharing results.
