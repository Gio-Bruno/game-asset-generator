# Asset Forge

A local 2D game asset workshop for macOS and Windows, powered by your Codex subscription. Tell Forge, your AI art director, about the game. Forge saves its art direction and reusable characters, structures and props, then generates assets and sprite animations with consistent identities and image references.

The interface is native Rust/GPUI. The CLI, stdio API and desktop app share the same typed backend. No web server, API key, Node runtime or separate paid image API is required by Asset Forge.

## Download and open the app

Get the [latest release](https://github.com/Gio-Bruno/game-asset-generator/releases/latest):

| Platform | Installer | Portable package |
| --- | --- | --- |
| macOS 12+, Apple Silicon | [Download DMG](https://github.com/Gio-Bruno/game-asset-generator/releases/latest/download/Asset-Forge-macOS-arm64.dmg) | [Download ZIP](https://github.com/Gio-Bruno/game-asset-generator/releases/latest/download/Asset-Forge-macOS.zip) |
| Windows 10/11, x64 | [Download Setup.exe](https://github.com/Gio-Bruno/game-asset-generator/releases/latest/download/Asset-Forge-Windows-x64-Setup.exe) | [Download ZIP](https://github.com/Gio-Bruno/game-asset-generator/releases/latest/download/Asset-Forge-Windows.zip) |

On macOS, open the DMG and drag **Asset Forge** into **Applications**. On Windows, run Setup.exe; it installs for your user and adds a Start menu shortcut. The published Windows cross-build links its C runtime statically and needs no separate Visual C++ runtime installation. These first release packages do not have public distribution signatures; the Mac app uses a local ad hoc signature and is not notarized.

Choose **Update** in the app to open the latest release, then download its installer. Close Asset Forge before installing an update. Your projects and generated assets are stored separately from the application.

On macOS, open `dist/Asset Forge.app`. The portable CLI is `dist/asset-forge`. On Windows, extract `dist/Asset-Forge-Windows.zip` and open `asset-forge-studio.exe`; `asset-forge.exe` is the CLI.

1. Install the [Codex CLI](https://learn.chatgpt.com/docs/cli) if it is not already available. Asset Forge was verified with Codex CLI **0.160.0**.
2. Open Asset Forge. It detects the existing Codex login. Otherwise choose **Connect Codex** and complete sign-in in your browser; Codex stores and refreshes the credentials.
3. Choose **New game** to start a fresh chat, then describe your game: “Set up a woodland tower defense with an arrow tower, a cannon tower and a scout.” Forge chooses sensible style defaults and saves the named characters, structures and props. A request for a new game in an existing chat creates a separate project.
4. Browse **World** to review the saved art direction and the **Characters**, **Structures** and **Props** catalog. These are read-only summaries; ask Forge to change names, designs, colors, lighting or perspective.
5. Ask Forge to create an image: “Generate the arrow tower,” “Make Mira's first idle pose,” or “Create a matching woodland scene.” The guide uses the saved style and subject identity and queues an image only when requested. Setup and advice do not require generation.
6. Browse **Library** to see images and animations, select a preview, and export the result. The library provides read-only image and animation lists rather than creation forms.
7. Choose **Add to chat** on an asset, or **+ Image** in Forge to import a reference, then ask for a revision: “Keep this tower's silhouette and make its roof teal.” Forge receives the actual attached pixels; the generation also inherits them. Attachments apply to that message and can be removed before sending.
8. Ask Forge for Idle, Walk, Run, Jump, Attack or custom motion. It can generate a sprite animation, extract frames from an attached sheet, pin an image as a style or subject reference, and change playback timing. Preview, pause or step through the resulting clip in Library, then export its portable ZIP.

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

Packaging creates the macOS `.app`, DMG and ZIP, or a Windows installer and portable ZIP with both executables. Windows installer packaging requires [NSIS](https://nsis.sourceforge.io/). The Mac bundle receives a local ad hoc signature. Public distribution signing/notarization is separate. On Windows use `python` rather than `python3` if needed. Builds and releases are manual; GitHub Actions is disabled and there is no CI/CD workflow. See the [release procedure](docs/releases.md) for local packaging, release branches and version tags.

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

The subprocess integration tests use a deterministic fake Codex server and do not consume subscription capacity. They cover real PNG decoding, image dimensions, duplicate requests, provider failure, missing/invalid images, disconnection and cancellation. Guide tests verify action replay, project isolation, generation permissions, the one-image limit, custom palette changes with preserved references, saved preset dimensions, uncertain disconnects, subject classification and new-game creation. Chat reference tests check project scope, combined image limits and legacy messages. Store tests cover snapshots, interrupted-process recovery, workspace leases, cross-project references and validation.

The expanded suite passes 28 tests: 16 core tests and 12 Unix subprocess integration tests. At source `17e5d33`, the native Mac app opened and restarted with the existing Codex Pro login. Library displayed the existing concept sheet, World displayed two characters and three saved structures, Add to chat attachments and removal worked, New game retained a clean chat after refresh, and native PNG export matched the source bytes. Complete chat creation/revision interactions and the redesigned animation controls still need native checks.

The same source built optimized Windows x64 executables with static CRT, verified shaders, DPI metadata and build provenance. All 16 Windows core tests passed under Wine, as did the packaged CLI's preset and persistent structure checks. Both platform packages passed archive/hash validation, Mac signature and DMG checks, and strict NSIS compilation. Native Windows interaction and installation/uninstallation have not been exercised.

Earlier live Codex app-server checks generated the included [Mira sprite](examples/generated/mira-idle.png), a matching [woodland scene](examples/generated/woodland-scene.png), a [guided sprite](examples/generated/mira-guided.png), and a six-frame [walk atlas](examples/generated/mira-walk-atlas.png) with [GIF preview](examples/generated/mira-walk.gif) and [portable ZIP](examples/generated/mira-walk.zip). The historical Windows build opened a responsive native window. GitHub Actions is now disabled and its workflow is removed; subsequent builds use the manual release procedure.

See [current verification](docs/verification.md) for the requirement-by-requirement evidence and the remaining platform checks.

## Integration references

[GPUI examples](https://gpui.rs/examples/) and the [Codex app-server protocol](https://learn.chatgpt.com/docs/app-server) informed the integration. The app uses Codex-managed authentication for a local application. If turning it into a hosted or commercial service, review the current OpenAI authentication requirements before changing its distribution model.
