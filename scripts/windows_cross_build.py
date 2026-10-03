#!/usr/bin/env python3
"""Build optimized Windows MSVC binaries on macOS, with project-local tools.

Run with --prepare once to download Rust, cargo-xwin and the Microsoft SDK.
The root source and Cargo.lock stay unchanged: a build-only GPUI patch is applied
to an isolated source copy. The pinned, verified shaders retain native GPUI
rendering; neither debug mode nor Wine is required.
"""
import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import struct
import subprocess
import tarfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TOOLS = ROOT / "target/windows-tools"
SOURCE = ROOT / "target/windows-source"
OUTPUT = ROOT / "target/windows-release"
RUST_VERSION = "1.95.0"
TARGET = "x86_64-pc-windows-msvc"
SDK_VERSION = "10.0.26100"
SHADERS = ROOT / "assets/windows-shaders"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def run(command, env, cwd=ROOT):
    subprocess.run([str(value) for value in command], cwd=cwd, env=env, check=True)


def prepare_tools(prepare):
    if platform.system() != "Darwin":
        raise SystemExit("This helper targets a macOS host; Windows can build natively.")
    arch = "aarch64" if platform.machine() == "arm64" else "x86_64"
    host = f"{arch}-apple-darwin"
    toolchain = TOOLS / f"rustup/toolchains/{RUST_VERSION}-{host}"
    env = os.environ.copy()
    env.update(CARGO_HOME=str(TOOLS / "cargo"), RUSTUP_HOME=str(TOOLS / "rustup"),
               XWIN_CACHE_DIR=str(TOOLS / "xwin-cache"))
    TOOLS.mkdir(parents=True, exist_ok=True)
    if prepare:
        rustup = shutil.which("rustup")
        if not rustup:
            raise SystemExit("Install rustup first, then rerun with --prepare.")
        run([rustup, "toolchain", "install", RUST_VERSION, "--profile", "minimal",
             "--target", TARGET, "--component", "llvm-tools-preview", "--no-self-update"], env)
        uv = shutil.which("uv")
        if uv:
            run([uv, "pip", "install", "--python", shutil.which("python3"), "--target",
                 TOOLS / "python", "--cache-dir", TOOLS / "uv-cache", "cargo-xwin==0.23.1"], env)
        else:
            run([shutil.which("python3"), "-m", "pip", "install", "--target", TOOLS / "python",
                 "--cache-dir", TOOLS / "pip-cache", "cargo-xwin==0.23.1"], env)
    xwin = TOOLS / "python/bin/cargo-xwin"
    if not (toolchain / "bin/cargo").is_file() or not xwin.is_file():
        raise SystemExit("Project-local toolchain missing. Run with --prepare.")
    llvm = toolchain / f"lib/rustlib/{host}/bin"
    bin_dir = TOOLS / "bin"
    bin_dir.mkdir(exist_ok=True)
    compiler = bin_dir / "clang-cl"
    compiler.write_text('#!/bin/sh\nexec /usr/bin/clang --driver-mode=cl "$@"\n')
    compiler.chmod(0o755)
    for name, source in [("lld-link", "rust-lld"), ("llvm-lib", "llvm-ar"), ("llvm-ar", "llvm-ar")]:
        destination = bin_dir / name
        if destination.is_symlink():
            destination.unlink()
        destination.symlink_to(llvm / source)
    env["PATH"] = os.pathsep.join([str(bin_dir), str(toolchain / "bin"), str(TOOLS / "python/bin"), env["PATH"]])
    # Cache immutable crate archives locally; never read Cargo credentials/config.
    original = Path.home() / ".cargo/registry/cache"
    if original.is_dir():
        for directory in original.iterdir():
            if directory.is_dir():
                destination = TOOLS / "cargo/registry/cache" / directory.name
                destination.mkdir(parents=True, exist_ok=True)
                for archive in directory.glob("*.crate"):
                    output = destination / archive.name
                    if not output.exists():
                        try:
                            os.link(archive, output)
                        except OSError:
                            shutil.copy2(archive, output)
    if prepare:
        run([xwin, "cache", "xwin", "--xwin-sdk-version", SDK_VERSION, "--xwin-version", "17"], env)
    return xwin, env


