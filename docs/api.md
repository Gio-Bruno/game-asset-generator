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
| `projects/style/update` | `{projectId, style: StyleGuide}` | `Project` | Replaces style; retry only the same intended replacement |
| `characters/create` | `CreateCharacter` | `Character` | Creates a resource; unsafe to retry automatically |
| `characters/list` | Pagination + optional `projectId` | `Page<Character>` | Safe |
| `characters/references/update` | `{id, referenceAssetIds}` | `Character` | Replaces references; retry only the same intended replacement |
| `assets/import` | `{projectId, path, name, kind?}` | `Asset` | Copies a resource; unsafe to retry automatically |
| `assets/list` | Pagination + optional `projectId` | `Page<Asset>` | Safe |
| `assets/export` | `{id, path}` | `{assetId, path}` | Writes a new file; an existing target causes `EXPORT_ERROR` |
| `animations/presets/list` | `{}` | Motion enum values | Safe |
| `animations/create` | `CreateAnimation` | `Animation` immediately | Idempotent per project and generation key |
| `animations/get` | `{id}` | `Animation` with current job status | Safe |
| `animations/list` | Pagination + optional `projectId` | `Page<Animation>` | Safe |
| `animations/setup` | `SetupAnimation` | Completed `Animation` | Idempotent per project and setup key |
| `animations/timing/update` | `{id, fps, isLooping}` | Updated `Animation` | Same timing values can be reapplied |
| `animations/export` | `{id, path}` | `{animationId, path}` | Creates a ZIP; existing target fails |
| `jobs/create` | `GenerateInput` | `Job` immediately | Idempotent per project and key |
| `jobs/get` | `{id}` | `Job` | Safe |
| `jobs/list` | Pagination + optional `projectId` | `Page<Job>` | Safe |
| `jobs/cancel` | `{id}` | Current `Job`; terminal state arrives later | Safe |

Pagination: `page` defaults to 1; `pageSize` defaults to 50 and accepts 1–100. Lists use newest-first order. There are no deletion methods in v1. Paths for import and export belong to the local machine running the API. Export fails instead of overwriting an existing file.

## Resource types

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

`CreateCharacter`: `{projectId, name, description, referenceAssetIds?}`. `Character`: `{id, projectId, name, description, referenceAssetIds}`. Saving a character alone establishes its text identity; pinning actual images gives stronger visual continuity.

`Asset`: `{id, projectId, jobId, characterId, kind, name, path, width, height, hasAlpha, createdAt}`. `jobId` and `characterId` can be null. Assets are immutable, workspace-owned PNG files. `hasAlpha` reports actual translucent pixels rather than a format capability. Import accepts valid PNG, JPEG or WebP under 50 MB and at most 8192 pixels per side, then converts to PNG.

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

Required: `projectId`, `idempotencyKey`, `prompt`. Defaults: kind `CHARACTER`, dimensions 1024 × 1024, no character or extra references, opaque background. Other kinds: `SCENE`, `PROP`, `SPRITE_SHEET`. Dimensions accept 64–4096. Prompts accept 1–8000 characters. Keys accept 1–128 characters.

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

`CreateAnimation`: `{projectId, characterId, idempotencyKey, config, prompt?, referenceAssetIds?}`. Requires a saved character from the project. Generates one transparent atlas through the shared queue, with immutable style/character snapshots and their pinned references. Margin and spacing must be zero for generation. Width/height derive from the grid. The provider's source image is sliced into equal cells; each cell is resized proportionally to its target cell, preserving frame order and transparent padding. Empty frames fail with `EMPTY_ANIMATION_FRAME`, preserving the source atlas for manual setup. A valid opaque source is also preserved when transparency fails.

`Animation`: `{id, projectId, characterId, jobId, sourceAssetId, config, status, frames, previewPath, error, createdAt}`. Optional IDs and preview/error may be null. Each frame contains `{index, path, rect: {x,y,w,h}}`. Generated clip ID equals its job ID; `jobs/cancel` cancels its generation. Status and error follow the job, including `UNKNOWN` recovery. Generation retries replay one job; timing edits change the clip, leaving the original generation snapshot intact.

`SetupAnimation`: `{projectId, assetId, characterId?, idempotencyKey, config}`. Extracts a grid from an existing same-project image, validates cell bounds and nonempty frames, and creates a completed clip with `jobId: null`. It never generates new images. Identical retries return the existing clip's current state; changed payloads conflict; uncertain pending claims yield `OUTCOME_UNKNOWN`.

`animations/export` creates a ZIP containing `atlas.png`, `frames/frame-000.png` etc., `preview.gif`, and `animation.json`. JSON uses Aseprite-style frame rectangles, source sizes, durations in milliseconds, frame tags, FPS and loop metadata. Coordinates are in atlas pixels; default pivot is normalized `(0.5, 1.0)`. These are portable files rather than an engine-specific importer. PNGs carry full RGBA; GIF uses a limited palette and centisecond timing. The native preview uses PNG frames at the saved FPS.

