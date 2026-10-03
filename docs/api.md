# Asset Forge local API v1

Start `asset-forge serve`. The transport is newline-delimited UTF-8 JSON. Each request has an opaque string or number `id`, a `method`, and an optional `params` object. No TCP port is opened. One caller owns the process and its workspace. Lines are limited to 1 MB.

```json
{"id":"r1","method":"projects/list","params":{"page":1,"pageSize":20}}
{"id":"r1","result":{"data":[],"pagination":{"page":1,"pageSize":20,"totalItems":0,"totalPages":0}}}
```

Errors always use the same shape:

```json
{"id":"r1","error":{"code":"VALIDATION_ERROR","message":"page must be positive and pageSize must be between 1 and 100."}}
```

Match errors using `code`; messages are human-facing. Malformed request envelopes return an error with `id: null`. Fields use camelCase; enum values use UPPER_SNAKE_CASE. Unknown input fields fail validation. IDs are opaque and unique by resource type. Optional additions to the v1 contract remain backwards compatible; breaking changes require an explicit migration.

## Methods

| Method | Params | Result | Retry behavior |
| --- | --- | --- | --- |
| `system/info` | `{}` | API version, app version, data directory, transport and capabilities | Safe |
| `styles/presets/list` | `{}` | Finite array of six `StylePreset` values | Safe |
| `assistant/message` | `AssistantInput` | `AssistantSession` immediately | Idempotent by `requestId` |
| `assistant/get` | `{id}` | `AssistantSession` | Safe |
| `assistant/list` | Pagination + optional `projectId` | `Page<AssistantSession>` | Safe |
| `assistant/cancel` | `{id}` | Current `AssistantSession`; completion arrives later | Safe |
| `account/read` | `{}` | `AccountStatus` | Safe |
| `account/login/start` | `{}` | `{loginId, authUrl}` | Starts a new flow; unsafe to retry automatically |
| `account/login/cancel` | `{id: loginId}` | `{isCancelled: true}` | Safe |
| `projects/create` | `CreateProject` | `Project` | Creates a resource; unsafe to retry automatically |
| `projects/list` | Pagination | `Page<Project>` | Safe |
| `projects/get` | `{id}` | `Project` | Safe |
| `projects/update` | `{id, name}` | Renamed `Project`; style/assets preserved | Reapply same name |
| `projects/delete` | `{id}` | `Deletion` receipt; hides game contents | A repeated deletion returns `NOT_FOUND` |
| `projects/restore` | `{id}` | Restored `Project` | Safe |
| `projects/style/update` | `{projectId, style: StyleGuide}` | `Project` | Replaces style; retry only the same intended replacement |
| `characters/create` | `CreateCharacter` | `Character` | Creates a resource; unsafe to retry automatically |
| `characters/get` | `{id}` | `Character` | Safe |
| `characters/list` | Pagination + optional `projectId` | `Page<Character>` | Safe |
| `characters/update` | `{id, name?, description?, kind?}` | `Character` | Applies only supplied fields; retry the same intended changes |
| `characters/references/update` | `{id, referenceAssetIds}` | `Character` | Replaces references; retry only the same intended replacement |
| `assets/import` | `{projectId, path, name, kind?}` | `Asset` | Copies a resource; unsafe to retry automatically |
| `assets/get` | `{id}` | `Asset`, including assets outside the current list page | Safe |
| `assets/update` | `{id, name?, kind?}` | Updated `Asset`; `CONCEPT_SHEET` clears single-subject association | Reapply same fields |
| `assets/delete` | `{id}` | `Deletion`; also hides dependent clips and unpins references | Repeated deletion returns `NOT_FOUND` |
| `assets/restore` | `{id}` | Restored `Asset` and dependent clips; references stay unpinned | Safe |
| `assets/list` | Pagination + optional `projectId` | `Page<Asset>` | Safe |
| `assets/export` | `{id, path}` | `{assetId, path}` | Writes a new file; an existing target causes `EXPORT_ERROR` |
| `animations/presets/list` | `{}` | Motion enum values | Safe |
| `animations/create` | `CreateAnimation` | `Animation` immediately | Idempotent per project and generation key |
| `animations/get` | `{id}` | `Animation` with current job status | Safe |
| `animations/delete` | `{id}` | `Deletion`; source image preserved | Repeated deletion returns `NOT_FOUND` |
| `animations/restore` | `{id}` | Restored `Animation` | Safe |
| `animations/list` | Pagination + optional `projectId` | `Page<Animation>` | Safe |
| `animations/setup` | `SetupAnimation` | Completed `Animation` | Idempotent per project and setup key |
| `animations/timing/update` | `{id, fps, isLooping}` | Updated `Animation` | Same timing values can be reapplied |
| `animations/export` | `{id, path}` | `{animationId, path}` | Creates a ZIP; existing target fails |
| `jobs/create` | `GenerateInput` | `Job` immediately | Idempotent per project and key |
| `jobs/get` | `{id}` | `Job` | Safe |
| `jobs/list` | Pagination + optional `projectId` | `Page<Job>` | Safe |
| `jobs/cancel` | `{id}` | Current `Job`; terminal state arrives later | Safe |
| `jobs/batch/create` | `GenerateBatchInput` | `GenerationBatch` with separate job IDs | Idempotent per project and batch key |
| `jobs/batch/get` | `{id}` | `GenerationBatch` receipt | Safe |

