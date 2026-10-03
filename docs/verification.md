# Current verification

## Installer updates in 0.1.2

The Update button now checks and downloads newer stable GitHub releases, then offers Restart & install. It checks the expected platform asset, HTTPS origin/redirect hosts, size and SHA-256 digest. macOS additionally validates archive paths, package/binary metadata and code signature, copies beside the installed app and retains/restores its previous bundle. Windows uses the existing NSIS installer in the current installation directory. A detached CLI helper waits for app exit, retains OS download protection and reopens the app. Active work in any game blocks restart; the GUI also protects a chat draft and freezes queued work during the shutdown handoff. The installer helper never opens the workspace or Codex.

- All 45 macOS tests pass (29 core and 16 subprocess tests), with formatting and strict workspace Clippy. Update regressions cover numeric version comparisons/no downgrade, stable/platform selection, origin/digest validation, truncated/corrupt/oversized downloads, archive traversal, failed replacement rollback and backup retention. A real signed harmless Mac fixture rejects executable tampering, replaces the bundle with its valid payload, keeps the original app, preserves separate workspace bytes and retains a valid code signature. All-game update readiness rejects both queued generation and a thinking guide, then succeeds when idle; unknown request fields fail validation.
- Optimized packages, native Update interaction and Windows compilation are pending at this checkpoint. Earlier public releases remain unchanged; versions before 0.1.2 use the old manual-download button.

## Update button check after the local idle fix

Verified on 2026-10-03 in the rebuilt local Mac app. Clicking **Update** displayed the installed version and opened Safari at the stable `/releases/latest` URL, which resolved to the published `v0.1.1` release. The browser displayed its Latest marker and both installer assets. Unauthenticated HEAD requests to the stable macOS DMG and Windows Setup.exe download URLs followed redirects and returned HTTP 200 with binary content types. No installation or release publication was performed. Update currently opens the release page for manual download/installation; it does not compare versions or install updates automatically. The local idle fix remains newer than the published installers. Native Windows button interaction was not tested.

## Woodland idle repair after v0.1.1

Diagnosed on 2026-10-03 from the native app and persisted frames. Three legacy `Idle` entries incorrectly extracted 256 × 256 rectangles from the upper quarter of a 1536 × 1024 walk atlas whose actual cells were 512 × 512. The native preview displayed pieces of Mira instead of complete idle poses.

- The chat now rejects obvious source/grid mismatches and known generated static-image extraction; its instructions distinguish source cell dimensions from generation defaults and require new poses for a different motion. Regression checks cover correct full-sheet extraction, generic native-import metadata, exact replay and deliberately selected subregions through the direct API.
- One real Codex request generated a genuine six-frame standing Mira idle using the saved original identity and Woodland style. Whole-character visual inspection and transparency/bounds checks passed, but the render initially displaced the bottom row upward by 10 pixels.
- Standing idle normalization now aligns the lower silhouette and foot baseline. An idempotent `animations/align` API/chat action also creates an aligned atlas/clip from an existing valid idle without generating images or changing its source. Tests cover source preservation, replay, frame/atlas export agreement and preservation of jump motion.
- The aligned Mira frames all share a bottom bound of 247 in their 256-pixel cells. A valid ZIP contains the atlas, six frames, GIF and JSON. After a SQLite backup, the API imported this aligned atlas into the existing Woodland demo, created `Mira Idle`, and recoverably removed exactly the three broken idle clips. The project style, Mira identity/references, original images and walk clips were preserved.
- All 40 macOS tests pass (24 core and 16 subprocess integration tests), along with formatting and strict workspace Clippy. The final native-import compatibility change also passed its targeted regression and strict Clippy.
- The optimized local Mac bundle was rebuilt from `14a9f2c`, passed deep strict signature verification and reopened successfully. Native Woodland Library now lists only the repaired `Mira Idle` and the two preserved Walk clips. Playback, Pause, Next frame (3 to 4), atlas view and return to playback show complete standing poses with a stable foot baseline. The app remains open playing `Mira Idle` at 8 FPS/loop.
- Native Download clip saved `target/woodland-idle-fix/Mira-Idle-UI.zip`. ZIP CRC and all nine entries pass; the export contains the 768 × 512 atlas, six 256 × 256 frames, GIF and JSON with 125 ms timing, 8 FPS/loop and the original Mira/project IDs. Published v0.1.1 installers are unchanged; this is a local Mac fix, and Windows native verification remains pending.