def prepare_gpui(env):
    provenance = json.loads((SHADERS / "provenance.json").read_text())
    gpui = TOOLS / "gpui-0.2.2"
    archive = TOOLS / "gpui-0.2.2.crate"
    if not archive.exists():
        urllib.request.urlretrieve("https://static.crates.io/crates/gpui/gpui-0.2.2.crate", archive)
    if sha(archive.read_bytes()) != provenance["gpuiCrateSha256"]:
        raise SystemExit("GPUI crate checksum does not match verified shader provenance.")
    if gpui.exists():
        shutil.rmtree(gpui)
    with tarfile.open(archive) as package:
        for member in package.getmembers():
            path = Path(member.name)
            if path.is_absolute() or ".." in path.parts or member.issym() or member.islnk():
                raise SystemExit("Unexpected path or symlink in GPUI crate archive.")
        package.extractall(TOOLS)
    for relative, expected in provenance["sourceSha256"].items():
        if sha((gpui / relative).read_bytes()) != expected:
            raise SystemExit(f"GPUI shader source changed: {relative}")
    bindings = []
    for name, expected in provenance["shaderSha256"].items():
        data = (SHADERS / name).read_bytes()
        if sha(data) != expected or data[:4] != b"DXBC":
            raise SystemExit(f"Compiled shader checksum mismatch: {name}")
        constant = name.removesuffix(".dxbc").upper() + "_BYTES"
        # Byte literals make the generated module independent of local paths.
        bindings.append(f"const {constant}: &[u8] = &{list(data)};\n")
    compiled = TOOLS / "shaders_bytes.rs"
    compiled.write_text("".join(bindings))
    env["ASSET_FORGE_WINDOWS_SHADERS"] = str(compiled)
    build = gpui / "build.rs"
    original = build.read_text()
    needle = '        Ok("windows") => {\n            #[cfg(target_os = "windows")]\n            windows::build();\n        }'
    replacement = '''        Ok("windows") => {
            #[cfg(target_os = "windows")]
            windows::build();
            #[cfg(not(target_os = "windows"))]
            {
                let source = env::var("ASSET_FORGE_WINDOWS_SHADERS")
                    .expect("Verified GPUI Windows shaders required");
                let output = std::path::PathBuf::from(env::var("OUT_DIR").unwrap())
                    .join("shaders_bytes.rs");
                std::fs::copy(&source, output).expect("Copy verified Windows shader bytes");
                println!("cargo:rerun-if-changed={source}");
            }
        }'''
    if original.count(needle) != 1:
        raise SystemExit("GPUI build script changed; refusing to apply the cross-build patch.")
    build.write_text(original.replace(needle, replacement))
    return gpui


def snapshot_source(gpui):
    if SOURCE.exists():
        shutil.rmtree(SOURCE)
    SOURCE.mkdir(parents=True)
    included = ["Cargo.toml", "Cargo.lock", "crates", "assets", "examples"]
    records = {}
    for name in included:
        original = ROOT / name
        output = SOURCE / name
        if original.is_dir():
            shutil.copytree(original, output)
            paths = sorted(original.rglob("*"))
        else:
            shutil.copy2(original, output)
            paths = [original]
        for path in paths:
            if path.is_file():
                records[str(path.relative_to(ROOT))] = sha(path.read_bytes())
    manifest = SOURCE / "Cargo.toml"
    manifest.write_text(manifest.read_text() + f'\n[patch.crates-io]\ngpui = {{ path = {json.dumps(str(gpui))} }}\n')
    studio_manifest = SOURCE / "crates/forge-studio/Cargo.toml"
    studio_manifest.write_text(studio_manifest.read_text().replace('[package]\n', '[package]\nbuild = "windows_build.rs"\n', 1))
    build_script = SOURCE / "crates/forge-studio/windows_build.rs"
    manifest_data = (gpui / "resources/windows/gpui.manifest.xml").read_bytes()
    resource = TOOLS / "gpui-manifest.res"
    # A standard Windows .res record avoids invoking Windows-only mt.exe.
    # lld-link reads .res directly and embeds the original GPUI manifest.
    null_header = struct.pack("<IIHHHHIHHII", 0, 32, 0xffff, 0, 0xffff, 0, 0, 0, 0, 0, 0)
    header = struct.pack("<IIHHHHIHHII", len(manifest_data), 32, 0xffff, 24, 0xffff, 1, 0, 0x30, 0x409, 0, 0)
    resource.write_bytes(null_header + header + manifest_data + b"\0" * (-len(manifest_data) % 4))
    build_script.write_text('fn main() {\n'
                            '    println!("cargo:rustc-link-arg-bin=asset-forge-studio=/manifest:no");\n'
                            f'    println!("cargo:rustc-link-arg-bin=asset-forge-studio={resource}");\n'
                            '}\n')
    return records


