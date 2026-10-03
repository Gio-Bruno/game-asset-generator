#!/usr/bin/env python3
"""Recover pinned GPUI DXBC from a verified Windows release, for cross-building.

No shader is recompiled or modified. Reflection identifies all eight modules and
both shader stages; extraction fails unless there is exactly one of each.
"""
import argparse
import hashlib
import json
import re
import struct
import tarfile
from pathlib import Path


def digest(data):
    return hashlib.sha256(data).hexdigest()


def reflected_resources(chunk):
    count, offset = struct.unpack_from("<II", chunk, 8)
    if count > 128 or offset + count * 32 > len(chunk):
        raise ValueError("Invalid DXBC reflection table")
    names = set()
    for index in range(count):
        name_offset = struct.unpack_from("<I", chunk, offset + index * 32)[0]
        name = chunk[name_offset:chunk.index(b"\0", name_offset)].decode("ascii")
        names.add(name)
    return names


def identify(stage, resources):
    for resource, module in [
        ("quads", "quad"), ("shadows", "shadow"),
        ("underlines", "underline"),
        ("path_rasterization_sprites", "path_rasterization"),
        ("path_sprites", "path_sprite"),
        ("mono_sprites", "monochrome_sprite"),
        ("poly_sprites", "polychrome_sprite"),
        ("t_layer", "emoji_rasterization"),
    ]:
        if resource in resources:
            return module
    if stage == "vertex" and not resources:
        return "emoji_rasterization"
    if stage == "fragment" and resources == {"s_sprite", "t_sprite"}:
        return "path_sprite"
    if stage == "fragment" and resources == {"s_sprite", "t_sprite", "GlobalParams"}:
        return "monochrome_sprite"
    raise ValueError(f"Unrecognized shader: {stage}, {sorted(resources)}")


def extract(binary):
    shaders = {}
    for match in re.finditer(b"DXBC", binary):
        start = match.start()
        if start + 32 > len(binary):
            continue
        reserved, size, count = struct.unpack_from("<III", binary, start + 20)
        if reserved != 1 or not 32 <= size <= 1_000_000 or not 1 <= count <= 32:
            continue
        if start + size > len(binary):
            continue
        blob = binary[start:start + size]
        resources = None
        stage = None
        for index in range(count):
            offset = struct.unpack_from("<I", blob, 32 + index * 4)[0]
            if offset + 8 > size:
                raise ValueError("Invalid DXBC chunk offset")
            kind, length = struct.unpack_from("<4sI", blob, offset)
            chunk = blob[offset + 8:offset + 8 + length]
            if len(chunk) != length:
                raise ValueError("Invalid DXBC chunk size")
            if kind == b"RDEF":
                resources = reflected_resources(chunk)
            if kind in (b"SHDR", b"SHEX"):
                shader_type = struct.unpack_from("<I", chunk)[0] >> 16
                stage = {0: "fragment", 1: "vertex"}.get(shader_type)
        if resources is None or stage is None:
            raise ValueError("Shader stage or reflection missing")
        name = f"{identify(stage, resources)}_{stage}.dxbc"
        if name in shaders:
            raise ValueError(f"Duplicate shader {name}")
        shaders[name] = blob
    modules = ["quad", "shadow", "underline", "path_rasterization", "path_sprite",
               "monochrome_sprite", "polychrome_sprite", "emoji_rasterization"]
    expected = {f"{module}_{stage}.dxbc" for module in modules for stage in ["vertex", "fragment"]}
    if set(shaders) != expected:
        raise ValueError("Missing or unexpected compiled GPUI shaders")
    return shaders


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", required=True, type=Path)
    parser.add_argument("--crate", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--verification-url", required=True)
    args = parser.parse_args()
    archive = args.crate.read_bytes()
    binary = args.exe.read_bytes()
    shaders = extract(binary)
    sources = ["build.rs", "src/platform/windows/shaders.hlsl",
               "src/platform/windows/alpha_correction.hlsl",
               "src/platform/windows/color_text_raster.hlsl",
               "src/platform/windows/directx_renderer.rs"]
    with tarfile.open(args.crate) as crate:
        hashes = {name: digest(crate.extractfile(f"gpui-0.2.2/{name}").read()) for name in sources}
        license_text = crate.extractfile("gpui-0.2.2/LICENSE-APACHE").read()
    args.output.mkdir(parents=True, exist_ok=True)
    for name, data in shaders.items():
        (args.output / name).write_bytes(data)
    (args.output / "LICENSE-APACHE").write_bytes(license_text)
    provenance = {
        "gpuiVersion": "0.2.2", "gpuiCrateSha256": digest(archive),
        "sourceCommit": args.source_commit,
        "verificationUrl": args.verification_url,
        "verifiedBinarySha256": digest(binary), "sourceSha256": hashes,
        "shaderSha256": {name: digest(data) for name, data in sorted(shaders.items())},
    }
    (args.output / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
    print(f"Extracted {len(shaders)} unchanged shaders with source and binary provenance.")


if __name__ == "__main__":
    main()