Pagination: `page` defaults to 1; `pageSize` defaults to 50 and accepts 1–100. Lists use newest-first order. Deletion is recoverable: records and files remain local but are hidden from normal reads and lists, including global lists of a deleted game’s contents. Deletion returns `{id, kind, projectId, name}`. A running guide or image job in the game returns `PROJECT_BUSY` before any deletion. Restore the game before restoring individual assets (`PROJECT_DELETED`). Paths for import and export belong to the local machine running the API. Export fails instead of overwriting an existing file.

## Resource types

### Separate production assets

`GenerateBatchInput`: `{projectId, idempotencyKey, items: [{characterId, prompt, width?, height?}], referenceAssetIds?}`. A batch has 2–12 distinct saved subjects in the same game. The backend derives each output kind from that subject: characters use `CHARACTER`, structures/props use `PROP`, environments use `SCENE`. Style defaults choose omitted dimensions. Characters, structures and props request real transparency; scenes request a complete environment. All jobs share the saved style and supplied references but have their own subject snapshots, asset IDs, names, PNG files and cancellation state.

`GenerationBatch`: `{id, projectId, jobIds, createdAt}`. The receipt, replay hash and jobs commit atomically before rendering starts. Invalid subjects or references leave no partial batch. Reusing the key with different items, order, dimensions or references returns `IDEMPOTENCY_CONFLICT`; exact replay returns the same job IDs without starting workers again, including after restart. Jobs render serially. Read their status with `jobs/get`; a failed item does not prevent remaining items from completing. Uncertain interrupted jobs are not automatically retried.

The one-shot `call jobs/batch/create` waits for every job and returns `{batch, jobs}`; inspect each job's status for partial failures. The persistent stdio method returns the receipt immediately. The first successful, isolated image is pinned to its matching subject if that subject has no references and its text identity has not changed during rendering. Existing pins are preserved. This gives future revisions and animations the subject's own pixels.

`CONCEPT_SHEET` is a visual reference board, not an individual character or scene. Generating one requires `characterId: null`. Reclassifying a legacy board with `assets/update` keeps its pixels and clears its incorrect single-subject association. Sprite sheets remain `SPRITE_SHEET`: their cells show frames of one identity, not a collection of different objects.

The canonical schemas are the shared [Rust types](../crates/forge-core/src/contract.rs). Every field without `?` is present unless the example states a default.

### StyleGuide

```json
{
  "presetId": "woodland",
  "name": "Woodland ink",
  "description": "Hand-painted 2D, rounded silhouettes, thin dark outlines.",
  "palette": ["#315C4B", "#D7AD70", "#F1E9D5"],
  "perspective": "Side view, orthographic",
  "lighting": "Soft light from upper left",
  "referenceAssetIds": []
}
```

`name` and `description` are required. `presetId` is optional and defaults to null; it preserves a starter preset’s output defaults while the developer changes its name, palette or visual rules. It must be one of the catalog IDs when present. Legacy style records without this field remain supported. Other fields default to empty values. The palette accepts up to 16 hex colors. Each reference list accepts up to 8 images. The combined, deduplicated references for a generation must not exceed 8. References must belong to the same project.

