#!/usr/bin/env python3
"""Manually package Asset Forge; no upload, release creation, or auto-update."""
import argparse
import hashlib
import json
import pathlib
import plistlib
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
WINDOWS_BINARIES = ("asset-forge-studio.exe", "asset-forge.exe")


def workspace_version(root):
    """Read the workspace version field without requiring Python 3.11."""
    source = (root / "Cargo.toml").read_text(encoding="utf-8")
    section = re.search(r"(?ms)^\[workspace\.package\]\s*\n(.*?)(?=^\[|\Z)", source)
    match = re.search(r'^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"\s*$', section.group(1), re.M) if section else None
    if not match:
        raise ValueError("Cargo.toml needs a numeric workspace.package version (major.minor.patch).")
    return match.group(1)


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def validate_windows_binary(path):
    with path.open("rb") as stream:
        header = stream.read(64)
        if len(header) < 64 or header[:2] != b"MZ":
            raise ValueError("Not a Windows executable: " + str(path))
        stream.seek(struct.unpack_from("<I", header, 60)[0])
        pe_header = stream.read(6)
        if pe_header[:4] != b"PE\0\0" or pe_header[4:] != b"\x64\x86":
            raise ValueError("Expected a Windows x64 executable: " + str(path))


def validated_prebuilt_manifest(path, build, version):
    manifest = json.loads(path.read_text(encoding="utf-8"))
    if manifest.get("schemaVersion") != 1 or manifest.get("version") != version:
        raise ValueError("Prebuilt manifest schema/version does not match this package.")
    if not re.fullmatch(r"[0-9a-fA-F]{40}", manifest.get("sourceCommit", "")):
        raise ValueError("Prebuilt manifest requires the actual 40-character build sourceCommit.")
    for name in WINDOWS_BINARIES:
        validate_windows_binary(build / name)
        if manifest.get("sha256", {}).get(name) != sha256(build / name):
            raise ValueError("Prebuilt binary does not match its manifest: " + name)
    return manifest


def copy_resources(root, destination):
    destination.mkdir(parents=True, exist_ok=True)
    for name in ("README.md", "LICENSE"):
        shutil.copy2(root / name, destination / name)
    for directory in ("docs", "examples"):
        shutil.copytree(root / directory, destination / directory)
    legal = destination / "licenses"
    legal.mkdir()
    for name in ("IBM-Plex-OFL.txt", "Lora-OFL.txt"):
        shutil.copy2(root / "assets" / "fonts" / name, legal / name)
    shutil.copy2(root / "assets/windows-shaders/LICENSE-APACHE", legal / "GPUI-Apache-2.0.txt")


