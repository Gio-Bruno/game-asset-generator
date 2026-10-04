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
| `system/update/ready` | `{}` | `{isReady: true}` or `WORKSPACE_BUSY` if any game has active jobs or Forge sessions | Safe; read-only readiness check |
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
| `library/subjects/list` | Pagination + optional `projectId` | `Page<LibrarySubject>`: saved identity, total image/animation counts, optional preview asset | Safe |
| `library/subjects/export` | `ExportCollectionInput`: `{id, path}`; id is a saved subject | `CollectionExport`; one portable ZIP of the whole visible subject library | Creates a new file; existing target fails; no generation |
| `characters/update` | `{id, name?, description?, kind?}` | `Character` | Applies only supplied fields; retry the same intended changes |
| `characters/references/update` | `{id, referenceAssetIds}` | `Character` | Replaces references; retry only the same intended replacement |
| `assets/import` | `{projectId, path, name, kind?}` | `Asset` | Copies a resource; unsafe to retry automatically |
| `assets/get` | `{id}` | `Asset`, including assets outside the current list page | Safe |
| `assets/update` | `{id, name?, kind?}` | Updated `Asset`; `CONCEPT_SHEET` clears single-subject association | Reapply same fields |
| `assets/delete` | `{id}` | `Deletion`; also hides dependent clips and unpins references | Repeated deletion returns `NOT_FOUND` |
| `assets/restore` | `{id}` | Restored `Asset` and dependent clips; references stay unpinned | Safe |
| `assets/list` | Pagination + optional `projectId`, `characterId` or `isUnassigned` | `Page<Asset>` | Safe |
| `assets/export` | `{id, path}` | `{assetId, path}` | Writes a new file; an existing target causes `EXPORT_ERROR` |
| `animations/presets/list` | `{}` | Motion enum values | Safe |
| `animations/directions/list` | `{}` | `N, NE, E, SE, S, SW, W, NW` | Safe |
| `animations/sets/create` | `CreateAnimationSet` | Complete `AnimationSet` manifest and new job IDs | Idempotent per project and set key; fills missing coverage |
| `animations/sets/get` | `{id}` | Saved `AnimationSet` | Safe |
| `animations/sets/list` | Media pagination/filters | `Page<AnimationSet>` | Safe |
| `animations/sets/export` | `ExportCollectionInput`: `{id, path}`; id is a saved set | `CollectionExport`; one ZIP of exactly that set's clips, atlases and subject references | Creates a new file; existing target fails; no generation |
| `animations/create` | `CreateAnimation` | `Animation` immediately | Idempotent per project and generation key |
| `animations/get` | `{id}` | `Animation` with current job status | Safe |
| `animations/delete` | `{id}` | `Deletion`; source image preserved | Repeated deletion returns `NOT_FOUND` |
| `animations/restore` | `{id}` | Restored `Animation` | Safe |
| `animations/list` | Pagination + optional `projectId`, `characterId` or `isUnassigned` | `Page<Animation>` | Safe |
| `animations/setup` | `SetupAnimation` | Completed `Animation` | Idempotent per project and setup key |
| `animations/timing/update` | `{id, fps, isLooping}` | Updated `Animation` | Same timing values can be reapplied |
| `animations/align` | `{id, idempotencyKey}` | New aligned `Animation` and atlas | Idempotent; original preserved; no generation |
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

`AnimationConfig`: `{name, motion?, direction?, frameCount?, columns?, frameWidth?, frameHeight?, fps?, isLooping?, margin?, spacing?}`. Name is required. Defaults: `IDLE`, six frames, three columns, 256 × 256 cells, 8 FPS, looping, zero margin and spacing. Motions: `IDLE`, `WALK`, `RUN`, `JUMP`, `ATTACK`, `HIT_REACTION`, `DEATH`, `CUSTOM`. Optional `direction` uses the eight compass values above relative to the screen; the saved camera/perspective is preserved. Legacy clips omit direction, and no facing is inferred from their names. Export metadata includes `meta.facingDirection` separately from the frame tag's playback direction.