## Animation QA after v0.1.1

Tested on 2026-10-03 with the real Codex app-server, Forge chat and Gravebound Raider's individual PNG. The Mac was locked and the running app held the default workspace lease, so the test used a separate local workspace seeded through the API with the existing game's style, subject description and original image. No existing game data was changed. Native playback controls and Windows interaction were not exercised.

- Forge chat queued one `ATTACK` clip: six frames, three columns, 256 × 256 cells, 8 FPS, non-looping, pinned original identity. The first render was accepted by v0.1.1, extracted six distinct RGBA frames and exported a valid ZIP. Visual inspection found boundary clipping and a changing foot baseline, so the animation did **not** pass visual quality review.
- Playback timing changed to 12 FPS/loop and returned to 8 FPS/play once without creating jobs or changing the generation snapshot. Replaying the exact animation request returned the same job. ZIP CRC, nine entries, 768 × 512 atlas, six PNG frames, frame rectangles and 125 ms JSON timing all passed. Export refused to overwrite an existing file. The GIF has six frames and 120 ms delays because GIF quantizes time to centiseconds; PNG playback and JSON retain the requested 8 FPS.
- Generated sheets now reject a visible run of pixels at any cell edge as `CLIPPED_ANIMATION_FRAME`, before extraction. The source image remains saved and available for deliberate grid setup. The prompt requests consistent scale and explicit transparent padding around each pose. This guard only checks borders; motion continuity, alignment and background quality still require visual review. Explicitly imported sheet grids are unchanged.
- A second real render with smaller requested poses and larger padding failed this new border check. Its source contained unwanted background haze reaching the cell edges. Its PNG was preserved; no completed frames, preview or downloadable clip were created. There were exactly two deliberate image generation requests, with no automatic retry.
- All 38 macOS tests pass (22 core and 16 Unix subprocess integration tests), including the new clipped-output/source-preservation/export-refusal regression. Workspace formatting and strict Clippy pass. These fixes are newer than the published v0.1.1 installers; existing release artifacts were not replaced.

Local QA files are under `target/animation-qa/`: requests, API responses, pixel bounds, a reviewed test ZIP and `verified-result.json`. The test ZIP is evidence of the pipeline and retains the known visual flaws; it is not an approved production animation.

## Published v0.1.1 evidence

Published manually on 2026-10-03 from source `ae589433842662c3f1cb226d44ca3e108940d986`, with annotated tag `v0.1.1` and the `release/0.1` branch at that source. The [public release](https://github.com/Gio-Bruno/game-asset-generator/releases/tag/v0.1.1) includes both installers, both portable ZIPs and SHA-256 checksums. The original v0.1.0 tag and artifacts are unchanged. GitHub Actions remains disabled; no CI/CD was added.

- Both optimized platforms were rebuilt locally from the same committed source. Native Mac packaging verified all build/package inputs against that commit; Windows packaging verified its original build manifest, version and executable hashes.
- All 37 macOS tests pass: 22 core and 15 Unix subprocess integration tests. Formatting and strict workspace Clippy pass. All 22 compiled Windows core tests pass under Wine; the packaged Windows CLI reports 0.1.1 and passes styles/motions, project rename, structure persistence and deletion/restoration checks in an isolated workspace.
- The generated icon's transparent master, macOS ICNS and ten Windows ICO sizes were inspected. The Windows studio EXE has group icon ID 1 with ten byte-identical image resources and the verified GPUI DPI/common-controls manifest. The NSIS installer contains the matching icons; installer/uninstaller and shortcuts use the app icon.
- Mac bundle icon declaration and 0.1.1 metadata, deep strict code signature and DMG checksum pass. Both ZIPs pass CRC, binary hash, source commit and icon checks. The packaged Mac CLI passes its isolated persistence smoke check.
- All five published files were downloaded publicly without authentication and match the local SHA-256 checksums. The stable latest-release URL resolves to v0.1.1, so the existing Update button targets this release.
- The final Mac app opened, but native automation stayed on the initial loading view and then timed out during Dock inspection. Final native visual interaction remains unverified; prior hierarchy and real separate-asset UI checks are recorded below. Native Windows UI/installer execution remains unverified. The Mac package is ad hoc signed and not notarized; Windows distribution is unsigned.

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
