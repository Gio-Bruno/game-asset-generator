//! Sprite clips share the generation queue and identity snapshots with static assets.
use crate::{
    Service,
    contract::*,
    error::{ApiError, Result},
    store::{id, now},
};
use image::{
    ImageReader,
    codecs::gif::{GifEncoder, Repeat},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Cursor, Write},
    path::Path,
};

pub fn motion_label(motion: Motion) -> &'static str {
    match motion {
        Motion::Idle => "Idle",
        Motion::Walk => "Walk",
        Motion::Run => "Run",
        Motion::Jump => "Jump",
        Motion::Attack => "Attack",
        Motion::Custom => "Custom",
    }
}
pub fn defaults(style: &StyleGuide, motion: Motion) -> AnimationConfig {
    let (size, _) = crate::presets::output_size(style, AssetKind::Character);
    AnimationConfig {
        name: motion_label(motion).into(),
        motion,
        frame_width: size / 2,
        frame_height: size / 2,
        fps: if motion == Motion::Run { 12 } else { 8 },
        is_looping: !matches!(motion, Motion::Jump | Motion::Attack),
        ..AnimationConfig::default()
    }
}
pub fn motion_direction(motion: Motion) -> &'static str {
    match motion {
        Motion::Idle => {
            "A calm idle cycle: subtle breathing, weight shift, then return to the opening pose."
        }
        Motion::Walk => {
            "A complete side-view walk cycle, facing right: contact, down, passing, opposite contact, opposite down, opposite passing. Alternate the leading leg; feet share a stable ground line."
        }
        Motion::Run => {
            "A complete side-view run cycle, facing right: contact, compression, flight, opposite contact, opposite compression, opposite flight. Distinct leg and arm poses with a stable center."
        }
        Motion::Jump => {
            "A single jump: neutral, anticipation crouch, takeoff, apex, descent, landing. Keep the character centered horizontally; preserve the vertical arc within every cell."
        }
        Motion::Attack => {
            "A single attack: neutral, anticipation, windup, strike, follow-through, recovery. Maintain the same facing direction and weapon identity."
        }
        Motion::Custom => {
            "Follow the user's motion brief as an ordered sequence of distinct poses."
        }
    }
}
pub(crate) fn sheet_direction(config: &AnimationConfig) -> String {
    format!(
        "SPRITE ANIMATION: {}. {}\nExactly {} sequential frames in a {} column × {} row grid. Read left to right, top to bottom. Every cell is {} × {} target pixels. No gutters, borders, labels, shadows outside the cell, or frame numbers. Leave trailing cells empty. Keep identical character scale, camera, face, costume, palette and pivot across all frames. Place each character at the center of its cell with a shared foot baseline at 85% of the cell height. Reserve at least 10% fully transparent padding on ALL FOUR sides of EVERY cell. The entire character, feet, weapon and effects must fit inside the central 80% of its cell; choose one scale that fits the widest attack pose and use that scale throughout. Never let pixels cross or touch cell boundaries. Poses must visibly advance through the motion; do not repeat a static portrait. Render one transparent atlas, not separate images. The clip plays at {} FPS; {}.",
        config.name,
        motion_direction(config.motion),
        config.frame_count,
        config.columns,
        config.rows(),
        config.frame_width,
        config.frame_height,
        config.fps,
        if config.is_looping {
            "last pose must transition smoothly into the first"
        } else {
            "finish in a clear recovery pose"
        }
    )
}

