# Live generation samples

Generated on 2026-10-03 through Asset Forge's CLI and Codex CLI 0.160.0 app-server, using native image generation with a Codex subscription.

- `mira-idle.png`: 512 × 512, actual transparent pixels. Character: chestnut hair, amber scarf, moss-green tunic, leather boots, round proportions.
- `woodland-scene.png`: 1536 × 1024, opaque. It reused the first image as both a style and character reference, with the same saved art direction and identity.

- `mira-guided.png`: 512 × 512 with actual alpha. A separate Fernlight test project was set up by the AI guide, then a chat request generated this sprite through the app’s queued image job.

- `mira-walk-atlas.png`, `mira-walk.gif`, `mira-walk.zip`: a real guide-generated Walk clip, six 512 × 512 transparent frames in a 1536 × 1024 atlas, 8 FPS and looping. The ZIP includes atlas, frames, GIF and timing metadata. Several poses repeat; inspect motion before using it in a game.

These are real backend outputs. They are not bundled into an empty user workspace as pretend generated history. Import them to try the reference workflow without generating a first asset.