`CreateProject`: `{name, style}`. Creation requires an empty style reference list; import images after creating the project. `Project`: `{id, name, style, createdAt}`. Times are Unix seconds.

`CreateCharacter`: `{projectId, name, description, kind?, referenceAssetIds?}`. `Character`: `{id, projectId, name, description, kind, referenceAssetIds}`. `kind` is `CHARACTER`, `STRUCTURE`, `PROP` or `SCENE` and defaults to `CHARACTER` for older records and omitted create inputs. The existing `characters/*` routes and generation `characterId` field also address structures and props, so all reusable identities share pinned references and immutable generation snapshots. Structure images use the static `PROP` output kind; their saved subject retains `STRUCTURE` identity.

Saving a subject establishes its text identity; pinning actual images gives stronger visual continuity. `characters/update` changes only non-null supplied identity/category fields, rejects an empty update, and preserves the project and pinned references. The guide reads these categories in workspace context and can use `create_subject` and `update_subject` to save towers, buildings, items and characters without rendering an image. Its `create_game` tool creates and selects a separate project for an explicit new-game request; it leaves the previous game intact and copies only that message's attached images into the new game. `choose_style` changes the current project rather than creating a separate game.

`Asset`: `{id, projectId, jobId, characterId, kind, name, path, width, height, hasAlpha, createdAt}`. `jobId` and `characterId` can be null. Asset pixels are immutable, workspace-owned PNG files; name and kind metadata can be updated. `hasAlpha` reports actual translucent pixels rather than a format capability. Import accepts valid PNG, JPEG or WebP under 50 MB and at most 8192 pixels per side, then converts to PNG.

### GenerateInput and Job

```json
{
  "projectId": "opaque-project-id",
  "idempotencyKey": "mira-run-001",
  "prompt": "Mira running right. Keep the same costume and face.",
  "kind": "CHARACTER",
  "characterId": "opaque-character-id",
  "referenceAssetIds": [],
  "width": 512,
  "height": 512,
  "transparentBackground": true
}
```

Required: `projectId`, `idempotencyKey`, `prompt`. Defaults: kind `CHARACTER`, dimensions 1024 × 1024, no character or extra references, opaque background. Other kinds: `SCENE`, `PROP`, `SPRITE_SHEET`, `CONCEPT_SHEET`. Dimensions accept 64–4096. Prompts accept 1–8000 characters. Keys accept 1–128 characters.

Generate the key once per intent, then reuse the same input and key on retries. A retry returns the same job even while it is running. A changed input with the same key returns `IDEMPOTENCY_CONFLICT`. The payload hash includes explicit request fields after default normalization. Keys remain stored for the life of the workspace. A new key means a new generation that can consume subscription capacity.

`Job`: `{id, projectId, status, request, styleSnapshot, characterSnapshot, referenceAssetIds, threadId, turnId, assetIds, error, createdAt}`. `characterSnapshot`, `threadId`, `turnId` and `error` can be null. Saved snapshots do not change when a project is edited. Thread/turn IDs are diagnostic metadata, not public controls for managing Codex directly.

Statuses:

| Status | Meaning |
| --- | --- |
| `QUEUED` | Intent persisted; waiting for the single generation slot |
| `RUNNING` | Preparing or rendering through Codex |
| `SUCCEEDED` | Codex completed and image pixels were validated and saved |
| `FAILED` | Confirmed failure; see `error` |
| `CANCELLED` | User cancellation requested; work already performed may still count toward usage |
| `UNKNOWN` | Process exit, lost connection, timeout or unusable protocol response; inference may have applied |

No job is automatically retried. Reopening the workspace turns unfinished jobs into `UNKNOWN`, without losing their original intent. Assets can be present on a failed job when an image was preserved but did not satisfy requested transparency.

### Sprite animations

`AnimationConfig`: `{name, motion?, frameCount?, columns?, frameWidth?, frameHeight?, fps?, isLooping?, margin?, spacing?}`. Name is required. Defaults: `IDLE`, six frames, three columns, 256 × 256 cells, 8 FPS, looping, zero margin and spacing. Motions: `IDLE`, `WALK`, `RUN`, `JUMP`, `ATTACK`, `CUSTOM`.

