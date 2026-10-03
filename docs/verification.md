# Current verification

Updated on 2026-10-03. The simplified interface separates Library previews and read-only catalogs from Forge chat creation/editing. Its expanded backend suite passes 28 tests: 16 core and 12 Unix subprocess integration tests. Native verification of the new Library/World interface is pending; earlier native checks below apply to the previous interface.

Both platforms previously built and passed checks in GitHub Actions. Source revision `8612085` passed Windows native startup and packaged CLI smoke checks. The published Windows cross-build uses a static C runtime. The repository is public, GitHub Actions is disabled, and its workflow has been removed at the user's request. Future builds and releases are manual.

| Requirement | Evidence | Scope and limits |
| --- | --- | --- |
| Local CLI/API and shared GUI backend | CLI, NDJSON subprocess checks and GPUI app share typed `Service::dispatch`; expanded backend suite passes | Earlier release/startup checks covered both targets; latest native interface checks pending |
| Codex CLI and subscription | Real Codex 0.160.0 app-server detects the existing Pro account and generated the included sprites, scene and walk atlas | Real signed-out browser round trip requires user sign-in |
| Style and character consistency | Saved style/identity snapshots; Mira's pixels attached as both character and style references to a woodland scene | Generated assets and motion need visual review |
| Custom style | Earlier real guide customized palette, lighting and name while preserving pinned references; backend operations retained | Latest World summaries/chat flow still need native verification |
| macOS app | Earlier optimized GPUI app, ad hoc signing, native dialogs and restart exercised | New Library/World native verification pending |
| Windows app | Historical MSVC release, 11 core tests, strict Clippy, packaged CLI persistence and responsive native window; subsequent manual cross-build uses static CRT | No separate Visual C++ runtime required by the static build; full interactive Windows flows not manually exercised |
| Simple interface | Library previews and read-only image/animation lists; World saved style and Characters/Structures/Props; Forge performs creation/editing | Source review completed; no new native UI verification claimed |
| Mobbin references | Gamma, Runway, Firefly and Leonardo screens inspected; previous native result reviewed at 1320 and approximately 1054 logical pixels wide | New layout not yet reviewed natively; minimum 720-pixel height was not exercised |
| Subject catalog | Default-compatible `CHARACTER`, `STRUCTURE`, `PROP`; persistence, partial updates, reference preservation and project-scoped guide tests pass | Existing narrative-only designs need to be saved as subjects; no automatic user-data repair performed |
| Guide actions and new games | Subject create/edit and explicit new-game tools; replay tests preserve old games and remap only current attached images | Earlier real guide rendered sprites/animation and changed timing; new native chat interactions pending |
| Sprite/animation generation and setup | Existing backend generation, extraction, timing, GIF and ZIP operations retained; earlier native playback/step/atlas/export checks passed | Creation/setup now happens in chat; new Library playback UI pending native verification; motion needs visual review |
| Image revisions | Add to chat and + Image feed current-message references; subprocess checks verify actual image input and inherited generation refs | Same-project ownership and up to eight combined references enforced; new attachment UI pending native verification |

## Latest automated checks

- 28 tests pass on macOS: 16 core tests and 12 subprocess integration tests. The executable Python fixture is Unix-only.
- Subject tests cover saved structures and props, legacy character records, persistent categories, immutable identity snapshots, partial updates preserving references, guide action replay and denied cross-project edits.
- New-game tests preserve the old game's style and assets, copy only explicitly attached current-message images, remap their IDs, and avoid duplicate projects/images on replay.
- Chat image checks verify message-scoped attachments, project ownership, combined limits, actual local-image input to the guide, and inherited image references for revisions. Empty attachments remain compatible with earlier requests.
- The simplified UI received a source review. Native Library/World, attachment, new-game and chat-led creation checks remain pending.

## Historical native and release evidence

- The previous source passed formatting and strict workspace Clippy, with 21 tests on macOS: 11 core and 10 subprocess tests. The historical Windows runner executed the 11 core tests.
- Previous release workspace builds succeeded locally on macOS and in GitHub Actions on both macOS and Windows.
- [Completed cross-platform build run](https://github.com/Gio-Bruno/game-asset-generator/actions/runs/37135878329) for source revision `8612085` passed its required checks. The Windows startup smoke opened a responsive Asset Forge window; packaged CLI persistence and six style/motion presets passed. This workflow is now disabled and removed.
- Real guided sprite generation produced a 512 × 512 PNG with actual alpha. Reference pinning and style customization created no extra image job.
- Real guided walk generation produced a 1536 × 1024 atlas with six 512-pixel cells and actual alpha, plus a GIF and portable ZIP. Its repeated poses illustrate why motion needs visual review.
- Previous sheet import: a 1536 × 1024 sheet automatically fitted to six 512-pixel frames, stayed selected despite existing clips, and extracted into successful playback. Sheet setup is now requested through Forge chat.
- Native app: guide-created Mira, imported atlas, fitted/extracted six frames, paused/stepped playback, displayed atlas and exported a ZIP. The ZIP contains nine files, six frame records and the selected timing.
- Previous native reference import added a removable thumbnail without changing the selected clip or playback. The current UI uses Add to chat and + Image instead of manual-generation attachment controls.
- Previous native PNG export matched the source bytes. Style and character reference buttons persisted the selected image; pinning is now a chat action.
- The real guide changed Walk to 12 FPS, then 10 FPS and back to 12 FPS without generating an image. Workspace inspection showed zero image jobs in that GUI test workspace. Timing changes are retained as a chat operation.
- An isolated fake server exercised signed-out, waiting, cancellation and connected account states. Its placeholder URL caused a browser authentication error; that test page was closed. This is not evidence of a real browser OAuth round trip. The real app still detects the existing Codex Pro account.

Temporary test workspaces are separate from the developer's default workspace. The real Codex authentication store was not read, copied, modified or logged out.
