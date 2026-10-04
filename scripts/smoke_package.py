#!/usr/bin/env python3
"""Verify a packaged CLI with an isolated workspace, without invoking Codex."""
import json
import pathlib
import subprocess
import sys
import tempfile
import zipfile

root = pathlib.Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="forge-package-smoke-") as temporary:
    temporary = pathlib.Path(temporary)
    if sys.platform == "win32":
        with zipfile.ZipFile(root / "dist/Asset-Forge-Windows.zip") as archive:
            archive.extractall(temporary / "package")
        cli = temporary / "package/asset-forge.exe"
        assert (temporary / "package/asset-forge-studio.exe").is_file()
    else:
        cli = root / "dist/asset-forge"

    def call(method, params):
        result = subprocess.run(
            [str(cli), "--data-dir", str(temporary / "data"), "call", method, json.dumps(params)],
            check=True, capture_output=True, text=True, timeout=20,
        )
        return json.loads(result.stdout)["result"]

    presets = call("styles/presets/list", {})
    assert len(presets) == 6
    project = call("projects/create", {"name": "Packaged workshop", "style": presets[0]["style"]})
    assert call("projects/get", {"id": project["id"]})["name"] == "Packaged workshop"
    motions = call("animations/presets/list", {})
    assert set(motions) == {"IDLE", "WALK", "RUN", "JUMP", "ATTACK", "HIT_REACTION", "DEATH", "CUSTOM"}
    print("Packaged CLI: six styles, eight motions, persistent project round trip passed.")
