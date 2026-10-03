# Current verification

## Unreleased separate production assets

Verified locally on 2026-10-03. The app can now render several requested subjects as individual production images instead of substituting a concept board. Published v0.1.0 installers are unchanged.

- All 37 tests pass on macOS: 22 core and 15 Unix subprocess integration tests. Formatting, strict workspace Clippy, optimized builds and the local Mac bundle's strict signature verification pass.
- Batch checks cover independent named PNGs and exports, correct subject associations, atomic rollback for invalid items, replay across restart, generation allowance enforcement, cancellation of one item without cancelling its peers, and one-shot CLI completion after a partial failure.
- Concept board reclassification preserves the original pixels and clears the incorrect single-character association. Production prompts isolate one subject and prohibit copying a whole reference board. Character, structure, prop and scene identities are supported; sprite sheets retain one identity across frames.
- Each subject's first successful matching individual image becomes its initial pinned visual reference. Existing references and identities edited during rendering are preserved.
- Native macOS Forge used Add to chat on Ashen Bastion's concept board, then queued five separate named images in the existing game. The sidebar displayed each pending item and its rendering/queued state. All five real Codex jobs succeeded, each producing one independent 512 × 512 PNG: Ironbolt Tower, Ember Reliquary, Warding Obelisk, Gravebound Raider and Cinder Fiend. Visual review shows one complete named subject per file, without labels or other subjects; all five have actual transparent pixels and their own pinned identity reference. These are individual renders using the original board as reference, rather than pixel-exact crops.
- The final native Library lists all five individual assets plus the preserved concept reference. Structure and character previews display their own names and categories. Download PNG exported Warding Obelisk byte-for-byte; Add to chat attached only that structure and its removable reference chip. The app remains open on Gravebound Raider in Ashen Bastion. The workspace's two active games were preserved.
- Native Windows interaction with this iteration and new animation generation remain unverified. The backend retains sprite extraction, playback timing and per-clip exports.

## Unreleased game hierarchy and creative questions

Verified locally on 2026-10-03. This iteration is packaged in `dist/Asset Forge.app` for testing; the published `v0.1.0` installers and tag retain their original source.

- All 32 tests pass on macOS: 20 core tests and 12 Unix subprocess integration tests. Formatting, strict workspace Clippy and the optimized workspace build pass. The local Mac app passes deep, strict signature verification.
- Automated checks cover rename validation and style preservation; recoverable deletion of games, images and clips; dependent clip restoration; reference unpinning; persistence across restart; and refusal to delete while generation or Forge is working.
- The guide cannot create a game before a setup question is answered or skipped. Pending questions block mutations. Tests cover selectable choices, typed answers, Skip, stale answers, replay safety, retained image attachments and the original one-image allowance.
- Native macOS checks show games and their assets in a vertical sidebar, with the selected game's name above Library/World. Rename/Save updated both the sidebar and title and survived reopening.
- The image confirmation's Keep it action retained the sample. Delete removed it from the list and preview; Undo restored both. Deleting the temporary game switched to Bramblewatch; Undo restored the temporary game, name and image. The temporary game was then removed again, leaving the four existing games intact.
- A real Codex planning conversation offered six art-style pills. Selecting Crisp pixel art resumed the conversation with that choice, then displayed a separate world-theme question. Skip accepted defaults for that detail and completed the discussion. This check requested no saved game changes or image generation.
- The updated app is open locally on Bramblewatch. Native Windows checks, full image creation/revision through this iteration's chat, clip deletion through the native UI and minimum-height layout checks remain pending. Clip deletion/restoration is covered by the backend suite.

## Published v0.1.0 evidence

Updated on 2026-10-03 for artifact source `17e5d33e7d49b8fc4cddbef629695669d81b20aa`. The simplified interface separates Library previews and read-only catalogs from Forge chat creation/editing. Its expanded backend suite passes 28 tests: 16 core and 12 Unix subprocess integration tests. The latest native Mac checks cover Library/World browsing, attachments, New game, restart and PNG export. Full chat creation/revision interactions and the redesigned animation controls remain untested natively.

Both target packages now come from source `17e5d33`; the Windows package uses a manually built optimized x64 MSVC cross-build with static CRT. Documentation evidence is updated separately from the artifact source. The repository is public, GitHub Actions is disabled, and its workflow has been removed at the user's request. Builds and releases are manual. Earlier source `8612085` passed a Windows native startup smoke check; that historical result does not verify the current Windows interface.

