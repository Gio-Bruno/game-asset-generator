# Asset Forge design

Asset Forge is a native 2D art workshop with an editorial, paper-and-forest palette. IBM Plex Sans provides clear controls; Lora gives the workspace a warmer voice. A vertical game-and-asset sidebar, a dominant preview and a persistent Forge chat are the primary surfaces. The selected game’s editable name is above its Library and World tabs; those views belong to that game. The chat handles creation and editing; Library and World make the resulting work easy to inspect.

## Mobbin references

The connected Mobbin library was searched for AI image workspaces and visual style selectors. Actual screen images were inspected before adopting these patterns:

- [Gamma — visual style selection](https://mobbin.com/screens/f059805b-ca58-4470-9c3e-0a27988a1621): selectable visual directions, concise labels, and details behind the initial choice.
- [Runway — creation workspace](https://mobbin.com/screens/51e747f2-bd84-4894-97f3-c8c4fbbf0a88): a dominant preview, compact input, and references kept close to creation.
- [Adobe Firefly — style presets](https://mobbin.com/screens/abe1d051-c3fa-4fa4-83ee-69411d615506): thumbnail-first choices and an asset gallery.
- [Leonardo — image generation](https://mobbin.com/screens/11d095a1-1545-4002-9d49-702e80c71174): sensible defaults and direct creation controls.

These references inform the interaction patterns. The app's typography, palette, layout, illustrations, and implementation are original. The woodland preview is an actual Codex-generated example included in this repository. Other preset thumbnails are original schematic SVG illustrations, intended to communicate a direction rather than guarantee an exact generated result.

## Browse and create

New game opens a fresh Forge conversation. A developer describes the game, and Forge first asks about a missing creative decision using short selectable pills. Questions support a typed answer and Skip. Forge asks one question at a time, reuses supplied details, and resumes the original task after the answer. New-game creation is blocked by the backend until a setup question is answered or skipped; all mutations are blocked while any question is pending. A tower-defense concept needs structures in its catalog; it does not need an invented main character. Explicitly requesting a separate game in an existing conversation creates and selects a new project while preserving the previous game.

World shows the saved art direction and Characters, Structures and Props. These read-only summaries let developers check what Forge actually saved. The game name has a direct Rename/Save/Cancel control, and Forge can rename it through chat. Subject names, descriptions, categories, palette, camera and lighting are edited through chat. Library shows read-only image and animation lists with a large selected preview, browsing controls and native export dialogs. The sidebar lists games first and the current game’s assets below them. Game, image and clip deletion requires a confirmation dialog; the footer offers Undo. Deleted records and files are retained locally, hidden from reads/lists until restored. Deleting an atlas also hides its dependent clips and unpins that image; Undo restores the image and clips without silently repinning references. Active guide or generation work blocks deletion. Creation forms are removed from these surfaces.

The guide acts through typed app tools for project creation, style changes, saved subject creation/editing, reference pinning, image generation, animation generation, sheet extraction and timing changes. Successful actions refresh the visible game and catalog. Characters, structures and props share saved text identity and pinned image references; each image job snapshots those rules. Older character records default to the Character category without rewriting user data.

The developer can ask for a sprite, structure, prop or matching scene without filling in technical settings. Static subjects default to transparent square PNGs; scenes default to a 1536 × 1024 opaque PNG. Pixel art uses smaller sprite defaults and Painterly fantasy uses larger ones. Starter dimensions survive style customization; explicit chat instructions can override output settings through the same backend contract.

The guide may queue one generation request per permitted user message: one image or, for a request involving several assets, one batch of 2–12 separate subject images. Every character, structure, prop and environment has its own named file and saved identity. A concept sheet is generated only when explicitly requested and displayed as a reference; it never substitutes for usable production assets. Sprite sheet cells belong to one animated identity. The sidebar lists pending subjects individually with rendering/queued state and cancellation. Completed images can be viewed, exported and added to chat independently. The first valid isolated image becomes its subject's initial visual reference, preserving existing pins.

Setup suggestions permit no generation. The batch receipt and jobs commit atomically, and replay never queues them again. Chat persists locally with streaming text and visible action receipts; rendering continues independently after Forge reports the queued jobs. Each failed job remains inspectable while the other items continue.

## Image revisions

Add to chat attaches a library asset to the next Forge message. + Image imports a reference through a native file picker and adds it to the same removable attachment chips. The guide receives the actual image pixels and saved asset metadata, and the generation automatically inherits that message's attachments. The developer can request a concrete change while keeping the selected subject's identity and project style.

Attachments belong to the current message and project. Subsequent messages do not silently inherit them; developers can attach them again or ask Forge to pin them as persistent style or subject references. At most eight images can be combined for a generation. Creating a separate game copies only explicitly attached current-message images into its new library and preserves the old game.

## Animation review

Forge handles Idle, Walk, Run, Jump, Attack and custom animation requests using saved subjects and motion/style defaults. It can also extract frames from an attached sprite sheet using the requested grid, margins and spacing, then change FPS or looping without generating another image. These creation and editing operations happen in chat.

Library provides playback, pause, frame stepping and atlas view for completed clips. The native export dialog writes a portable ZIP containing the atlas, individual PNG frames, GIF preview and JSON timing metadata. Motion quality remains approximate and needs visual review before engine import. The animation operations share the guide's generation allowance and preserve immutable image-job snapshots.