fn parse<T: serde::de::DeserializeOwned>(v: Value) -> Result<T> {
    serde_json::from_value(v).map_err(|e| ApiError::validation(e.to_string()))
}
fn encode<T: serde::Serialize>(v: T) -> Result<Value> {
    serde_json::to_value(v).map_err(ApiError::storage)
}
impl Service {
    pub(crate) async fn animation_dispatch(&self, method: &str, params: Value) -> Result<Value> {
        match method {
            "animations/presets/list" => Ok(json!([
                Motion::Idle,
                Motion::Walk,
                Motion::Run,
                Motion::Jump,
                Motion::Attack,
                Motion::Custom
            ])),
            "animations/create" => {
                let p: CreateAnimation = parse(params)?;
                p.config.validate()?;
                if p.config.motion == Motion::Custom {
                    nonempty("prompt", &p.prompt, 8000)?;
                }
                let (width, height) = p.config.atlas_size();
                let job = self
                    .dispatch(
                        "jobs/create",
                        json!(GenerateInput {
                            project_id: p.project_id,
                            character_id: Some(p.character_id),
                            idempotency_key: p.idempotency_key,
                            prompt: if p.prompt.trim().is_empty() {
                                motion_direction(p.config.motion).into()
                            } else {
                                p.prompt
                            },
                            kind: AssetKind::SpriteSheet,
                            width,
                            height,
                            transparent_background: true,
                            reference_asset_ids: p.reference_asset_ids,
                            animation: Some(p.config),
                        }),
                    )
                    .await?;
                encode(self.animation(&parse::<Job>(job)?.id)?)
            }
            "animations/get" => {
                let p: AnimationId = parse(params)?;
                encode(self.animation(&p.id)?)
            }
            "animations/list" => {
                let p: ListInput = parse(params)?;
                let mut page = self.store.list::<Animation>(
                    "animation",
                    p.project_id.as_deref(),
                    p.page,
                    p.page_size,
                )?;
                for clip in &mut page.data {
                    self.sync_animation(clip)?;
                }
                encode(page)
            }
            "animations/setup" => {
                let p: SetupAnimation = parse(params)?;
                p.config.validate()?;
                nonempty("idempotencyKey", &p.idempotency_key, 128)?;
                let assets = self
                    .store
                    .validate_references(&p.project_id, std::slice::from_ref(&p.asset_id))?;
                if let Some(character) = &p.character_id {
                    let c: Character = self.store.get("character", character)?;
                    if c.project_id != p.project_id {
                        return Err(ApiError::validation("Choose a character in this project."));
                    }
                }
                let key = format!("animation-setup:{}:{}", p.project_id, p.idempotency_key);
                if let Some(cached) = self.store.claim_effect(&key, &json!(p))? {
                    if let Some(error) = cached.get("error") {
                        return Err(parse(error.clone())?);
                    }
                    let cached: Animation = parse(cached["result"].clone())?;
                    return encode(self.animation(&cached.id)?);
                }
                let mut clip = Animation {
                    id: id(),
                    project_id: p.project_id,
                    character_id: p.character_id,
                    job_id: None,
                    source_asset_id: Some(p.asset_id),
                    config: p.config,
                    status: JobStatus::Succeeded,
                    frames: vec![],
                    preview_path: None,
                    error: None,
                    created_at: now(),
                };
                let result = self.extract_frames(&mut clip, &assets[0]).and_then(|()| {
                    self.save_animation(&clip)?;
                    encode(&clip)
                });
                self.store.finish_effect(
                    &key,
                    &match &result {
                        Ok(v) => json!({"result":v}),
                        Err(e) => json!({"error":e}),
                    },
                )?;
                result
            }
            "animations/timing/update" => {
                let p: UpdateAnimationTiming = parse(params)?;
                let mut clip = self.animation(&p.id)?;
                if clip.status != JobStatus::Succeeded {
                    return Err(ApiError::validation(
                        "Finish the animation before changing its timing.",
                    ));
                }
                clip.config.fps = p.fps;
                clip.config.is_looping = p.is_looping;
                clip.config.validate()?;
                self.write_preview(&mut clip)?;
                self.save_animation(&clip)?;
                encode(clip)
            }
            "animations/align" => {
                let p: AlignAnimation = parse(params)?;
                nonempty("idempotencyKey", &p.idempotency_key, 128)?;
                let original = self.animation(&p.id)?;
                if original.status != JobStatus::Succeeded || original.config.motion != Motion::Idle
                {
                    return Err(ApiError::validation(
                        "Choose a completed standing idle animation to align its planted feet.",
                    ));
                }
                let source: Asset = self
                    .store
                    .get("asset", original.source_asset_id.as_deref().unwrap())?;
                let cfg = &original.config;
                cfg.validate()?;
                if cfg.margin != 0
                    || cfg.spacing != 0
                    || cfg.atlas_size() != (source.width, source.height)
                {
                    return Err(ApiError::new(
                        "ANIMATION_GRID_MISMATCH",
                        "Correct the source grid before aligning the idle. Alignment requires a complete atlas with zero margin and spacing.",
                    ));
                }
                let key = format!("animation-align:{}:{}", p.id, p.idempotency_key);
                if let Some(cached) = self.store.claim_effect(&key, &json!(p))? {
                    if let Some(error) = cached.get("error") {
                        return Err(parse(error.clone())?);
                    }
                    let saved: Animation = parse(cached["result"].clone())?;
                    return encode(self.animation(&saved.id)?);
                }
                let result = (|| {
                    let bytes = normalize_grid(
                        &fs::read(&source.path).map_err(ApiError::storage)?,
                        cfg,
                        false,
                    )?;
                    let asset = self.save_animation_atlas(
                        &original.project_id,
                        original.character_id.as_deref(),
                        &format!("{} atlas", cfg.name),
                        &bytes,
                    )?;
                    let mut clip = original.clone();
                    clip.id = id();
                    clip.job_id = None;
                    clip.source_asset_id = Some(asset.id.clone());
                    clip.frames.clear();
                    clip.preview_path = None;
                    clip.created_at = now();
                    self.extract_frames(&mut clip, &asset)?;
                    self.save_animation(&clip)?;
                    encode(clip)
                })();
                self.store.finish_effect(
                    &key,
                    &match &result {
                        Ok(value) => json!({"result":value}),
                        Err(error) => json!({"error":error}),
                    },
                )?;
                result
            }
            "animations/export" => {
                let p: AnimationExport = parse(params)?;
                let clip = self.animation(&p.id)?;
                if clip.status != JobStatus::Succeeded || clip.frames.is_empty() {
                    return Err(ApiError::validation(
                        "Only completed animations can be exported.",
                    ));
                }
                let asset: Asset = self
                    .store
                    .get("asset", clip.source_asset_id.as_ref().unwrap())?;
                export_zip(&clip, &asset, Path::new(&p.path))?;
                Ok(json!({"path":p.path,"animationId":clip.id}))
            }
            _ => Err(ApiError::new(
                "METHOD_NOT_FOUND",
                format!("Unknown API method: {method}"),
            )),
        }
    }
    fn save_animation(&self, clip: &Animation) -> Result<()> {
        self.store
            .put("animation", &clip.id, Some(&clip.project_id), clip)
    }
    fn sync_animation(&self, clip: &mut Animation) -> Result<()> {
        if let Some(job) = &clip.job_id {
            let job = self.store.job(job)?;
            clip.status = job.status;
            clip.error = job.error;
            if clip.source_asset_id.is_none() {
                clip.source_asset_id = job.asset_ids.first().cloned();
            }
        }
        Ok(())
    }
    pub(crate) fn animation(&self, key: &str) -> Result<Animation> {
        let mut clip = self.store.get("animation", key)?;
        self.sync_animation(&mut clip)?;
        Ok(clip)
    }
    pub(crate) fn animation_context(&self, project: &str) -> Result<Vec<Animation>> {
        self.store
            .list::<Animation>("animation", Some(project), 1, 50)?
            .data
            .into_iter()
            .map(|a| self.animation(&a.id))
            .collect()
    }
    pub(crate) fn ensure_animation(&self, job: &Job) -> Result<()> {
        let Some(config) = &job.request.animation else {
            return Ok(());
        };
        match self.store.get::<Animation>("animation", &job.id) {
            Ok(_) => Ok(()),
            Err(e) if e.code == "NOT_FOUND" => self.save_animation(&Animation {
                id: job.id.clone(),
                project_id: job.project_id.clone(),
                character_id: job.request.character_id.clone(),
                job_id: Some(job.id.clone()),
                source_asset_id: job.asset_ids.first().cloned(),
                config: config.clone(),
                status: job.status,
                frames: vec![],
                preview_path: None,
                error: job.error.clone(),
                created_at: job.created_at,
            }),
            Err(e) => Err(e),
        }
    }
    pub(crate) fn finish_animation(&self, job: &Job, asset: &Asset) -> Result<()> {
        let mut clip: Animation = self.store.get("animation", &job.id)?;
        clip.source_asset_id = Some(asset.id.clone());
        self.save_animation(&clip)?;
        self.extract_frames(&mut clip, asset)?;
        self.save_animation(&clip)
    }
    fn extract_frames(&self, clip: &mut Animation, asset: &Asset) -> Result<()> {
        let cfg = &clip.config;
        cfg.validate()?;
        let (width, height) = cfg.atlas_size();
        if width > asset.width || height > asset.height {
            return Err(ApiError::validation(
                "This frame grid extends outside the sheet. Reduce cell size, frame count, margin or spacing.",
            ));
        }
        let atlas = image::open(&asset.path)
            .map_err(ApiError::storage)?
            .to_rgba8();
        let directory = self.store.root.join("animations").join(&clip.id);
        fs::create_dir_all(&directory).map_err(ApiError::storage)?;
        let mut frames = Vec::new();
        for index in 0..cfg.frame_count {
            let x = cfg.margin + (index % cfg.columns) * (cfg.frame_width + cfg.spacing);
            let y = cfg.margin + (index / cfg.columns) * (cfg.frame_height + cfg.spacing);
            let frame = image::imageops::crop_imm(&atlas, x, y, cfg.frame_width, cfg.frame_height)
                .to_image();
            if !frame.pixels().any(|p| p[3] > 16) {
                return Err(ApiError::new(
                    "EMPTY_ANIMATION_FRAME",
                    format!(
                        "Frame {} is empty. The sheet is saved; adjust its grid with animations/setup or generate a new clip.",
                        index + 1
                    ),
                ));
            }
            let path = directory.join(format!("frame-{index:03}.png"));
            frame.save(&path).map_err(ApiError::storage)?;
            frames.push(AnimationFrame {
                index,
                path: path.to_string_lossy().into(),
                rect: FrameRect {
                    x,
                    y,
                    w: cfg.frame_width,
                    h: cfg.frame_height,
                },
            });
        }
        clip.frames = frames;
        self.write_preview(clip)
    }
    fn write_preview(&self, clip: &mut Animation) -> Result<()> {
        let directory = self.store.root.join("animations").join(&clip.id);
        let path = directory.join(format!(
            "preview-{}-{}.gif",
            clip.config.fps, clip.config.is_looping
        ));
        let file = fs::File::create(&path).map_err(ApiError::storage)?;
        let mut encoder = GifEncoder::new_with_speed(file, 10);
        if clip.config.is_looping {
            encoder
                .set_repeat(Repeat::Infinite)
                .map_err(ApiError::storage)?;
        }
        for frame in &clip.frames {
            let pixels = image::open(&frame.path)
                .map_err(ApiError::storage)?
                .to_rgba8();
            encoder
                .encode_frame(image::Frame::from_parts(
                    pixels,
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(1000, clip.config.fps),
                ))
                .map_err(ApiError::storage)?;
        }
        drop(encoder);
        clip.preview_path = Some(path.to_string_lossy().into());
        Ok(())
    }
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AnimationId {
    id: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AnimationExport {
    id: String,
    path: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AlignAnimation {
    id: String,
    idempotency_key: String,
}

/// Keep grid boundaries exact even when the provider chooses a different canvas aspect ratio.
pub(crate) fn normalize_grid(
    data: &[u8],
    cfg: &AnimationConfig,
    pixel_art: bool,
) -> Result<Vec<u8>> {
    cfg.validate()?;
    if data.len() > 50 * 1024 * 1024 {
        return Err(ApiError::new(
            "INVALID_IMAGE",
            "Images must be smaller than 50 MB.",
        ));
    }
    let mut reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(ApiError::storage)?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let source = reader
        .decode()
        .map_err(|_| ApiError::new("INVALID_IMAGE", "The provider returned an invalid image."))?;
    if !source.to_rgba8().pixels().any(|p| p[3] < 255) {
        return Err(ApiError::new(
            "TRANSPARENCY_UNAVAILABLE",
            "Animation generation requires real transparency. The provider returned an opaque sheet.",
        ));
    }
    let (width, height) = cfg.atlas_size();
    let mut atlas = image::RgbaImage::new(width, height);
    let filter = if pixel_art {
        image::imageops::FilterType::Nearest
    } else {
        image::imageops::FilterType::Lanczos3
    };
    let mut cells = Vec::new();
    for index in 0..cfg.frame_count {
        let col = index % cfg.columns;
        let row = index / cfg.columns;
        let x = col * source.width() / cfg.columns;
        let y = row * source.height() / cfg.rows();
        let w = (col + 1) * source.width() / cfg.columns - x;
        let h = (row + 1) * source.height() / cfg.rows() - y;
        if w == 0 || h == 0 {
            return Err(ApiError::new(
                "INVALID_IMAGE",
                "The source sheet is smaller than its frame grid.",
            ));
        }
        let source_cell = source.crop_imm(x, y, w, h).to_rgba8();
        if touches_cell_edge(&source_cell) {
            return Err(ApiError::new(
                "CLIPPED_ANIMATION_FRAME",
                format!(
                    "Frame {} touches its grid boundary and may be clipped. The source sheet is saved. Generate again with more transparent padding, or use animations/setup to choose the grid deliberately.",
                    index + 1
                ),
            ));
        }
        let cell = image::DynamicImage::ImageRgba8(source_cell)
            .resize(cfg.frame_width, cfg.frame_height, filter)
            .to_rgba8();
        let mut padded = image::RgbaImage::new(cfg.frame_width, cfg.frame_height);
        image::imageops::overlay(
            &mut padded,
            &cell,
            ((cfg.frame_width - cell.width()) / 2) as i64,
            ((cfg.frame_height - cell.height()) / 2) as i64,
        );
        cells.push(padded);
    }
    if cfg.motion == Motion::Idle {
        align_idle_cells(&mut cells)?;
    }
    for (index, cell) in cells.iter().enumerate() {
        image::imageops::overlay(
            &mut atlas,
            cell,
            (index as u32 % cfg.columns * cfg.frame_width) as i64,
            (index as u32 / cfg.columns * cfg.frame_height) as i64,
        );
    }
    let mut result = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(atlas)
        .write_to(&mut result, image::ImageFormat::Png)
        .map_err(ApiError::storage)?;
    Ok(result.into_inner())
}

fn visible_bounds(cell: &image::RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let (mut left, mut top) = cell.dimensions();
    let (mut right, mut bottom) = (0, 0);
    for (x, y, pixel) in cell.enumerate_pixels() {
        if pixel[3] > 32 {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    (bottom > 0).then_some((left, top, right, bottom))
}

// Standing idles keep their bottom silhouette (boots/base) stationary. Other
// motions retain their original translations, including a jump's vertical arc.
fn align_idle_cells(cells: &mut [image::RgbaImage]) -> Result<()> {
    let mut anchors = Vec::new();
    for (index, cell) in cells.iter().enumerate() {
        let bounds = visible_bounds(cell).ok_or_else(|| {
            ApiError::new(
                "EMPTY_ANIMATION_FRAME",
                format!("Frame {} is empty.", index + 1),
            )
        })?;
        let foot_top = bounds.3 - ((bounds.3 - bounds.1) / 10).max(1);
        let (mut left, mut right) = (cell.width(), 0);
        for (x, y, pixel) in cell.enumerate_pixels() {
            if y >= foot_top && pixel[3] > 32 {
                left = left.min(x);
                right = right.max(x + 1);
            }
        }
        anchors.push((((left + right) / 2) as i64, bounds.3 as i64, bounds));
    }
    let mut xs: Vec<_> = anchors.iter().map(|a| a.0).collect();
    let mut ys: Vec<_> = anchors.iter().map(|a| a.1).collect();
    xs.sort_unstable();
    ys.sort_unstable();
    let target = (xs[xs.len() / 2], ys[ys.len() / 2]);
    for (cell, (x, y, bounds)) in cells.iter_mut().zip(anchors) {
        let (dx, dy) = (target.0 - x, target.1 - y);
        if bounds.0 as i64 + dx < 1
            || bounds.1 as i64 + dy < 1
            || bounds.2 as i64 + dx >= cell.width() as i64
            || bounds.3 as i64 + dy >= cell.height() as i64
        {
            return Err(ApiError::new(
                "ANIMATION_ALIGNMENT_UNAVAILABLE",
                "There is not enough transparent padding to align this idle without cutting it. Generate with more padding.",
            ));
        }
        let mut aligned = image::RgbaImage::new(cell.width(), cell.height());
        image::imageops::overlay(&mut aligned, cell, dx, dy);
        *cell = aligned;
    }
    Ok(())
}

// Ignore isolated antialiasing/noise pixels, but reject a visible silhouette that
// reaches a generated cell's boundary. Padding cannot restore already cut art.
fn touches_cell_edge(cell: &image::RgbaImage) -> bool {
    fn visible_run(pixels: impl Iterator<Item = u8>) -> bool {
        let mut run = 0;
        for alpha in pixels {
            run = if alpha > 32 { run + 1 } else { 0 };
            if run >= 3 {
                return true;
            }
        }
        false
    }
    let (w, h) = cell.dimensions();
    visible_run((0..w).map(|x| cell.get_pixel(x, 0)[3]))
        || visible_run((0..w).map(|x| cell.get_pixel(x, h - 1)[3]))
        || visible_run((0..h).map(|y| cell.get_pixel(0, y)[3]))
        || visible_run((0..h).map(|y| cell.get_pixel(w - 1, y)[3]))
}

fn export_zip(clip: &Animation, atlas: &Asset, path: &Path) -> Result<()> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| {
            ApiError::new(
                "EXPORT_ERROR",
                "Choose a writable ZIP filename that does not already exist.",
            )
        })?;
    let result = (|| -> Result<()> {
        let mut archive = zip::ZipWriter::new(file);
        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        archive
            .start_file("atlas.png", options)
            .map_err(ApiError::storage)?;
        archive
            .write_all(&fs::read(&atlas.path).map_err(ApiError::storage)?)
            .map_err(ApiError::storage)?;
        for frame in &clip.frames {
            archive
                .start_file(format!("frames/frame-{:03}.png", frame.index), options)
                .map_err(ApiError::storage)?;
            archive
                .write_all(&fs::read(&frame.path).map_err(ApiError::storage)?)
                .map_err(ApiError::storage)?;
        }
        if let Some(preview) = &clip.preview_path {
            archive
                .start_file("preview.gif", options)
                .map_err(ApiError::storage)?;
            archive
                .write_all(&fs::read(preview).map_err(ApiError::storage)?)
                .map_err(ApiError::storage)?;
        }
        let frames: Vec<Value> = clip.frames.iter().map(|f|json!({"filename":format!("frames/frame-{:03}.png",f.index),"frame":f.rect,"rotated":false,"trimmed":false,"spriteSourceSize":{"x":0,"y":0,"w":f.rect.w,"h":f.rect.h},"sourceSize":{"w":f.rect.w,"h":f.rect.h},"duration":(1000.0/clip.config.fps as f64).round() as u32})).collect();
        let metadata = json!({"frames":frames,"meta":{"app":"Asset Forge","version":1,"image":"atlas.png","format":"RGBA8888","size":{"w":atlas.width,"h":atlas.height},"scale":"1","frameTags":[{"name":clip.config.name,"from":0,"to":clip.config.frame_count-1,"direction":"forward"}],"fps":clip.config.fps,"loop":clip.config.is_looping,"motion":clip.config.motion,"pivot":{"x":0.5,"y":1.0},"characterId":clip.character_id,"projectId":clip.project_id}});
        archive
            .start_file("animation.json", options)
            .map_err(ApiError::storage)?;
        archive
            .write_all(&serde_json::to_vec_pretty(&metadata).map_err(ApiError::storage)?)
            .map_err(ApiError::storage)?;
        archive
            .finish()
            .map_err(ApiError::storage)?
            .sync_all()
            .map_err(ApiError::storage)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::AnimationDecoder;

    fn displaced_idle_sheet() -> image::RgbaImage {
        let mut sheet = image::RgbaImage::new(96, 64);
        for (index, (left, top)) in [(5, 5), (8, 7), (6, 4), (7, 9), (4, 6), (8, 6)]
            .into_iter()
            .enumerate()
        {
            for y in top..top + 16 {
                for x in left..left + 12 {
                    sheet.put_pixel(
                        index as u32 % 3 * 32 + x,
                        index as u32 / 3 * 32 + y,
                        image::Rgba([30 + index as u8 * 30, 90, 70, 255]),
                    );
                }
            }
        }
        sheet
    }

    #[tokio::test]
    async fn idle_alignment_preserves_source_replays_and_exports_matching_frames() {
        let tmp = tempfile::tempdir().unwrap();
        let service = Service::open(tmp.path().join("data")).unwrap();
        let project = service
            .dispatch(
                "projects/create",
                json!({"name":"Idle","style":{"name":"Ink","description":"Ink"}}),
            )
            .await
            .unwrap();
        let path = tmp.path().join("idle.png");
        displaced_idle_sheet().save(&path).unwrap();
        let asset = service
            .dispatch(
                "assets/import",
                json!({"projectId":project["id"],"path":path,"name":"Idle","kind":"SPRITE_SHEET"}),
            )
            .await
            .unwrap();
        let original_bytes = fs::read(asset["path"].as_str().unwrap()).unwrap();
        let original = service.dispatch("animations/setup", json!({"projectId":project["id"],"assetId":asset["id"],"idempotencyKey":"source","config":{"name":"Idle","motion":"IDLE","frameWidth":32,"frameHeight":32}})).await.unwrap();
        let request = json!({"id":original["id"],"idempotencyKey":"align-1"});
        let aligned = service
            .dispatch("animations/align", request.clone())
            .await
            .unwrap();
        assert_ne!(aligned["id"], original["id"]);
        assert_ne!(aligned["sourceAssetId"], original["sourceAssetId"]);
        assert_eq!(
            service.dispatch("animations/align", request).await.unwrap()["id"],
            aligned["id"]
        );
        assert_eq!(
            service
                .dispatch("assets/list", json!({"projectId":project["id"]}))
                .await
                .unwrap()["data"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            service
                .dispatch("animations/get", json!({"id":original["id"]}))
                .await
                .unwrap(),
            original
        );
        assert_eq!(
            fs::read(asset["path"].as_str().unwrap()).unwrap(),
            original_bytes
        );
        let clip: Animation = parse(aligned.clone()).unwrap();
        for frame in &clip.frames {
            let pixels = image::open(&frame.path).unwrap().to_rgba8();
            assert_eq!(visible_bounds(&pixels), Some((7, 6, 19, 22)));
            assert_eq!(pixels.get_pixel(13, 21)[0], 30 + frame.index as u8 * 30);
        }
        let exported = tmp.path().join("idle.zip");
        service
            .dispatch(
                "animations/export",
                json!({"id":aligned["id"],"path":exported}),
            )
            .await
            .unwrap();
        let mut zip = zip::ZipArchive::new(fs::File::open(&exported).unwrap()).unwrap();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut zip.by_name("atlas.png").unwrap(), &mut bytes).unwrap();
        let atlas = image::load_from_memory(&bytes).unwrap().to_rgba8();
        for frame in &clip.frames {
            assert_eq!(
                image::imageops::crop_imm(&atlas, frame.rect.x, frame.rect.y, 32, 32).to_image(),
                image::open(&frame.path).unwrap().to_rgba8()
            );
        }
        let mut jump_cfg = clip.config.clone();
        jump_cfg.motion = Motion::Jump;
        let normalized = normalize_grid(&original_bytes, &jump_cfg, false).unwrap();
        assert_eq!(
            image::load_from_memory(&normalized).unwrap().to_rgba8(),
            displaced_idle_sheet()
        );
    }

    #[test]
    fn grid_normalization_keeps_frame_order_and_dimensions() {
        let cfg = AnimationConfig {
            frame_width: 16,
            frame_height: 16,
            ..Default::default()
        };
        let mut source = image::RgbaImage::new(90, 40);
        for index in 0..6 {
            for y in 2..18 {
                for x in 2..28 {
                    source.put_pixel(
                        (index % 3) * 30 + x,
                        (index / 3) * 20 + y,
                        image::Rgba([30 + index as u8 * 30, 90, 70, 255]),
                    );
                }
            }
        }
        let mut input = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(source)
            .write_to(&mut input, image::ImageFormat::Png)
            .unwrap();
        let bytes = normalize_grid(&input.into_inner(), &cfg, true).unwrap();
        let atlas = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(atlas.dimensions(), (48, 32));
        for i in 0..6 {
            assert_eq!(
                atlas.get_pixel((i % 3) * 16 + 8, (i / 3) * 16 + 8).0,
                [30 + i as u8 * 30, 90, 70, 255]
            );
        }
        let mut bad = cfg.clone();
        bad.frame_count = 0;
        assert!(bad.validate().is_err());
        bad = cfg;
        bad.frame_width = u32::MAX;
        assert!(bad.validate().is_err());
    }

    #[tokio::test]
    async fn imported_sheet_extracts_padding_timing_and_portable_export() {
        let tmp = tempfile::tempdir().unwrap();
        let service = Service::open(tmp.path().join("data")).unwrap();
        let project = service
            .dispatch(
                "projects/create",
                json!({"name":"Sprites","style":{"name":"Ink","description":"Ink outlines"}}),
            )
            .await
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let cfg = AnimationConfig {
            frame_width: 16,
            frame_height: 16,
            margin: 2,
            spacing: 1,
            ..Default::default()
        };
        let (w, h) = cfg.atlas_size();
        let mut image = image::RgbaImage::new(w, h);
        for i in 0..6 {
            for y in 0..16 {
                for x in 0..16 {
                    image.put_pixel(
                        2 + (i % 3) * 17 + x,
                        2 + (i / 3) * 17 + y,
                        image::Rgba([30 + i as u8 * 30, 90, 70, 255]),
                    );
                }
            }
        }
        let source = tmp.path().join("sheet.png");
        image.save(&source).unwrap();
        let asset=service.dispatch("assets/import",json!({"projectId":project,"name":"Walk sheet","path":source,"kind":"SPRITE_SHEET"})).await.unwrap();
        let request = json!(SetupAnimation {
            project_id: project.clone(),
            asset_id: asset["id"].as_str().unwrap().into(),
            character_id: None,
            idempotency_key: "setup".into(),
            config: cfg
        });
        let clip: Animation = parse(
            service
                .dispatch("animations/setup", request.clone())
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(clip.frames.len(), 6);
        assert_eq!(clip.frames[5].rect.x, 36);
        assert_eq!(clip.frames[5].rect.y, 19);
        for frame in &clip.frames {
            assert_eq!(
                image::open(&frame.path)
                    .unwrap()
                    .to_rgba8()
                    .get_pixel(8, 8)
                    .0,
                [30 + frame.index as u8 * 30, 90, 70, 255]
            );
        }
        assert_eq!(
            service
                .dispatch("animations/setup", request.clone())
                .await
                .unwrap()["id"],
            clip.id
        );
        let mut changed = request;
        changed["config"]["fps"] = json!(12);
        assert_eq!(
            service
                .dispatch("animations/setup", changed)
                .await
                .unwrap_err()
                .code,
            "IDEMPOTENCY_CONFLICT"
        );
        let updated: Animation = parse(
            service
                .dispatch(
                    "animations/timing/update",
                    json!({"id":clip.id,"fps":20,"isLooping":false}),
                )
                .await
                .unwrap(),
        )
        .unwrap();
        let gif = image::codecs::gif::GifDecoder::new(std::io::BufReader::new(
            fs::File::open(updated.preview_path.as_ref().unwrap()).unwrap(),
        ))
        .unwrap();
        let frames = gif.into_frames().collect_frames().unwrap();
        assert_eq!(frames.len(), 6);
        assert_eq!(frames[0].delay().numer_denom_ms(), (50, 1));
        let path = tmp.path().join("walk.zip");
        let params = json!({"id":clip.id,"path":path});
        service
            .dispatch("animations/export", params.clone())
            .await
            .unwrap();
        assert_eq!(
            service
                .dispatch("animations/export", params)
                .await
                .unwrap_err()
                .code,
            "EXPORT_ERROR"
        );
        let mut archive = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(archive.len(), 9);
        let metadata: Value =
            serde_json::from_reader(archive.by_name("animation.json").unwrap()).unwrap();
        assert_eq!(metadata["meta"]["fps"], 20);
        assert_eq!(metadata["meta"]["loop"], false);
        assert_eq!(metadata["frames"][5]["frame"]["x"], 36);
        let bad = json!({"projectId":project,"assetId":asset["id"],"idempotencyKey":"bad-grid","config":{"name":"Too large","frameWidth":512,"frameHeight":512}});
        assert_eq!(
            service
                .dispatch("animations/setup", bad)
                .await
                .unwrap_err()
                .code,
            "VALIDATION_ERROR"
        );
        let other = service
            .dispatch(
                "projects/create",
                json!({"name":"Other","style":{"name":"Ink","description":"Ink outlines"}}),
            )
            .await
            .unwrap();
        assert!(service.dispatch("animations/setup",json!({"projectId":other["id"],"assetId":asset["id"],"idempotencyKey":"wrong-project","config":{"name":"Walk","frameWidth":16,"frameHeight":16}})).await.is_err());
    }
}
