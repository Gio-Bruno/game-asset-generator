# Asset Forge application icon

The original artwork is `app-icon.png`. It was generated with the built-in image generation tool on 2026-10-03. The forest tile, ivory anvil and amber spark match Asset Forge's existing interface palette. The outer margin has real transparency.

`AssetForge.icns` supplies macOS Finder and Dock sizes. `AssetForge.ico` supplies Windows executable, taskbar, installer and shortcut sizes. Regenerate the containers with `python3 scripts/prepare_icons.py` after installing Pillow. This performs format and size conversion only, preserving the artwork and alpha.

The Windows build script embeds group icon resource ID 1, which GPUI loads for its native window, together with its component images. The binary resource encoding follows Microsoft's [resource file formats](https://learn.microsoft.com/en-us/windows/win32/menurc/resource-file-formats).

## Generation prompt

Use case: logo-brand. Asset type: a finished macOS and Windows desktop application icon for Asset Forge, a friendly native app that helps indie developers make consistent 2D game assets. Primary request: one polished icon, square 1024 by 1024 composition, no text or letters. A sturdy small ivory anvil with a single warm amber four-point spark above it, forming one memorable simple silhouette, centered on a deep forest-green rounded square tile. Refined tactile stylized 3D rendering with subtle bevels, soft studio lighting, crisp clean shapes, restrained detail. Colors match the app: forest green #315C4B, warm ivory #F1E9D5, amber #D7AD70. Tile occupies about 84 percent of the square canvas with generous transparent outer margins, rounded corners suitable for a macOS dock icon and Windows taskbar. The anvil and spark must read clearly at 32 pixels. Everything fully contained in the image. No scene, background objects, extra icons, logo grid, badges, mockup, lettering or watermark. Actual transparent alpha outside the rounded tile.
