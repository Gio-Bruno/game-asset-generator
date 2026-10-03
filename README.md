# Asset Forge

A local 2D game asset workshop for macOS and Windows, powered by your Codex subscription. Choose a visual preset or tell Forge, your AI art director, about the game. Forge can save the art direction, develop characters, and generate assets and sprite animations while keeping their identities and references consistent.

The interface is native Rust/GPUI. The CLI, stdio API and desktop app share the same typed backend. No web server, API key, Node runtime or separate paid image API is required by Asset Forge.

## Open the app

On this workspace's Mac, open `dist/Asset Forge.app`. The portable CLI is `dist/asset-forge`.

1. Install the [Codex CLI](https://learn.chatgpt.com/docs/cli) if it is not already available. Asset Forge was verified with Codex CLI **0.160.0**.
2. Open Asset Forge. It detects the existing Codex login. Otherwise choose **Connect Codex** and complete sign-in in your browser; Codex stores and refreshes the credentials.
3. Choose one of six visual directions in **Art direction**, or use a game suggestion in the Forge panel. The guide can set up the style and cast for you.
4. Tell Forge what you want to create: “Make Mira's first idle pose,” “Give her a running pose,” or “Create a matching woodland scene.” It acts in the current project and queues an image when you request one. You can also ask it to customize colors, lighting and perspective while retaining your saved references.
5. For direct control, use **Canvas**: choose a type and character, enter one brief, and generate. Dimensions and backgrounds have sensible defaults; **Output settings** exposes overrides.
6. Choose a result and pin it with **Style ref** or **Character ref**. Future generations attach those actual pixels. **+ Image reference** imports pixels directly into the next manual generation; reference chips remove them with one click. Library **+ Reference** buttons also reuse existing assets.
7. Use **Animate** for Idle, Walk, Run, Jump, Attack or custom motion. Generate a transparent clip from a saved character, preview or step through frames, adjust timing, then **Export clip**. To use existing art, import a sprite sheet and extract frames; the grid fits the current preset automatically. Clip settings expose rectangular cells, margin and spacing. Image references can be added directly in Canvas or Animate.
8. Export a PNG, or import your existing art with the native file picker. **Cast** and **Customize details** provide manual identity and style editing.

Presets include Woodland ink, Pixel adventure, Bold & playful, Sketchbook, Painterly fantasy, and Tiny isometric. The [design notes](docs/design.md) document the inspected Mobbin references.

Generation uses your account's capacity and requires native image generation support from the Codex provider. Account availability is checked in the app. Descriptions and reference images guide consistency; results still need visual review.

## Build and package

Use Rust **1.95.0 or newer**. The dependency lockfile is included. GPUI and its controls are pinned to compatible releases: GPUI 0.2.2 and gpui-component 0.5.1. The two bundled fonts use the SIL Open Font License.

```sh
cargo build --locked --workspace
cargo run -p forge-studio
```

macOS needs Apple's Command Line Tools. GPUI uses runtime Metal shaders, so installing the separate Metal compiler component is unnecessary. Windows needs the Rust MSVC toolchain and Visual Studio C++ build tools with a Windows SDK; GPUI renders with DirectX.

```sh
python3 scripts/package.py
```

Packaging creates a macOS `.app` and zip, or a Windows portable zip with both executables. The Mac bundle receives a local ad hoc signature. Public distribution signing/notarization is separate. On Windows use `python` rather than `python3` if needed. The GitHub Actions workflow builds and tests on both operating systems and uploads the packages.

## CLI

```sh
asset-forge doctor
asset-forge login
asset-forge call projects/create '{"name":"Woodland","style":{"name":"Woodland ink","description":"Hand-painted 2D. Thin green outlines, round proportions, warm amber accents.","palette":["#315C4B","#D7AD70","#F1E9D5"]}}'
asset-forge call projects/list
asset-forge call characters/create '{"projectId":"PROJECT_ID","name":"Mira","description":"Chestnut hair, amber scarf, moss-green tunic, round proportions."}'
asset-forge generate --request request.json
asset-forge call assets/list '{"projectId":"PROJECT_ID"}'
asset-forge call assets/export '{"id":"ASSET_ID","path":"/absolute/path/mira.png"}'
```

`request.json`:

```json
{
  "projectId": "PROJECT_ID",
  "idempotencyKey": "mira-idle-001",
  "characterId": "CHARACTER_ID",
  "kind": "CHARACTER",
  "prompt": "Full-body idle pose, facing right, generous margins.",
  "width": 512,
  "height": 512,
  "transparentBackground": true
}
```

Replace the opaque IDs with the returned IDs. Generate one key per intended generation and reuse that key if retrying it. `generate` and `call jobs/create` wait until the job finishes. `call assistant/message` waits for the guide and any image it starts; the persistent stdio API returns jobs immediately. Output goes to stdout as JSON; diagnostics go to stderr. A failed command exits with status 1.

Use `--data-dir PATH` or `ASSET_FORGE_DATA_DIR` to select a workspace. The default is the operating system's local application-data directory under `AssetForge`. Set `ASSET_FORGE_CODEX` to the **native Codex executable** if it is not detected. On Windows that should be `codex.exe`, rather than npm's `.cmd` wrapper.

One process owns a workspace at a time. Close the desktop app before using a CLI command on the same workspace, or use a different `--data-dir`. Scripts that need a persistent backend can keep `asset-forge serve` open.

To use the same guide from a script:

```sh
asset-forge call styles/presets/list
asset-forge call assistant/message '{"requestId":"setup-woodland-001","message":"Set up a cozy forest RPG and save a scout character.","allowGeneration":false}'
asset-forge call assistant/message '{"requestId":"mira-idle-002","sessionId":"SESSION_ID","message":"Generate the scout in an idle pose.","allowGeneration":true}'
```

Reuse each `requestId` for a retry of that same message. The persistent stdio API accepts guide messages immediately and emits progress notifications. See the API reference for session states and tool boundaries.

## Sprite animation CLI

```sh
asset-forge animate --request animation.json
asset-forge call animations/list '{"projectId":"PROJECT_ID"}'
asset-forge call animations/timing/update '{"id":"ANIMATION_ID","fps":12,"isLooping":true}'
asset-forge call animations/export '{"id":"ANIMATION_ID","path":"/absolute/path/mira-walk.zip"}'
```

`animation.json`:

```json
{
  "projectId": "PROJECT_ID",
  "characterId": "CHARACTER_ID",
  "idempotencyKey": "mira-walk-001",
  "config": {
    "name": "Walk",
    "motion": "WALK",
    "frameCount": 6,
    "columns": 3,
    "frameWidth": 256,
    "frameHeight": 256,
    "fps": 8,
    "isLooping": true
  },
  "prompt": "Walk to the right, relaxed pace, steady foot baseline."
}
```

The CLI waits for a playable clip. Its ID also identifies the generation job for cancellation and recovery. The guide can do the same: ask “Generate Mira's walk cycle,” or “Set this clip to 12 FPS” to adjust timing without another generation.

## API and architecture

The local [v1 API](docs/api.md) uses newline-delimited JSON over stdin/stdout. [Rust contracts](crates/forge-core/src/contract.rs) are the input and output types. The GUI calls that same backend contract from a worker thread, keeping disk and inference work off the UI thread.

- `forge-core`: contract validation, SQLite storage, presets, guide actions, immutable job snapshots, image import/export, Codex JSON-RPC integration and job lifecycle.
- `forge-cli`: human CLI commands and the stdio API transport.
- `forge-studio`: GPUI window, native dialogs, font and color system, and asynchronous state updates.

Codex is launched as a local child process with the stdio app-server protocol. Native image output is validated and stored as PNG. Generation and guide turns disable shell execution, subagents, and unrelated configured MCP integrations through process-local overrides, run in a read-only sandbox, and decline unrelated approval requests. The guide uses explicit tools for scoped workspace actions; image jobs run in separate native image-generation turns. Asset Forge does not read or copy the authentication store.

Jobs store the exact style, character and reference IDs used. SQLite claims each `(projectId, idempotencyKey)` within an immediate transaction protected by a unique constraint. Identical retries replay the original job; changed payloads fail. Keys do not expire. If the process closes or Codex disconnects during inference, the job becomes `UNKNOWN` and is never automatically resubmitted.

Requested output dimensions are enforced after decoding. Sprites are resized proportionally and centered with padding; scenes fill the frame with proportional cropping if needed. Pixel-art descriptions select nearest-neighbor resizing. Opaque requests flatten transparency. A transparent request without actual alpha fails clearly while preserving its image in the library. Animation jobs normalize each grid cell independently, extract ordered PNG frames and create a GIF preview. Clip exports contain an atlas, individual frames, GIF and Aseprite-style JSON rectangles/timing. Generated motion is approximate: inspect the playback for pose continuity, alignment and identity before importing into an engine. PNGs retain full alpha; GIF previews have palette and timing limitations.

## Verification

```sh
cargo fmt --all -- --check
cargo test --locked -p forge-core -p forge-cli
cargo clippy --locked --workspace --all-targets -- -D warnings
```

The subprocess integration tests use a deterministic fake Codex server and do not consume subscription capacity. They cover real PNG decoding, image dimensions, duplicate requests, provider failure, missing/invalid images, disconnection and cancellation. Guide tests verify action replay, project isolation, generation permissions, the one-image limit, custom palette changes with preserved references, saved preset dimensions, and uncertain disconnects. Store tests cover snapshots, interrupted-process recovery, workspace leases, cross-project references and validation.

The macOS app has been built and exercised with native reference import/pinning, guide-created cast, sprite-sheet setup, playback, frame stepping and PNG/animation export. All 21 local tests pass. Live Codex app-server generated the included [Mira sprite](examples/generated/mira-idle.png), a matching [woodland scene](examples/generated/woodland-scene.png), a [guided sprite](examples/generated/mira-guided.png), and a six-frame [walk atlas](examples/generated/mira-walk-atlas.png) with [GIF preview](examples/generated/mira-walk.gif) and [portable ZIP](examples/generated/mira-walk.zip). Windows build automation is configured in the private repository; its build and runtime verification are pending.

See [current verification](docs/verification.md) for the requirement-by-requirement evidence and the remaining platform checks.

## Integration references

[GPUI examples](https://gpui.rs/examples/) and the [Codex app-server protocol](https://learn.chatgpt.com/docs/app-server) informed the integration. The app uses Codex-managed authentication for a local application. If turning it into a hosted or commercial service, review the current OpenAI authentication requirements before changing its distribution model.