`CreateAnimationSet`: `{projectId, characterId, idempotencyKey, motions, directions, prompt?, referenceAssetIds?, frameCount?, frameSize?, fps?}`. Use unique arrays of 1–7 preset motions (excluding CUSTOM) and 1–8 facings. The backend expands the complete cross product, creating a separate named atlas/clip for every missing cell, up to 56. It atomically saves clips, jobs, identity/style snapshots and the replay receipt before launching the shared rendering queue. Ready or in-flight clips with explicit matching motion/direction are reused, retaining their existing settings; overrides apply to newly rendered cells. Failed, cancelled, unknown, deleted or legacy unspecified clips cannot satisfy coverage. This operation fills a set; use `animations/create` to deliberately revise an existing motion/facing.

`AnimationSet`: `{id, projectId, characterId, entries, jobIds, createdAt}`. Each entry is `{motion, direction, animationId, isReused}`. `jobIds` contains the new generation jobs belonging to this intent; entries also link reused clips. Query `animations/get` for current status and error. The manifest records queued coverage, not a promise that every render succeeded. Identical retries return the same receipt without rerendering; changed payloads sharing a key return `IDEMPOTENCY_CONFLICT`. No automatic retry occurs after interruption or a failed cell. `call animations/sets/create` waits for all newly owned jobs and reports each outcome; `serve` returns immediately. Cancel individual jobs with `jobs/cancel`.

Bounds: 2–16 frames, 1–8 columns (no more than frame count), cell dimensions 16–1024, 1–60 FPS, margin 0–128, spacing 0–64. The complete atlas must fit within 4096 × 4096. Frames are row-major; unused trailing cells are ignored. Rows are `ceil(frameCount / columns)`. Margin is the outer left/top offset; spacing is the gutter between cells. Imported sheets can contain extra unused pixels to the right or bottom.

`CreateAnimation`: `{projectId, characterId, idempotencyKey, config, prompt?, referenceAssetIds?}`. Requires a saved subject from the project. Generates one transparent atlas through the shared queue, with immutable style/subject snapshots and their pinned references. Margin and spacing must be zero for generation. Width/height derive from the grid. The provider's source image is sliced into equal cells; each cell is resized proportionally to its target cell, preserving frame order and transparent padding. Empty frames fail with `EMPTY_ANIMATION_FRAME`. A visible silhouette touching a generated cell boundary fails with `CLIPPED_ANIMATION_FRAME`; this is a padding check, not a guarantee of motion quality. Source images are preserved for deliberate grid setup, including when transparency fails. Imported sheets through `animations/setup` retain their explicitly chosen grid and can intentionally reach cell edges.

`Animation`: `{id, projectId, characterId, jobId, sourceAssetId, config, status, frames, previewPath, error, createdAt}`. Optional IDs and preview/error may be null. Each frame contains `{index, path, rect: {x,y,w,h}}`. Generated clip ID equals its job ID; `jobs/cancel` cancels its generation. Status and error follow the job, including `UNKNOWN` recovery. Generation retries replay one job; timing edits change the clip, leaving the original generation snapshot intact.

`SetupAnimation`: `{projectId, assetId, characterId?, idempotencyKey, config}`. Extracts a grid from an existing same-project image, validates cell bounds and nonempty frames, and creates a completed clip with `jobId: null`. It never generates new images. Identical retries return the existing clip's current state; changed payloads conflict; uncertain pending claims yield `OUTCOME_UNKNOWN`.

Forge chat's `setup_animation` uses the actual source grid of a sprite sheet. It rejects explicit concept boards and generated static assets; imported sheets remain supported even when their default metadata says `CHARACTER`, so the guide must inspect their poses. It rejects a grid that leaves an entire cell's width or height unused with `ANIMATION_GRID_MISMATCH`, preventing generation defaults from cropping a larger atlas into fragments. Small trailing pixels remain supported. Direct `animations/setup` callers can deliberately select a subregion. Frame extraction preserves existing poses; a walk sheet cannot become an idle cycle without generating new motion poses.

