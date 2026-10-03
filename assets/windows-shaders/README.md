# GPUI Windows shaders

These 16 unchanged DXBC shader blobs come from the verified GPUI 0.2.2 Windows
release of Asset Forge, built from source revision `8612085`. The linked check
opened a responsive Windows window. Reflection identifies each shader module
and vertex/fragment stage; `scripts/extract_windows_shaders.py` rejects missing,
duplicate or unknown modules.

`provenance.json` records the original executable hash, official crate checksum,
relevant source hashes, individual shader hashes and verification URL. The
cross-build script verifies them before use. The shader sources and compiled
derivatives are licensed under GPUI's Apache License 2.0; see `LICENSE-APACHE`.

GPUI 0.2.2's build script compiles Windows shaders only on a Windows host. The
macOS cross-build applies a build-only patch to an isolated crate copy, providing
these already compiled shaders. Rendering source and shader bytes are unchanged.
The optimized Windows app receives the original GPUI DPI/common-controls manifest.

To replace these assets after changing GPUI, build and verify the new version on
Windows and run the extraction helper with that executable and official crate.
Do not silently reuse these bytes after changing the pinned shader sources.