The GUI and guide choose cell defaults from the style: pixel 128, painterly 512, other styles 256. Six frames/three columns work well with landscape image generation. Run defaults to 12 FPS; Jump and Attack play once. Direct API callers receive the documented config defaults regardless of project style. `animate --request` and `call animations/create` wait for completion; `serve` returns immediately.

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
  "allowGeneration": false
}
```

Required: `requestId`, `message`. The optional `sessionId` continues saved conversation. With no session, optional `projectId` scopes a new session; with neither, the guide can create a project. An existing session cannot be rebound to another project. `allowGeneration` defaults to false and permits at most one image job in this message when true. The guide must also interpret an explicit request to make an image; a permission flag alone is not an instruction to render. The server enforces the hard limit.

`AssistantSession`: `{id, projectId, status, messages, threadId, turnId, allowGeneration, generatedJobIds, turnJobCount, error, createdAt}`. Project and diagnostic thread/turn IDs can be null. Messages contain `{role: "USER" | "ASSISTANT", text}`. Sessions keep up to 128 messages; the latest ten contextual messages plus current project data are supplied to each fresh ephemeral guide thread.

Statuses: `THINKING`, `READY`, `FAILED`, `UNKNOWN`. Cancellation ends as `FAILED` with `CANCELLED`; completed app changes and independently queued image jobs remain. Cancel image jobs separately with `jobs/cancel`. Reopening an interrupted workspace marks thinking sessions `UNKNOWN` and never repeats actions.

The guide's app tools read current context, apply a preset, customize the current style’s name/direction/palette/camera/lighting, create a character, pin a same-project reference, queue an image or sprite animation, extract an existing sheet, and change clip timing. Static images and animation generation share the same one-job allowance. Arbitrary API methods, file export, external integrations and unrelated projects are outside its tools. Style customization applies only the supplied fields, preserves pinned references and the starter preset, and rejects an empty change. The composer and guide share the preset’s output dimensions; pixel sprites default to 256 × 256, painterly sprites to 1024 × 1024, and other sprites to 512 × 512. Explicit dimensions override defaults. The low-level `jobs/create` method retains its documented 1024 × 1024 defaults when dimensions are omitted; the GUI and guide pass their chosen preset dimensions explicitly.

Mutating tool calls have an atomic, payload-checked effect ledger keyed by session, turn and call ID. Repeated calls replay the stored result. A crash after claiming an effect yields `OUTCOME_UNKNOWN` instead of repeating it.

Message `requestId` values remain in the workspace permanently. Same key and input replay the original **acceptance response**, which can still show `THINKING`; fetch `assistant/get` for current state. Changed input returns `IDEMPOTENCY_CONFLICT`. An in-flight ledger claim with no recorded acceptance yields `OUTCOME_UNKNOWN`; a separate message on a thinking session yields `ASSISTANT_BUSY`. Each message permits at most twelve tool calls. Notifications stream progress; they do not replace fetching current resources.

The one-shot CLI waits for the guide and any image started by its current message. The persistent `serve` transport returns immediately. CLI termination during a request still has an uncertain outcome; use the same request key to recover, then inspect saved state.

### AccountStatus

`{isLoggedIn, email, plan, canGenerateImages, message}`. `email`, `plan`, and `message` can be null. `isLoggedIn` specifically indicates a Codex-managed ChatGPT account. Asset Forge does not accept API keys or external access tokens. Capability information is provider-reported; a completed generation is the actual access verification.

## Notifications

Notifications have no `id` and can arrive between replies:

```json
{"method":"events/notification","params":{"kind":"IMAGE_GENERATING","jobId":"opaque-job-id","message":"Rendering your game asset…"}}
```

Kinds: `ASSISTANT_THINKING`, `ASSISTANT_DELTA`, `ASSISTANT_ACTION`, `ASSISTANT_FINISHED`, `JOB_RUNNING`, `IMAGE_GENERATING`, `JOB_FINISHED`, `ACCOUNT_CONNECTED`, `LOGIN_FAILED`, `RESYNC_REQUIRED`. `jobId` is null for account and guide events. Guide notifications have a `sessionId`; it is null on other events. `ASSISTANT_DELTA` messages are text fragments; `ASSISTANT_ACTION` messages are successful action receipts. Fetch `assistant/get` after action or completion, then refresh the session’s project, characters, jobs and assets. Progress is descriptive, not a percentage. Notifications are advisory: refresh `jobs/get` and `assets/list` after completion or `RESYNC_REQUIRED`. Persist the returned job ID before waiting. Clients can also poll `jobs/get` for recovery.

## Error codes

Common codes include `VALIDATION_ERROR`, `NOT_FOUND`, `METHOD_NOT_FOUND`, `WORKSPACE_BUSY`, `STORAGE_ERROR`, `IDEMPOTENCY_CONFLICT`, `CODEX_NOT_FOUND`, `CODEX_CONFIG_ERROR`, `ASSISTANT_BUSY`, `ASSISTANT_FAILED`, `ACTION_DENIED`, `ACTION_LIMIT`, `GENERATION_NOT_AUTHORIZED`, `AUTH_REQUIRED`, `IMAGE_GENERATION_UNAVAILABLE`, `CODEX_ERROR`, `CODEX_TIMEOUT`, `CODEX_DISCONNECTED`, `PROTOCOL_ERROR`, `GENERATION_FAILED`, `IMAGE_GENERATION_FAILED`, `NO_IMAGE_GENERATED`, `INVALID_IMAGE`, `EMPTY_ANIMATION_FRAME`, `TRANSPARENCY_UNAVAILABLE`, `GENERATION_TIMEOUT`, `EVENTS_LOST`, `OUTCOME_UNKNOWN`, `CANCELLED` and `EXPORT_ERROR`. New error codes may be added; clients should display unknown codes with their message.