Bounds: 2–16 frames, 1–8 columns (no more than frame count), cell dimensions 16–1024, 1–60 FPS, margin 0–128, spacing 0–64. The complete atlas must fit within 4096 × 4096. Frames are row-major; unused trailing cells are ignored. Rows are `ceil(frameCount / columns)`. Margin is the outer left/top offset; spacing is the gutter between cells. Imported sheets can contain extra unused pixels to the right or bottom.

`CreateAnimation`: `{projectId, characterId, idempotencyKey, config, prompt?, referenceAssetIds?}`. Requires a saved subject from the project. Generates one transparent atlas through the shared queue, with immutable style/subject snapshots and their pinned references. Margin and spacing must be zero for generation. Width/height derive from the grid. The provider's source image is sliced into equal cells; each cell is resized proportionally to its target cell, preserving frame order and transparent padding. Empty frames fail with `EMPTY_ANIMATION_FRAME`, preserving the source atlas for grid setup. A valid opaque source is also preserved when transparency fails.

`Animation`: `{id, projectId, characterId, jobId, sourceAssetId, config, status, frames, previewPath, error, createdAt}`. Optional IDs and preview/error may be null. Each frame contains `{index, path, rect: {x,y,w,h}}`. Generated clip ID equals its job ID; `jobs/cancel` cancels its generation. Status and error follow the job, including `UNKNOWN` recovery. Generation retries replay one job; timing edits change the clip, leaving the original generation snapshot intact.

`SetupAnimation`: `{projectId, assetId, characterId?, idempotencyKey, config}`. Extracts a grid from an existing same-project image, validates cell bounds and nonempty frames, and creates a completed clip with `jobId: null`. It never generates new images. Identical retries return the existing clip's current state; changed payloads conflict; uncertain pending claims yield `OUTCOME_UNKNOWN`.

`animations/export` creates a ZIP containing `atlas.png`, `frames/frame-000.png` etc., `preview.gif`, and `animation.json`. JSON uses Aseprite-style frame rectangles, source sizes, durations in milliseconds, frame tags, FPS and loop metadata. Coordinates are in atlas pixels; default pivot is normalized `(0.5, 1.0)`. These are portable files rather than an engine-specific importer. PNGs carry full RGBA; GIF uses a limited palette and centisecond timing. The native preview uses PNG frames at the saved FPS.

Forge chooses cell defaults from the style: pixel 128, painterly 512, other styles 256. Six frames/three columns work well with landscape image generation. Run defaults to 12 FPS; Jump and Attack play once. Developers request changes in chat; the native interface displays the resulting library, world catalog and animation playback, and provides downloads. Direct API callers receive the documented config defaults regardless of project style. `animate --request` and `call animations/create` wait for completion; `serve` returns immediately.

Motion continuity and character alignment are visual quality checks, not guaranteed by successful file extraction. Review playback before use in an engine.

### AI guide

`StylePreset`: `{id, title, subtitle, style, characterSize, sceneWidth, sceneHeight}`. The preset catalog is finite and unpaginated. IDs: `woodland`, `pixel`, `flat`, `ink`, `paint`, `isometric`.

`AssistantInput`:

```json
{
  "requestId": "setup-game-001",
  "sessionId": null,
  "projectId": null,
  "message": "Set up a cozy forest RPG and save its first scout character.",
  "allowGeneration": false,
  "referenceAssetIds": []
}
```

Required: `requestId`, `message`. The optional `sessionId` continues saved conversation. With no session, optional `projectId` scopes a new session; with neither, the guide can create a project. A caller cannot rebind an existing session by supplying a different `projectId`. Forge's `create_game` tool can create a separate game and move the conversation to it when the user explicitly asks for a new game. `allowGeneration` defaults to false and permits at most one generation request in this message when true: one image or one batch of 2–12 separately requested subjects. The guide must also interpret an explicit request to make an image; a permission flag alone is not an instruction to render. The server enforces the hard limit.

`referenceAssetIds` defaults to an empty array. The native **Add to chat** action supplies image IDs here, including the source atlas for an animation. For example:

```json
{
  "requestId": "revise-tower-001",
  "sessionId": "opaque-session-id",
  "projectId": "opaque-project-id",
  "message": "Give this tower a red roof and preserve its silhouette.",
  "allowGeneration": true,
  "referenceAssetIds": ["opaque-tower-asset-id"]
}
```

