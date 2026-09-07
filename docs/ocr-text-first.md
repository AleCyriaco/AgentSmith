# OCR-first operation

A frame is captured from the remote RDP session. Apple Vision extracts text, confidence, and positions locally. The text path receives compact OCR elements with observation-scoped IDs, the authorized plan, the current criterion, and recent input context.

For an OCR click, the model chooses a current target ID. The engine resolves the position; invented IDs or insufficient confidence cannot become clicks. Icons, focus states, blank inputs, ambiguous text, and insufficient evidence can trigger visual assistance.

## Completion

A text model can propose completion but cannot confirm a normal step alone: the visual assistant checks the current screenshot and original criterion. This adds a visual call at completion boundaries. An explicit expected-text rule instead uses direct OCR validation, with exact text/punctuation, confidence threshold, a unique match contained in the configured region, matching resolution, and unchanged region pixels before confirmation.

Use exact OCR rules only when the visible text proves the whole step. A number visible elsewhere on the screen is not enough. Moving the target window can invalidate the region. Repeated cycles require the criterion to leave the matching state before it can be confirmed again.

## Caching and crops

At most two observations are cached in memory per execution. Exact pixels and resolution must match before an OCR result is reused. Changes outside a crop do not invalidate that crop's reading but do require a new whole-screen observation. Pause/resume, restart, and a new repetition cycle start a new cache. Cached actions are never replayed.

The visual model can request a crop with `inspect`. This changes its observation, not the Windows state. The engine maps coordinates from resized/cropped images to the remote session. Old OCR is not paired with a new image. A changed image can invalidate a pending decision.

## Failure and progress

Invalid text output gets a bounded repair attempt or configured fallback. Insufficient information escalates to vision. Three inputs without pixel change stop the attempt for review. Pixel change is only a progress signal: animation can change pixels without advancing the task, and hidden application work can happen without a visible change.

The current engine can still stop when an intermediate step is no longer applicable even if the overall task is already complete. This is a known goal-reconciliation limitation, not proof of an authentication or routing failure.