def committed_source(records, requested_commit):
    if not requested_commit:
        return None
    commit = subprocess.check_output(["git", "rev-parse", requested_commit], cwd=ROOT, text=True).strip()
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise SystemExit("Expected a resolved Git source commit.")
    for relative, expected in records.items():
        data = subprocess.check_output(["git", "show", f"{commit}:{relative}"], cwd=ROOT)
        if sha(data) != expected:
            raise SystemExit(f"Source snapshot differs from the requested commit: {relative}")
    return commit


def validate_lock():
    def packages(text):
        entries = {}
        for block in text.split("[[package]]")[1:]:
            name = re.search(r'^name = "([^"]+)"', block, re.M).group(1)
            version = re.search(r'^version = "([^"]+)"', block, re.M).group(1)
            if name == "gpui":
                block = re.sub(r'^(source|checksum) = .*\n', '', block, flags=re.M)
            entries[name, version] = block.strip()
        return entries
    if packages((ROOT / "Cargo.lock").read_text()) != packages((SOURCE / "Cargo.lock").read_text()):
        raise SystemExit("Cross-build changed dependencies beyond the pinned GPUI path patch.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepare", action="store_true", help="Download/install only inside target/windows-tools.")
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--source-commit", help="Verify all copied inputs against this commit and emit a release manifest.")
    args = parser.parse_args()
    xwin, env = prepare_tools(args.prepare)
    gpui = prepare_gpui(env)
    records = snapshot_source(gpui)
    source_commit = committed_source(records, args.source_commit)
    command = [xwin, "build", "--manifest-path", SOURCE / "Cargo.toml", "--workspace", "--release",
               "--target", TARGET, "--target-dir", OUTPUT, "--jobs", args.jobs,
               "--xwin-sdk-version", SDK_VERSION, "--xwin-version", "17",
               "--config", f'target.{TARGET}.rustflags=["-C", "target-feature=+crt-static"]']
    print("Building optimized Windows binaries from an isolated source snapshot.", flush=True)
    run(command, env)
    validate_lock()
    binaries = OUTPUT / TARGET / "release"
    report = {"target": TARGET, "rustVersion": RUST_VERSION,
              "cargoXwinVersion": "0.23.1", "sdkVersion": SDK_VERSION,
              "profile": "release", "staticCrt": True,
              "sourceTreeSha256": sha(json.dumps(records, sort_keys=True).encode()),
              "sourceFileSha256": records,
              "binarySha256": {name: sha((binaries / name).read_bytes())
                               for name in ["asset-forge-studio.exe", "asset-forge.exe"]},
              "shaderProvenance": json.loads((SHADERS / "provenance.json").read_text())}
    (OUTPUT / "build-provenance.json").write_text(json.dumps(report, indent=2) + "\n")
    if source_commit:
        version = re.search(r'^version = "([^"]+)"', (SOURCE / "Cargo.toml").read_text(), re.M).group(1)
        report["sourceCommit"] = source_commit
        (OUTPUT / "build-provenance.json").write_text(json.dumps(report, indent=2) + "\n")
        release = {"schemaVersion": 1, "version": version, "sourceCommit": source_commit,
                   "sha256": report["binarySha256"], "buildProvenance": "verified-local-cross-build"}
        (OUTPUT / "windows-prebuilt-manifest.json").write_text(json.dumps(release, indent=2) + "\n")
    print(binaries)


if __name__ == "__main__":
    main()