Generated `IDLE` sheets align their planted feet/base after normalization: the bottom tenth of the visible silhouette defines the horizontal anchor, and its bottom defines the baseline. Frames translate to the median anchors without scaling or redrawing pixels. Other motions retain their original positions, including a jump's vertical arc. This is intended for standing idles; hovering or translating motion should use `CUSTOM`.

`animations/align` applies that same alignment to an existing completed `IDLE` clip, creating a new immutable atlas and clip while preserving the original. It requires an exact complete source grid with zero margin/spacing. It refuses cut art and fails with `ANIMATION_ALIGNMENT_UNAVAILABLE` if translation would clip visible pixels. The new atlas, frame PNGs, GIF and exported rectangles agree. Forge chat exposes this as `align_animation`; no image generation or subscription capacity is used. Alignment fixes translation drift, not inconsistent poses or scale.

`animations/export` creates a ZIP containing `atlas.png`, `frames/frame-000.png` etc., `preview.gif`, and `animation.json`. JSON uses Aseprite-style frame rectangles, source sizes, durations in milliseconds, frame tags, FPS and loop metadata. Coordinates are in atlas pixels; default pivot is normalized `(0.5, 1.0)`. These are portable files rather than an engine-specific importer. PNGs carry full RGBA; GIF uses a limited palette and centisecond timing. The native preview uses PNG frames at the saved FPS.

Collection downloads use the same clip format in a single archive. **Download all** on a subject exports every active owned image and clip across the whole library, plus linked subject references. It ignores the current filter and page. `animations/sets/export` restricts clips to the saved set's entries and includes their atlases and the subject's linked references. Neither operation generates, edits or deletes artwork. Export runs locally and requires no account connection.

```text
manifest.json
README.txt
images/<safe-name>-<id-hash>.png
references/<safe-name>-<id-hash>.png
animations/<motion>/<facing>/<safe-name>-<id-hash>/
  atlas.png
  frames/frame-000.png ...
  preview.gif
  animation.json
```

Motion folders use lowercase enum names (`idle`, `walk`, `hit_reaction`, etc.); facing folders use `N`, `NE`, `E`, `SE`, `S`, `SW`, `W`, `NW`, or `unspecified` for legacy clips. No facing is inferred from a name. Safe ASCII filenames with stable ID hashes keep same-name revisions distinct and avoid path traversal or Windows-invalid characters. Atlases already included in a clip are mapped to that path instead of duplicated under images. Each clip's JSON paths remain relative to its own folder, so it can also be moved or imported on its own.

`CollectionExport`: `{path, characterId, animationSetId, imageCount, animationCount, skippedAnimationCount, isComplete}`. Counts describe distinct images (including references and atlases) and exported clips. `animationSetId` is null for a whole-subject download. `manifest.json` schema version 1 includes `{schemaVersion, exportedAt, subject, project, animationSetId, images, animations, animationSets, isComplete}`. Project/subject metadata includes the saved names, identity and art direction; source filesystem paths and chat are excluded. Image entries have `{id, name, kind, width, height, hasAlpha, isReference, path}`. Animation entries have `{id, name, motion, facingDirection, status, config, path, skipReason}`; path points to the clip folder. All archive paths are relative.

Only `SUCCEEDED` clips with all configured frames and an atlas are written. Other entries have null paths and an explicit skip reason (`QUEUED`, `RUNNING`, `FAILED`, `CANCELLED`, `UNKNOWN`, `INCOMPLETE_FRAMES_OR_ATLAS`, or `DELETED_OR_MISSING` for a removed set cell). A removed cell also has null status/config; other entries include current authoritative job status and saved playback config. A partial pack returns success with `isComplete:false`; the GUI reports the unfinished count. Download again to a new filename once generation finishes. For whole-subject exports, completeness covers currently active clips; `animationSets` records earlier requested coverage, which can still link removed clips. A set download evaluates every requested cell. When no files are ready, the API returns `VALIDATION_ERROR` without creating an archive. Missing/unreadable source files abort the operation and remove a partial archive; existing destinations remain untouched.

