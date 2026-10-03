#!/usr/bin/env python3
"""Build a macOS .app or Windows portable zip, then replace local artifacts safely."""
import argparse
import pathlib
import plistlib
import shutil
import subprocess
import sys
import tempfile
import zipfile

root = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("--profile", choices=["debug", "release"], default="release")
parser.add_argument("--no-build", action="store_true")
args = parser.parse_args()
if not args.no_build:
    command = ["cargo", "build", "--locked", "--workspace"]
    if args.profile == "release":
        command.append("--release")
    subprocess.run(command, cwd=root, check=True)
dist = root / "dist"
dist.mkdir(exist_ok=True)
build = root / "target" / args.profile

with tempfile.TemporaryDirectory(prefix=".package-", dir=dist) as temporary:
    stage = pathlib.Path(temporary)
    if sys.platform == "darwin":
        staged_app = stage / "Asset Forge.app"
        contents = staged_app / "Contents"
        (contents / "MacOS").mkdir(parents=True)
        resources = contents / "Resources"
        resources.mkdir()
        for name in ["asset-forge-studio", "asset-forge"]:
            shutil.copy2(build / name, contents / "MacOS" / name)
        shutil.copy2(root / "README.md", resources / "README.md")
        shutil.copy2(root / "LICENSE", resources / "LICENSE")
        shutil.copytree(root / "docs", resources / "docs")
        shutil.copytree(root / "examples", resources / "examples")
        legal = resources / "licenses"
        legal.mkdir()
        for name in ["IBM-Plex-OFL.txt", "Lora-OFL.txt"]:
            shutil.copy2(root / "assets/fonts" / name, legal / name)
        with (contents / "Info.plist").open("wb") as stream:
            plistlib.dump({
                "CFBundleName": "Asset Forge", "CFBundleDisplayName": "Asset Forge",
                "CFBundleIdentifier": "dev.assetforge.studio", "CFBundleExecutable": "asset-forge-studio",
                "CFBundleVersion": "1", "CFBundleShortVersionString": "0.1.0", "CFBundlePackageType": "APPL",
                "LSMinimumSystemVersion": "12.0", "NSHighResolutionCapable": True,
            }, stream)
        subprocess.run(["codesign", "--force", "--deep", "--sign", "-", str(staged_app)], check=True)
        app = dist / "Asset Forge.app"
        previous = stage / "previous.app"
        if app.exists():
            app.rename(previous)
        try:
            staged_app.rename(app)
        except BaseException:
            if previous.exists():
                previous.rename(app)
            raise
        # Replace paths, never truncate an executable that may still be running.
        shutil.copy2(build / "asset-forge", stage / "asset-forge")
        (stage / "asset-forge").replace(dist / "asset-forge")
        staged_zip = stage / "Asset-Forge-macOS.zip"
        subprocess.run(["ditto", "-c", "-k", "--keepParent", str(app), str(staged_zip)], check=True)
        staged_zip.replace(dist / staged_zip.name)
        print(app)
    elif sys.platform == "win32":
        staged_zip = stage / "Asset-Forge-Windows.zip"
        with zipfile.ZipFile(staged_zip, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for name in ["asset-forge-studio.exe", "asset-forge.exe"]:
                archive.write(build / name, name)
            for directory in ["docs", "examples"]:
                for path in (root / directory).rglob("*"):
                    if path.is_file():
                        archive.write(path, path.relative_to(root))
            archive.write(root / "README.md", "README.md")
            archive.write(root / "LICENSE", "LICENSE")
            for name in ["IBM-Plex-OFL.txt", "Lora-OFL.txt"]:
                archive.write(root / "assets/fonts" / name, "licenses/" + name)
        staged_zip.replace(dist / staged_zip.name)
        print(dist / staged_zip.name)
    else:
        parser.error("Desktop packaging targets macOS and Windows.")