Attachments must exist in the session's current project. A foreign-project image returns `VALIDATION_ERROR`; an unknown asset returns `NOT_FOUND`. A message accepts at most eight supplied image IDs. The deduplicated combination of saved style references and message attachments must also fit within eight images before the message is accepted. At generation, the combined style, subject, message and tool-supplied reference IDs are deduplicated again and must total at most eight. Invalid counts or combined limits return `VALIDATION_ERROR` rather than silently dropping references.

Forge receives the actual attached image pixels, metadata in `attachedReferences`, and matching clip metadata in `attachedAnimations`, including attachments outside the current list page. Its asset and animation generation tools automatically inherit the current message's attachments; additional explicit references merge without duplicate images. Adding an attachment alone does not pin it to the saved style or subject. It applies to this message only and is replaced by the next accepted message's list. Earlier conversation attachments are not automatically reused.

`AssistantSession`: `{id, projectId, status, messages, threadId, turnId, allowGeneration, referenceAssetIds, generatedJobIds, turnJobCount, pendingQuestion, setupApproved, error, createdAt}`. Project and diagnostic thread/turn IDs can be null. `referenceAssetIds` contains the latest accepted message's deduplicated attachments, including after that turn finishes. Messages contain `{role: "USER" | "ASSISTANT", text, referenceAssetIds?}`; user messages preserve their attachment IDs and empty lists may be omitted. Older sessions and messages without these fields load with empty references. Sessions keep up to 128 messages; the latest ten contextual messages plus current project data are supplied to each fresh ephemeral guide thread.

`pendingQuestion` is null or `{id, prompt, options: [{id, label}], forNewGame}`. The guide’s `ask_question` tool accepts 2–6 distinct choices. It finishes the turn with status `READY` and a pending question; no mutation is allowed until the user responds. New-game creation requires an answered or explicitly skipped setup question. `setupApproved` is an internal eligibility marker, reset after creating the game or accepting a fresh request that does not answer a pending question.

Answer through `assistant/message` with the same `sessionId`, a new `requestId`, a nonempty `message`, and `questionAnswer: {questionId, optionId?, skipped?}`. For a pill use its `optionId`; for Skip set `skipped: true` and omit `optionId`; for a typed answer omit `optionId` and leave `skipped` false. The server uses the stored option label rather than trusting caller-supplied text for a pill. Stale IDs return `QUESTION_STALE`, unknown options return `VALIDATION_ERROR`, and omitting an answer while a question is pending returns `QUESTION_PENDING`. Answering preserves the original task’s image references and remaining generation allowance, merges newly attached images, and does not grant an extra image job. Request replay still uses the exact original input. Old inputs omit `questionAnswer` and retain their hashes.

Statuses: `THINKING`, `READY`, `FAILED`, `UNKNOWN`. Cancellation ends as `FAILED` with `CANCELLED`; completed app changes and independently queued image jobs remain. Cancel image jobs separately with `jobs/cancel`. Reopening an interrupted workspace marks thinking sessions `UNKNOWN` and never repeats actions.

Forge owns the creation and editing actions in the native app. Its tools ask structured questions, rename the current game, read current context, create a separate game, apply a preset, customize the current style’s name/direction/palette/camera/lighting, create or edit saved subjects, pin a same-project reference, queue an image or sprite animation, extract an existing sheet, and change clip timing. `create_subject` saves `CHARACTER`, `STRUCTURE`, `PROP` or `SCENE` identities; `update_subject` edits an existing identity or category while preserving its references. The legacy `create_character` tool remains available for character identities. Static images, animation generation and separate-subject batches share one generation-request allowance. A batch consumes that allowance and queues one job per requested subject; `turnJobCount` records the number of jobs in that turn. Arbitrary API methods, file export, external integrations and unrelated projects are outside its tools. Style customization applies only the supplied fields, preserves pinned references and the starter preset, and rejects an empty change. Forge chooses static output dimensions from the preset: pixel sprites default to 256 × 256, painterly sprites to 1024 × 1024, and other sprites to 512 × 512. Explicit dimensions override defaults. The low-level `jobs/create` method retains its documented 1024 × 1024 defaults when dimensions are omitted.

