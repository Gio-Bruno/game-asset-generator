# Current verification

Checked on 2026-10-03. macOS has been built and exercised. Windows verification is running through the private GitHub repository.

| Requirement | Evidence | Remaining verification |
| --- | --- | --- |
| Local CLI/API and shared GUI backend | Release CLI, NDJSON subprocess checks and native GPUI app use the same typed `Service::dispatch` backend | Windows build and runtime smoke |
| Codex CLI and subscription | Real Codex 0.160.0 app-server detects the existing Pro account and generated the included sprites, scene and walk atlas | Real signed-out browser round trip requires user sign-in |
| Style and character consistency | Saved style/identity snapshots; Mira's pixels attached as both character and style references to a woodland scene | Generated assets and motion need visual review |
| Custom style | Real guide customized palette, lighting and name while preserving pinned references; native editor reviewed | None for macOS |
| macOS app | Optimized GPUI build, packaged app, ad hoc signing, native dialogs and successful restart | None for the verified local package |
| Windows app | MSVC paths, portable packaging and GitHub Actions matrix | Windows build and runtime smoke |
| Simple presets and controls | Six visual directions and six motion presets; detailed output/grid fields behind disclosure; buttons for sizes, timing and looping | Native simplified controls reviewed |
| Mobbin references | Gamma, Runway, Firefly and Leonardo screens inspected; native result reviewed at 1320 and approximately 1059 logical pixels wide | Minimum 720-pixel height was not exercised |
| Guide actions | Real guide created cast and style, generated a sprite and animation, pinned references, and updated timing; native character and timing receipts verified | None for the exercised actions |
| Sprite/animation generation and setup | Real six-frame transparent walk atlas, PNG extraction, GIF preview and ZIP export; native import, grid fitting, extraction, pause, step and atlas view exercised | Guide timing changes refresh both playback and FPS controls |
| Image references | Native import and style/character pinning; backend attaches actual same-project pixels | Direct import, thumbnail display and one-click removal verified in Animate |

## Completed checks

- Formatting and strict workspace Clippy pass.
- 21 tests pass on macOS: 11 core tests and 10 subprocess integration tests. The Python executable fixture is Unix-only; Windows still runs the 11 core tests.
- Release workspace build succeeds.
- Real guided sprite generation produced a 512 × 512 PNG with actual alpha. Reference pinning and style customization created no extra image job.
- Real guided walk generation produced a 1536 × 1024 atlas with six 512-pixel cells and actual alpha, plus a GIF and portable ZIP. Its repeated poses illustrate why motion needs visual review.
- Native app: guide-created Mira, imported atlas, fitted/extracted six frames, paused/stepped playback, displayed atlas and exported a ZIP. The ZIP contains nine files, six frame records and the selected timing.
- Native direct reference import in Animate adds a removable thumbnail without changing the selected clip or playback. Extra references are sent to the next manual generation.
- Native PNG export matches the source bytes. Style and character reference buttons persisted the selected image.
- Native guide changed Walk to 12 FPS, then 10 FPS and back to 12 FPS without generating an image. The latest UI refreshes the FPS controls after guide edits. Workspace inspection showed zero image jobs in this GUI test workspace.
- An isolated fake server exercised signed-out, waiting, cancellation and connected account states. Its placeholder URL caused a browser authentication error; that test page was closed. This is not evidence of a real browser OAuth round trip. The real app still detects the existing Codex Pro account.

Temporary test workspaces are separate from the developer's default workspace. The real Codex authentication store was not read, copied, modified or logged out.