To use a pack, unzip once, then import numbered frame PNGs in order or use each atlas and its JSON rectangles, durations, FPS and loop flag. `README.txt` includes those steps. No nested clip ZIPs or engine-specific setup is required to access the files; configuring playback in a particular engine remains the developer's task.

Forge chooses cell defaults from the style: pixel 128, painterly 512, other styles 256. Six frames/three columns work well with landscape image generation. Run defaults to 12 FPS; Jump, Attack, Hit reaction and Death play once. Developers request changes in chat; the native interface displays the resulting library, world catalog and animation playback, and provides downloads. Direct API callers receive the documented config defaults regardless of project style. `animate --request` and `call animations/create` wait for completion; `serve` returns immediately.

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

Required: `requestId`, `message`. The optional `sessionId` continues saved conversation. With no session, optional `projectId` scopes a new session; with neither, the guide can create a project. A caller cannot rebind an existing session by supplying a different `projectId`. Forge's `create_game` tool can create a separate game and move the conversation to it when the user explicitly asks for a new game. `allowGeneration` defaults to false and permits at most one generation request in this message when true: one image, one animation, one batch of 2–12 separately requested subjects, or one requested motion-by-direction animation set. The guide must also interpret an explicit request to make an image; a permission flag alone is not an instruction to render. The server enforces the hard limit.

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

Forge owns the creation and editing actions in the native app. Its tools ask structured questions, rename the current game, read current context, create a separate game, apply a preset, customize the current style’s name/direction/palette/camera/lighting, create or edit saved subjects, pin a same-project reference, queue an image or sprite animation, extract an existing sheet, and change clip timing. `create_subject` saves `CHARACTER`, `STRUCTURE`, `PROP` or `SCENE` identities; `update_subject` edits an existing identity or category while preserving its references. The legacy `create_character` tool remains available for character identities. Static images, animation generation, animation sets and separate-subject batches share one generation-request allowance. Broad basic-animation requests offer creative question pills for motion scope and direction coverage, with Skip using the stated five-core-motion/eight-direction default (40 cells). Explicitly supplied scope is executed directly. generate_animation_set queues the whole cross product as one intent; it never asks for a separate chat message per clip. A batch consumes that allowance and queues one job per requested subject; `turnJobCount` records the number of jobs in that turn. Arbitrary API methods, file export, external integrations and unrelated projects are outside its tools. Style customization applies only the supplied fields, preserves pinned references and the starter preset, and rejects an empty change. Forge chooses static output dimensions from the preset: pixel sprites default to 256 × 256, painterly sprites to 1024 × 1024, and other sprites to 512 × 512. Explicit dimensions override defaults. The low-level `jobs/create` method retains its documented 1024 × 1024 defaults when dimensions are omitted.

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

## Browsing a subject library

`library/subjects/list` returns one entry per saved identity, including characters, structures, props and scenes. Each `LibrarySubject` contains `subject: Character`, `imageCount`, `animationCount` and `preview: Asset | null`. Counts cover all active owned media, independent of pagination. A preview prefers a saved identity reference or original image over an animation atlas. Different identities with the same name remain separate. Game-level references can be linked without becoming owned media.

For `assets/list` and `animations/list`, `characterId` narrows to one identity and is validated against `projectId` when supplied. `isUnassigned: true` selects only game-level media with no identity. These filters are mutually exclusive. Omitting both retains the previous project-wide listing. Filtering occurs before pagination; metadata counts only matching visible records, and deleted games/media are excluded.
