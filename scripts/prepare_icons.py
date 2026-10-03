#!/usr/bin/env python3
"""Convert the original app artwork into platform icon containers (requires Pillow)."""
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
BRANDING = ROOT / "assets/branding"


def main():
    with Image.open(BRANDING / "app-icon.png") as source:
        if source.width != source.height:
            raise ValueError("The app icon master must be square.")
        image = source.convert("RGBA")
        if image.getchannel("A").getextrema() != (0, 255):
            raise ValueError("The app icon master must preserve transparent outer margins.")
        # Only container/size conversion; the generated design and alpha are preserved.
        image = image.resize((1024, 1024), Image.Resampling.LANCZOS)
        image.save(BRANDING / "AssetForge.icns", format="ICNS")
        image.save(
            BRANDING / "AssetForge.ico", format="ICO", bitmap_format="bmp",
            sizes=[(size, size) for size in (16, 20, 24, 32, 40, 48, 64, 96, 128, 256)],
        )
    print("Prepared AssetForge.icns and AssetForge.ico from app-icon.png.")


if __name__ == "__main__":
    main()
