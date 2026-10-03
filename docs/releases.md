# Manual releases

Releases are prepared and published manually. GitHub Actions and other hosted CI/CD are not enabled. The stable download links always select the latest published, non-prerelease GitHub release:

- [macOS Apple Silicon DMG](https://github.com/Gio-Bruno/game-asset-generator/releases/latest/download/Asset-Forge-macOS-arm64.dmg)
- [Windows x64 installer](https://github.com/Gio-Bruno/game-asset-generator/releases/latest/download/Asset-Forge-Windows-x64-Setup.exe)
- [macOS portable ZIP](https://github.com/Gio-Bruno/game-asset-generator/releases/latest/download/Asset-Forge-macOS.zip)
- [Windows portable ZIP](https://github.com/Gio-Bruno/game-asset-generator/releases/latest/download/Asset-Forge-Windows.zip)

The app's update button opens the latest release page. Installation is explicit: quit Asset Forge, download the installer for the current platform, and replace the installed application. Workspace data and Codex authentication are stored separately from the installed program.

## Distribution targets

The macOS package contains Apple Silicon binaries. Open the DMG and drag Asset Forge to Applications; the command-line executable is also inside `Asset Forge.app/Contents/MacOS`. The app is currently ad hoc signed rather than signed with an Apple Developer ID and notarized. A downloaded build may therefore require the macOS Open Anyway action in Privacy & Security.

The Windows package targets Windows 10/11 x64 and installs per user to `%LOCALAPPDATA%\Programs\Asset Forge`. It creates Start menu and desktop shortcuts and an uninstall entry. Install and uninstall check that the studio and CLI are closed before changing the program files. Its uninstaller removes the bundled program files and shortcuts, preserves added files, and never deletes the Asset Forge workspace or Codex login. The installer is currently unsigned. The published Windows cross-build uses a static C runtime, so no separate Visual C++ runtime installation is required. The installer does not download dependencies.

## Local packaging

Use Python 3.9 or newer and the Rust toolchain required by this repository. On macOS, `hdiutil`, `ditto`, and `codesign` are supplied by the operating system. Windows installer compilation requires the local NSIS `makensis` compiler, on either macOS or Windows. If the compiler is outside `PATH`, pass `--makensis /absolute/path/to/makensis`.

```sh
# On the target OS: build the workspace and make the app/ZIP plus DMG or Setup.exe.
python3 scripts/package.py
python3 scripts/smoke_package.py

# Repackage previously built native outputs without rebuilding.
python3 scripts/package.py --no-build

# Explicitly create just the app/portable ZIP when an installer is not needed.
python3 scripts/package.py --no-build --portable-only
```

For a native Windows distribution build, set the static runtime flag in PowerShell before packaging:

```powershell
$env:RUSTFLAGS = "-C target-feature=+crt-static"
python scripts/package.py
python scripts/smoke_package.py
```

Version numbers come from `[workspace.package]` in `Cargo.toml`. macOS package metadata and Windows installer metadata use that version. Native packaging checks the compiled CLI's reported version; Windows packages also check both EXEs are PE x64. Output paths are replaced instead of truncating a running executable.

To package verified Windows binaries on macOS, provide their original build manifest. The manifest records the commit that actually produced the EXEs; do not substitute a newer source commit or relabel older binaries as a rebuild. Both hashes and the version must match before packaging proceeds:

```json
{
  "schemaVersion": 1,
  "version": "0.1.0",
  "sourceCommit": "<actual 40-character build commit>",
  "buildUrl": "<optional verification/build evidence URL>",
  "sha256": {
    "asset-forge.exe": "<SHA-256 of the CLI EXE>",
    "asset-forge-studio.exe": "<SHA-256 of the studio EXE>"
  }
}
```

```sh
python3 scripts/package.py --platform windows \
  --windows-build /absolute/path/to/windows-binaries \
  --windows-build-manifest /absolute/path/to/build-manifest.json
```

Every package includes the CLI, README, docs, examples, font licenses, and a release manifest with binary hashes. `--no-build` on native outputs records existing output provenance and does not assert that those files were built from current HEAD. For a publication, build and verify from the intended release commit, or use prebuilt outputs whose original source and hashes have been independently verified.

### Windows build from macOS

The local cross-build helper downloads its tools into `target/windows-tools`, snapshots the source, and builds optimized x64 MSVC executables without GitHub Actions:

```sh
python3 scripts/windows_cross_build.py --prepare --source-commit "<verified-commit>"
# Subsequent builds reuse the downloaded tools:
python3 scripts/windows_cross_build.py --source-commit "<verified-commit>"
```

Replace `<verified-commit>` with the full commit that will identify the release. The helper verifies every copied build input against that commit before building. It verifies the pinned GPUI crate and its shader sources, then reuses the exact compiled shader bytes from the previously verified Windows build. Its build-only dependency patch stays in the isolated snapshot; the application's source and root Cargo manifests are unchanged. See [shader provenance](../assets/windows-shaders/README.md).

Output is in `target/windows-release/x86_64-pc-windows-msvc/release`. `target/windows-release/build-provenance.json` records source and executable hashes, and a verified `--source-commit` automatically emits `target/windows-release/windows-prebuilt-manifest.json` for packaging. The helper uses a static C runtime. Validate the resulting binaries before packaging, then use that emitted manifest without changing its source commit or hashes:

```sh
python3 scripts/package.py --platform windows \
  --windows-build target/windows-release/x86_64-pc-windows-msvc/release \
  --windows-build-manifest target/windows-release/windows-prebuilt-manifest.json
```

## Git release branches and tags

`main` is ongoing development. `release/0.1` holds the 0.1 release line; subsequent minor versions receive their own `release/<major>.<minor>` branch. Tags are annotated and immutable. A release tag identifies the source actually built for both platforms.

```sh
git switch main
git status --short                 # Must be clean before choosing a release commit.
git switch -c release/0.1          # For the first 0.1 release only.
git push -u origin release/0.1

# Build, test, and package both platforms at the selected commit first.
git tag -a v0.1.0 -m "Asset Forge 0.1.0"
git push origin v0.1.0
```

For later patches, make the fix on `main`, carry it to the release branch with a reviewed cherry-pick or merge, set the patch version in Cargo manifests/lockfile, build both targets, and create the next tag. Never move a published tag or overwrite release assets with different binaries under the same version.

## Publish only verified artifacts

Create a release notes file stating the exact source commit, target architectures, checks run, and any limits. Describe the shipped interface accurately: Library and World provide previews and read-only catalogs; Forge chat creates and edits games, subjects, images and animations. Include SHA-256 checksums for the four distribution files. First create a draft, review its attached payload, then publish it. `--verify-tag` requires the tag to exist on GitHub; there is no deployment or build workflow behind this command.

```sh
gh release create v0.1.0 --repo Gio-Bruno/game-asset-generator \
  --verify-tag --draft --title "Asset Forge 0.1.0" \
  --notes-file /absolute/path/to/release-notes.md \
  dist/Asset-Forge-macOS-arm64.dmg \
  dist/Asset-Forge-Windows-x64-Setup.exe \
  dist/Asset-Forge-macOS.zip dist/Asset-Forge-Windows.zip \
  dist/SHA256SUMS.txt

gh release view v0.1.0 --repo Gio-Bruno/game-asset-generator
gh release edit v0.1.0 --repo Gio-Bruno/game-asset-generator --draft=false --latest
```

After publishing, verify the release page and download links, and compare downloaded artifact hashes with `SHA256SUMS.txt`. Packaging and publishing remain separate operations; `scripts/package.py` does not push Git branches, create tags, upload artifacts, or change repository visibility.