| Requirement | Evidence | Scope and limits |
| --- | --- | --- |
| Local CLI/API and shared GUI backend | Shared typed `Service::dispatch`; 28 macOS tests pass; current Windows CLI preset/structure persistence checks pass under Wine | Wine checks do not replace native Windows interaction |
| Codex CLI and subscription | Real Codex 0.160.0 app-server detects the existing Pro account and generated the included sprites, scene and walk atlas | Real signed-out browser round trip requires user sign-in |
| Style and character consistency | Saved style/identity snapshots; Mira's pixels attached as both character and style references to a woodland scene | Generated assets and motion need visual review |
| Custom style | Earlier real guide customized palette, lighting and name while preserving pinned references; current World shows the saved style | Latest end-to-end chat style editing not exercised natively |
| macOS app | Source `17e5d33` optimized app opened/restarted with real Codex Pro; Library/World, attachments, New game and PNG export exercised | Some background clicks/render automation were delayed; full chat and new animation interactions remain pending |
| Windows app | Optimized x64 MSVC cross-build from `17e5d33`; 16 Windows core tests and CLI persistence smoke checks pass under Wine; PE/static CRT/shaders/DPI/manifest validated | Current native Windows interaction and installer execution unverified; no separate Visual C++ runtime needed |
| Simple interface | Current Library displayed the existing concept sheet; World displayed five saved subjects; read-only browsing and chat attachment controls exercised | Creation/revision execution through native chat not tested |
| Mobbin references | Gamma, Runway, Firefly and Leonardo screens inspected; previous native result reviewed at 1320 and approximately 1054 logical pixels wide; current Library/World inspected natively | Minimum 720-pixel height remains untested |
| Subject catalog | Two goblin characters and three `STRUCTURE` towers appear in current World; persistence, default compatibility, partial updates and project-scoped guide tests pass | Missing tower records were explicitly repaired through the API after a backup, using pinned existing concept-sheet pixels; no automatic migration |
| Guide actions and new games | Backend create/edit/new-game replay tests pass; native New game opened a clean chat and refresh preserved it | Complete native chat creation/revision execution pending |
| Sprite/animation generation and setup | Backend generation, extraction, timing, GIF and ZIP operations retained; earlier native playback/step/atlas/export checks passed | New animation buttons not exercised natively; motion needs visual review |
| Image revisions | Native Add to chat on an image and a structure thumbnail attached references; prompt entry/removal worked; subprocess tests verify real pixels and inherited generation refs | Up to eight same-project combined references; full native revision generation not tested |
| Distribution installers | ZIP CRC/hash checks, Mac signature/DMG checks and strict NSIS compilation pass for current source packages | Mac is ad hoc signed and not notarized; Windows installer is unsigned and execution remains unverified |

### Automated checks for v0.1.0

- 28 tests pass on macOS: 16 core tests and 12 subprocess integration tests. The executable Python fixture is Unix-only.
- Subject tests cover saved structures and props, legacy character records, persistent categories, immutable identity snapshots, partial updates preserving references, guide action replay and denied cross-project edits.
- New-game tests preserve the old game's style and assets, copy only explicitly attached current-message images, remap their IDs, and avoid duplicate projects/images on replay.
- Chat image checks verify message-scoped attachments, project ownership, combined limits, actual local-image input to the guide, and inherited image references for revisions. Empty attachments remain compatible with earlier requests.
- Current optimized Windows binaries passed all 16 core tests under Wine. Their packaged CLI reported six styles and six motions, and structure create/update/get/list persistence checks passed.
- Windows PE x64, static CRT imports, pinned shader bytes, DPI metadata, embedded manifest and source/binary provenance were validated.
- Both platform packages passed ZIP CRC and hash validation. The Mac app signature and DMG were checked; the Windows installer compiled with NSIS warnings treated as errors. Native Windows installation/uninstallation was not tested.
- The public `v0.1.0` release contains both installers, both portable ZIPs and checksums. All five files were downloaded without authentication through the stable latest-release links and matched their local SHA-256 hashes.

### Native Mac checks for v0.1.0

- The packaged app at source `17e5d33` opened and restarted, detecting the existing real Codex Pro account.
- Library displayed the existing game concept sheet. World showed five saved subjects: two goblin characters and three towers classified as `STRUCTURE`.
- The missing tower identities were saved through the API after a workspace backup and pinned to the existing sheet; no new image generation was needed for this catalog repair.
- Add to chat worked from the selected asset and a structure thumbnail. Reference chips, prompt entry and removal worked.
- New game opened a clean chat; refresh retained that clean state.
- The native PNG Download/Save dialog exported bytes identical to the selected source image.
- Update opened the published latest release in Safari, resolving to `v0.1.0` with its source tag and installer downloads.
- Background click/render automation sometimes responded slowly. Full native new-chat creation/revision execution, redesigned animation controls and the minimum 720-pixel window height remain pending.

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

Automated tests use separate temporary workspaces. Native management checks used a disposable game and imported sample in the default workspace; that game is now removed through the recoverable deletion API. The real Codex authentication store was not read, copied, modified or logged out.
