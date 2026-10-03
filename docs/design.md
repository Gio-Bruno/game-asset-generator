# Asset Forge design

Asset Forge is a native 2D art workshop with an editorial, paper-and-forest palette. IBM Plex Sans provides clear controls; Lora gives the workspace a warmer voice. The primary surfaces are a large canvas, a compact asset composer, and a persistent art director.

## Mobbin references

The connected Mobbin library was searched for AI image workspaces and visual style selectors. Actual screen images were inspected before adopting these patterns:

- [Gamma — visual style selection](https://mobbin.com/screens/f059805b-ca58-4470-9c3e-0a27988a1621): selectable visual directions, concise labels, and details behind the initial choice.
- [Runway — creation workspace](https://mobbin.com/screens/51e747f2-bd84-4894-97f3-c8c4fbbf0a88): a dominant preview, compact input, and references kept close to creation.
- [Adobe Firefly — style presets](https://mobbin.com/screens/abe1d051-c3fa-4fa4-83ee-69411d615506): thumbnail-first choices and an asset gallery.
- [Leonardo — image generation](https://mobbin.com/screens/11d095a1-1545-4002-9d49-702e80c71174): sensible defaults and direct creation controls.

These references inform the interaction patterns. The app's typography, palette, layout, illustrations, and implementation are original. The woodland preview is an actual Codex-generated example included in this repository. Other preset thumbnails are original schematic SVG illustrations, intended to communicate a direction rather than guarantee an exact generated result.

## Guided flow

New users can pick a visual preset or choose a game concept in Forge's panel. Forge can establish the style, customize its palette, camera and lighting, and develop the initial cast without making the user complete a long form. It sees only the current project's app context, and acts through typed application tools. Successful actions immediately refresh the visible workspace.

A user can then ask for an asset in chat or use the compact manual composer. Characters default to a square transparent PNG; scenes default to a 1536 × 1024 opaque PNG. Pixel art defaults to 256 pixels; Painterly fantasy sprites default to 1024 pixels; other manual sprites default to 512 pixels. Starter defaults survive style customization. Output settings and custom art-direction fields are available through disclosure controls.

The guide may create one image job per permitted user message. Setup suggestions permit no generation. It never automatically repeats uncertain actions. Chat persists in the local workspace, with streaming text and visible action receipts. The generation queue continues independently after the guide explains that an image is rendering.

Projects preserve a saved art direction; characters preserve text identity and pinned image references. Each job snapshots those rules. Developers can review a result, pin it to a style or character, then request another pose or a matching scene. The library stays local and exports PNGs through a native file dialog.

## Sprite workshop

Animate keeps the large preview and compact composer pattern. Six motion choices set frame count, grid, timing and loop defaults. Character chips select a saved identity, and generation attaches its pinned pixels alongside the project style. A single optional motion brief adds direction without requiring a form.

Clips have a playback preview, pause, frame stepping and atlas view. FPS and looping stay visible as small controls. Clip settings disclose frame count, columns, standard cell sizes, rectangular dimensions, margin and spacing. The main view starts with motion, character and one optional brief. Canvas and Animate both offer direct image-reference import with removable thumbnail chips. Import fits the current preset grid automatically; Extract frames creates a clip without spending generation capacity. Fit selected sheet and Clip settings support other grids. Timing changes regenerate only the preview; the image job's original snapshot stays intact.

The native export dialog writes a portable ZIP containing an atlas, individual PNG frames, GIF preview and JSON metadata. The AI guide uses the same typed animation operations and shared generation allowance.