def write_manifest(destination, version, platform, binaries, prebuilt=None, built=False):
    manifest = {
        "schemaVersion": 1, "version": version, "platform": platform,
        "sha256": {name: sha256(path) for name, path in binaries.items()},
        "buildProvenance": "packaging-build" if built else "existing-output",
    }
    # --no-build does not claim that existing binaries were built from current HEAD.
    if prebuilt:
        manifest["sourceCommit"] = prebuilt["sourceCommit"]
        manifest["buildProvenance"] = "verified-prebuilt-manifest"
        if "buildUrl" in prebuilt:
            manifest["buildUrl"] = prebuilt["buildUrl"]
    (destination / "release-manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )
    return manifest


def replace_app(staged_app, app, stage):
    previous = stage / "previous.app"
    if app.exists():
        app.rename(previous)
    try:
        staged_app.rename(app)
    except BaseException:
        if previous.exists():
            previous.rename(app)
        raise


def package_macos(args, build, stage, dist, version):
    if sys.platform != "darwin":
        raise ValueError("macOS app/DMG packaging requires macOS.")
    for name in ("asset-forge-studio", "asset-forge"):
        with (build / name).open("rb") as stream:
            header = stream.read(8)
        if header != b"\xcf\xfa\xed\xfe\x0c\x00\x00\x01":
            raise ValueError("The arm64 package requires Apple Silicon binaries: " + name)
    cli_version = subprocess.run(
        [str(build / "asset-forge"), "--version"], check=True,
        capture_output=True, text=True,
    ).stdout.strip().split()[-1]
    if cli_version != version:
        raise ValueError("Compiled CLI version does not match Cargo.toml.")
    staged_app = stage / "Asset Forge.app"
    contents = staged_app / "Contents"
    (contents / "MacOS").mkdir(parents=True)
    resources = contents / "Resources"
    copy_resources(ROOT, resources)
    for name in ("asset-forge-studio", "asset-forge"):
        shutil.copy2(build / name, contents / "MacOS" / name)
    with (contents / "Info.plist").open("wb") as stream:
        plistlib.dump({
            "CFBundleName": "Asset Forge", "CFBundleDisplayName": "Asset Forge",
            "CFBundleIdentifier": "dev.assetforge.studio", "CFBundleExecutable": "asset-forge-studio",
            "CFBundleVersion": version, "CFBundleShortVersionString": version,
            "CFBundlePackageType": "APPL", "LSMinimumSystemVersion": "12.0",
            "NSHighResolutionCapable": True,
        }, stream)
    subprocess.run(["codesign", "--force", "--deep", "--sign", "-", str(staged_app)], check=True)
    # Keep signed-binary hashes outside the bundle: embedding the main executable's
    # hash in a signed resource would create a circular signature/hash dependency.
    write_manifest(stage, version, "macos-arm64", {
        name: contents / "MacOS" / name for name in ("asset-forge-studio", "asset-forge")
    }, built=not args.no_build)
    app = dist / "Asset Forge.app"
    replace_app(staged_app, app, stage)
    # Replace paths; never truncate an executable that may still be running.
    shutil.copy2(build / "asset-forge", stage / "asset-forge")
    (stage / "asset-forge").replace(dist / "asset-forge")
    staged_zip = stage / "Asset-Forge-macOS.zip"
    subprocess.run(["ditto", "-c", "-k", "--keepParent", str(app), str(staged_zip)], check=True)
    with zipfile.ZipFile(staged_zip, "a", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.write(stage / "release-manifest.json", "release-manifest.json")
    staged_zip.replace(dist / staged_zip.name)
    if not args.portable_only:
        image_root = stage / "dmg"
        image_root.mkdir()
        shutil.copytree(app, image_root / app.name)
        (image_root / "Applications").symlink_to("/Applications", target_is_directory=True)
        shutil.copy2(stage / "release-manifest.json", image_root / "release-manifest.json")
        staged_dmg = stage / "Asset-Forge-macOS-arm64.dmg"
        subprocess.run([
            "hdiutil", "create", "-volname", "Asset Forge", "-srcfolder", str(image_root),
            "-format", "UDZO", "-ov", str(staged_dmg),
        ], check=True)
        staged_dmg.replace(dist / staged_dmg.name)
        print(dist / staged_dmg.name)
    shutil.copy2(stage / "release-manifest.json", stage / "macOS-release-manifest.json")
    (stage / "macOS-release-manifest.json").replace(dist / "macOS-release-manifest.json")
    print(app)


def nsis_quote(value):
    return str(value).replace("$", "$$").replace('"', '$\\"')


def write_uninstall_files(payload, destination):
    """Delete exactly the installed payload, preserving any additional user files."""
    files = sorted(path for path in payload.rglob("*") if path.is_file())
    directories = sorted(
        (path for path in payload.rglob("*") if path.is_dir()),
        key=lambda path: len(path.parts), reverse=True,
    )
    lines = [
        'Delete "$INSTDIR\\{}"'.format(nsis_quote(path.relative_to(payload)).replace("/", "\\"))
        for path in files
    ]
    lines += [
        'RMDir "$INSTDIR\\{}"'.format(nsis_quote(path.relative_to(payload)).replace("/", "\\"))
        for path in directories
    ]
    destination.write_text("\n".join(lines) + "\n", encoding="utf-8")


def package_windows(args, build, stage, dist, version):
    prebuilt = None
    if args.windows_build:
        if not args.windows_build_manifest:
            raise ValueError("--windows-build requires --windows-build-manifest for version/source/hash verification.")
        prebuilt = validated_prebuilt_manifest(args.windows_build_manifest, build, version)
    elif sys.platform != "win32":
        raise ValueError("On macOS, Windows packaging requires --windows-build and its build manifest.")
    for name in WINDOWS_BINARIES:
        validate_windows_binary(build / name)
    if sys.platform == "win32":
        cli_version = subprocess.run(
            [str(build / "asset-forge.exe"), "--version"], check=True,
            capture_output=True, text=True,
        ).stdout.strip().split()[-1]
        if cli_version != version:
            raise ValueError("Compiled CLI version does not match Cargo.toml.")
    payload = stage / "windows"
    copy_resources(ROOT, payload)
    for name in WINDOWS_BINARIES:
        shutil.copy2(build / name, payload / name)
    manifest = write_manifest(payload, version, "windows-x64", {
        name: payload / name for name in WINDOWS_BINARIES
    }, prebuilt=prebuilt, built=not args.no_build and not args.windows_build)
    staged_zip = stage / "Asset-Forge-Windows.zip"
    with zipfile.ZipFile(staged_zip, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(payload.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(payload))
    staged_zip.replace(dist / staged_zip.name)
    print(dist / staged_zip.name)
    if not args.portable_only:
        makensis = args.makensis or shutil.which("makensis")
        if not makensis:
            raise ValueError("Windows installer needs NSIS (makensis). Install it locally or choose --portable-only.")
        uninstall_files = stage / "uninstall-files.nsh"
        write_uninstall_files(payload, uninstall_files)
        staged_installer = stage / "Asset-Forge-Windows-x64-Setup.exe"
        define = "/D" if sys.platform == "win32" else "-D"
        subprocess.run([
            str(makensis),
            ("/V2" if sys.platform == "win32" else "-V2"),
            ("/WX" if sys.platform == "win32" else "-WX"),
            define + "APP_VERSION=" + version,
            define + "PACKAGE_DIR=" + str(payload),
            define + "OUTPUT_FILE=" + str(staged_installer),
            define + "UNINSTALL_FILES=" + str(uninstall_files),
            str(ROOT / "scripts" / "windows-installer.nsi"),
        ], check=True)
        staged_installer.replace(dist / staged_installer.name)
        print(dist / staged_installer.name)
    # Separate manifest supports later packaging from the same verified binaries.
    (dist / "windows-release-manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=["debug", "release"], default="release")
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--platform", choices=["macos", "windows"])
    parser.add_argument("--portable-only", action="store_true", help="Create app/ZIP without DMG or NSIS installer.")
    parser.add_argument("--windows-build", type=pathlib.Path, help="Directory with prebuilt Windows x64 EXEs; implies --no-build.")
    parser.add_argument("--windows-build-manifest", type=pathlib.Path, help="JSON with version, sourceCommit, and both binary hashes.")
    parser.add_argument("--makensis", type=pathlib.Path, help="Explicit path to the NSIS compiler.")
    args = parser.parse_args()
    platform = args.platform or ({"darwin": "macos", "win32": "windows"}.get(sys.platform))
    if not platform:
        parser.error("Select --platform windows with verified prebuilt binaries, or package on macOS/Windows.")
    if args.windows_build:
        if platform != "windows":
            parser.error("--windows-build requires --platform windows.")
        args.no_build = True
    if args.windows_build_manifest and not args.windows_build:
        parser.error("--windows-build-manifest requires --windows-build.")
    version = workspace_version(ROOT)
    if not args.no_build:
        command = ["cargo", "build", "--locked", "--workspace"]
        if args.profile == "release":
            command.append("--release")
        subprocess.run(command, cwd=ROOT, check=True)
    dist = ROOT / "dist"
    dist.mkdir(exist_ok=True)
    build = (args.windows_build or (ROOT / "target" / args.profile)).resolve()
    with tempfile.TemporaryDirectory(prefix=".package-", dir=dist) as temporary:
        stage = pathlib.Path(temporary)
        try:
            if platform == "macos":
                package_macos(args, build, stage, dist, version)
            else:
                package_windows(args, build, stage, dist, version)
        except (ValueError, OSError) as error:
            parser.exit(1, "Packaging failed: " + str(error) + "\n")


if __name__ == "__main__":
    main()