The guide-only `create_game` tool leaves the previous game intact and copies only current-message attachments into the new project. Its result retains the `Project` fields and adds `attachedReferences` with the copied asset records and `referenceAssetIdMap`, an object mapping each old asset ID to its new copy's ID. Subsequent tools must use the new IDs. The session and current user message are updated to those IDs; generation automatically inherits the copied attachments. Historical style references and subjects remain in the original project. Replaying the same tool call returns the same project, copied IDs and mapping.

Mutating tool calls have an atomic, payload-checked effect ledger keyed by session, turn and call ID. Repeated calls replay the stored result. A crash after claiming an effect yields `OUTCOME_UNKNOWN` instead of repeating it.

Message `requestId` values remain in the workspace permanently. Same key and input replay the original **acceptance response**, which can still show `THINKING`; fetch `assistant/get` for current state. Changed input, including a changed attachment list, returns `IDEMPOTENCY_CONFLICT`. An omitted or empty `referenceAssetIds` list preserves the legacy request hash; nonempty lists are part of the payload, so preserve their order and contents on retries. An in-flight ledger claim with no recorded acceptance yields `OUTCOME_UNKNOWN`; a separate message on a thinking session yields `ASSISTANT_BUSY`. Each message permits at most twelve tool calls. Notifications stream progress; they do not replace fetching current resources.

The one-shot CLI waits for the guide and every image started by its current message. The persistent `serve` transport returns immediately. CLI termination during a request still has an uncertain outcome; use the same request key to recover, then inspect saved state.

### AccountStatus

`{isLoggedIn, email, plan, canGenerateImages, message}`. `email`, `plan`, and `message` can be null. `isLoggedIn` specifically indicates a Codex-managed ChatGPT account. Asset Forge does not accept API keys or external access tokens. Capability information is provider-reported; a completed generation is the actual access verification.

## Notifications

Notifications have no `id` and can arrive between replies:

```json
{"method":"events/notification","params":{"kind":"IMAGE_GENERATING","jobId":"opaque-job-id","message":"Rendering your game asset…"}}
```

Kinds: `ASSISTANT_THINKING`, `ASSISTANT_DELTA`, `ASSISTANT_ACTION`, `ASSISTANT_FINISHED`, `JOB_RUNNING`, `IMAGE_GENERATING`, `JOB_FINISHED`, `ACCOUNT_CONNECTED`, `LOGIN_FAILED`, `RESYNC_REQUIRED`. `jobId` is null for account and guide events. Guide notifications have a `sessionId`; it is null on other events. `ASSISTANT_DELTA` messages are text fragments; `ASSISTANT_ACTION` messages are successful action receipts. Fetch `assistant/get` after action or completion, then refresh the session’s project, characters, jobs and assets. Progress is descriptive, not a percentage. Notifications are advisory: refresh `jobs/get` and `assets/list` after completion or `RESYNC_REQUIRED`. Persist the returned job ID before waiting. Clients can also poll `jobs/get` for recovery.

## Error codes

Common codes include `VALIDATION_ERROR`, `NOT_FOUND`, `METHOD_NOT_FOUND`, `WORKSPACE_BUSY`, `STORAGE_ERROR`, `IDEMPOTENCY_CONFLICT`, `CODEX_NOT_FOUND`, `CODEX_CONFIG_ERROR`, `ASSISTANT_BUSY`, `ASSISTANT_FAILED`, `QUESTION_PENDING`, `QUESTION_STALE`, `SETUP_QUESTION_REQUIRED`, `PROJECT_BUSY`, `PROJECT_DELETED`, `ACTION_DENIED`, `ACTION_LIMIT`, `GENERATION_NOT_AUTHORIZED`, `AUTH_REQUIRED`, `IMAGE_GENERATION_UNAVAILABLE`, `CODEX_ERROR`, `CODEX_TIMEOUT`, `CODEX_DISCONNECTED`, `PROTOCOL_ERROR`, `GENERATION_FAILED`, `IMAGE_GENERATION_FAILED`, `NO_IMAGE_GENERATED`, `INVALID_IMAGE`, `EMPTY_ANIMATION_FRAME`, `TRANSPARENCY_UNAVAILABLE`, `GENERATION_TIMEOUT`, `EVENTS_LOST`, `OUTCOME_UNKNOWN`, `CANCELLED` and `EXPORT_ERROR`. New error codes may be added; clients should display unknown codes with their message.
