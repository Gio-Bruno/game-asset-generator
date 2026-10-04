//! Portable collection downloads, shared with the existing single-clip format.
use crate::{
    Service,
    animation::motion_label,
    contract::*,
    error::{ApiError, Result},
    store::now,
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{self, Write},
    path::Path,
};

pub(crate) struct ExportMedia {
    pub images: Vec<Asset>,
    pub animations: Vec<Animation>,
    pub sets: Vec<AnimationSet>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema_version: u32,
    exported_at: u64,
    subject: Character,
    project: Project,
    animation_set_id: Option<String>,
    images: Vec<ManifestImage>,
    animations: Vec<ManifestAnimation>,
    animation_sets: Vec<AnimationSet>,
    is_complete: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestImage {
    id: String,
    name: String,
    kind: AssetKind,
    width: u32,
    height: u32,
    has_alpha: bool,
    is_reference: bool,
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestAnimation {
    id: String,
    name: String,
    motion: Motion,
    facing_direction: Option<FacingDirection>,
    status: Option<JobStatus>,
    config: Option<AnimationConfig>,
    /// Archive-relative folder, or null when this clip could not be exported.
    path: Option<String>,
    skip_reason: Option<String>,
}

impl Service {
    pub(crate) async fn export_collection(
        &self,
        input: ExportCollectionInput,
        is_set: bool,
    ) -> Result<CollectionExport> {
        nonempty("path", &input.path, 4096)?;
        let service = self.clone();
        // ZIP work must not occupy an async executor thread used by the render queue.
        tokio::task::spawn_blocking(move || service.export_collection_sync(input, is_set))
            .await
            .map_err(ApiError::storage)?
    }

    fn export_collection_sync(
        &self,
        input: ExportCollectionInput,
        is_set: bool,
    ) -> Result<CollectionExport> {
        let set = if is_set {
            Some(self.store.get::<AnimationSet>("animation_set", &input.id)?)
        } else {
            None
        };
        let subject: Character = self.store.get(
            "character",
            set.as_ref().map_or(input.id.as_str(), |s| &s.character_id),
        )?;
        let project: Project = self.store.get("project", &subject.project_id)?;
        let media = self.store.subject_media_for_export(&subject)?;
        let mut clips = Vec::new();
        let mut entries = Vec::new();
        let requested: Vec<_> = if let Some(set) = &set {
            set.entries
                .iter()
                .map(|e| (e.animation_id.clone(), Some((e.motion, e.direction))))
                .collect()
        } else {
            media
                .animations
                .iter()
                .map(|a| (a.id.clone(), None))
                .collect()
        };
        let owned: HashSet<_> = media.animations.iter().map(|a| a.id.as_str()).collect();
        for (id, cell) in requested {
            // Deleted clips stay absent, including when a saved set still links them.
            let clip = if owned.contains(id.as_str()) {
                match self.animation(&id) {
                    Ok(clip) => Some(clip),
                    Err(e) if e.code == "NOT_FOUND" => None,
                    Err(e) => return Err(e),
                }
            } else {
                None
            };
            let Some(clip) = clip else {
                let (motion, direction) = cell.ok_or_else(|| {
                    ApiError::new(
                        "NOT_FOUND",
                        "An animation was removed during export. Try again.",
                    )
                })?;
                entries.push(ManifestAnimation {
                    id,
                    name: format!("{} {}", motion_label(motion), direction_code(direction)),
                    motion,
                    facing_direction: Some(direction),
                    status: None,
                    config: None,
                    path: None,
                    skip_reason: Some("DELETED_OR_MISSING".into()),
                });
                continue;
            };
            let ready = clip.status == JobStatus::Succeeded
                && clip.frames.len() == clip.config.frame_count as usize
                && !clip.frames.is_empty()
                && clip.source_asset_id.is_some();
            let folder = ready.then(|| {
                format!(
                    "animations/{}/{}/{}",
                    enum_slug(clip.config.motion),
                    clip.config
                        .direction
                        .map(direction_code)
                        .unwrap_or("unspecified"),
                    archive_name(&clip.config.name, &clip.id)
                )
            });
            let skip_reason = if ready {
                None
            } else if clip.status == JobStatus::Succeeded {
                Some("INCOMPLETE_FRAMES_OR_ATLAS".into())
            } else {
                Some(
                    serde_json::to_value(clip.status)
                        .map_err(ApiError::storage)?
                        .as_str()
                        .unwrap_or("UNKNOWN")
                        .to_owned(),
                )
            };
            entries.push(ManifestAnimation {
                id: clip.id.clone(),
                name: clip.config.name.clone(),
                motion: clip.config.motion,
                facing_direction: clip.config.direction,
                status: Some(clip.status),
                config: Some(clip.config.clone()),
                path: folder.clone(),
                skip_reason,
            });
            if let Some(folder) = folder {
                let atlas: Asset = self
                    .store
                    .get("asset", clip.source_asset_id.as_ref().unwrap())?;
                if atlas.project_id != subject.project_id {
                    return Err(ApiError::validation(
                        "The animation atlas belongs to another game.",
                    ));
                }
                clips.push((clip, atlas, folder));
            }
        }
        let mut images = if is_set { Vec::new() } else { media.images };
        images.extend(
            self.store
                .validate_references(&subject.project_id, &subject.reference_asset_ids)?,
        );
        images.extend(clips.iter().map(|(_, atlas, _)| atlas.clone()));
        let mut seen = HashSet::new();
        images.retain(|a| seen.insert(a.id.clone()));
        if images.is_empty() && clips.is_empty() {
            return Err(ApiError::validation(
                "No ready files to download yet. Wait for an asset to finish.",
            ));
        }
        let atlas_paths: HashMap<_, _> = clips
            .iter()
            .map(|(_, atlas, folder)| (atlas.id.as_str(), format!("{folder}/atlas.png")))
            .collect();
        let image_entries = images
            .iter()
            .map(|a| {
                let is_reference = a.character_id.as_ref() != Some(&subject.id);
                ManifestImage {
                    id: a.id.clone(),
                    name: a.name.clone(),
                    kind: a.kind,
                    width: a.width,
                    height: a.height,
                    has_alpha: a.has_alpha,
                    is_reference,
                    path: atlas_paths.get(a.id.as_str()).cloned().unwrap_or_else(|| {
                        format!(
                            "{}/{}.png",
                            if is_reference { "references" } else { "images" },
                            archive_name(&a.name, &a.id)
                        )
                    }),
                }
            })
            .collect();
        let skipped = entries.iter().filter(|e| e.path.is_none()).count();
        let manifest = Manifest {
            schema_version: 1,
            exported_at: now(),
            subject,
            project,
            animation_set_id: set.as_ref().map(|s| s.id.clone()),
            images: image_entries,
            animations: entries,
            animation_sets: set.map(|s| vec![s]).unwrap_or(media.sets),
            is_complete: skipped == 0,
        };
        write_zip(Path::new(&input.path), |archive| {
            for (clip, atlas, folder) in &clips {
                write_clip(archive, &format!("{folder}/"), clip, atlas)?;
            }
            for (asset, entry) in images.iter().zip(&manifest.images) {
                if !atlas_paths.contains_key(asset.id.as_str()) {
                    add_file(archive, &entry.path, Path::new(&asset.path))?;
                }
            }
            add_bytes(
                archive,
                "manifest.json",
                &serde_json::to_vec_pretty(&manifest).map_err(ApiError::storage)?,
            )?;
            add_bytes(archive, "README.txt", EXPORT_README.as_bytes())
        })?;
        Ok(CollectionExport {
            path: input.path,
            character_id: manifest.subject.id,
            animation_set_id: manifest.animation_set_id,
            image_count: manifest.images.len(),
            animation_count: clips.len(),
            skipped_animation_count: skipped,
            is_complete: manifest.is_complete,
        })
    }
}

fn direction_code(direction: FacingDirection) -> &'static str {
    match direction {
        FacingDirection::North => "N",
        FacingDirection::Northeast => "NE",
        FacingDirection::East => "E",
        FacingDirection::Southeast => "SE",
        FacingDirection::South => "S",
        FacingDirection::Southwest => "SW",
        FacingDirection::West => "W",
        FacingDirection::Northwest => "NW",
    }
}

fn enum_slug(motion: Motion) -> String {
    serde_json::to_value(motion)
        .expect("Motion is serializable")
        .as_str()
        .unwrap()
        .to_ascii_lowercase()
}

fn archive_name(name: &str, id: &str) -> String {
    let slug: String = name
        .chars()
        .take(48)
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    // Fixed hash suffix disambiguates revisions and keeps hostile/user names portable.
    let hash = format!("{:x}", Sha256::digest(id.as_bytes()));
    format!(
        "{}-{}",
        if slug.is_empty() { "asset" } else { slug },
        &hash[..16]
    )
}

pub(crate) fn write_zip(
    path: &Path,
    write: impl FnOnce(&mut zip::ZipWriter<fs::File>) -> Result<()>,
) -> Result<()> {
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
    let result = (|| {
        let mut archive = zip::ZipWriter::new(file);
        write(&mut archive)?;
        archive
            .finish()
            .map_err(ApiError::storage)?
            .sync_all()
            .map_err(ApiError::storage)
    })();
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

fn add_bytes(archive: &mut zip::ZipWriter<fs::File>, name: &str, bytes: &[u8]) -> Result<()> {
    archive
        .start_file(
            name,
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored),
        )
        .map_err(ApiError::storage)?;
    archive.write_all(bytes).map_err(ApiError::storage)
}

fn add_file(archive: &mut zip::ZipWriter<fs::File>, name: &str, path: &Path) -> Result<()> {
    archive
        .start_file(
            name,
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored),
        )
        .map_err(ApiError::storage)?;
    let mut file = fs::File::open(path).map_err(ApiError::storage)?;
    io::copy(&mut file, archive).map_err(ApiError::storage)?;
    Ok(())
}

pub(crate) fn write_clip(
    archive: &mut zip::ZipWriter<fs::File>,
    prefix: &str,
    clip: &Animation,
    atlas: &Asset,
) -> Result<()> {
    add_file(
        archive,
        &format!("{prefix}atlas.png"),
        Path::new(&atlas.path),
    )?;
    for frame in &clip.frames {
        add_file(
            archive,
            &format!("{prefix}frames/frame-{:03}.png", frame.index),
            Path::new(&frame.path),
        )?;
    }
    if let Some(preview) = &clip.preview_path {
        add_file(archive, &format!("{prefix}preview.gif"), Path::new(preview))?;
    }
    // Paths within animation.json remain relative to its own clip folder.
    let frames: Vec<Value> = clip.frames.iter().map(|f|json!({"filename":format!("frames/frame-{:03}.png",f.index),"frame":f.rect,"rotated":false,"trimmed":false,"spriteSourceSize":{"x":0,"y":0,"w":f.rect.w,"h":f.rect.h},"sourceSize":{"w":f.rect.w,"h":f.rect.h},"duration":(1000.0/clip.config.fps as f64).round() as u32})).collect();
    let metadata = json!({"frames":frames,"meta":{"app":"Asset Forge","version":1,"image":"atlas.png","format":"RGBA8888","size":{"w":atlas.width,"h":atlas.height},"scale":"1","frameTags":[{"name":clip.config.name,"from":0,"to":clip.config.frame_count-1,"direction":"forward"}],"fps":clip.config.fps,"loop":clip.config.is_looping,"motion":clip.config.motion,"facingDirection":clip.config.direction,"pivot":{"x":0.5,"y":1.0},"characterId":clip.character_id,"projectId":clip.project_id}});
    add_bytes(
        archive,
        &format!("{prefix}animation.json"),
        &serde_json::to_vec_pretty(&metadata).map_err(ApiError::storage)?,
    )
}

const EXPORT_README: &str = r#"Asset Forge portable asset pack

Unzip this archive into a folder. manifest.json maps names and stable IDs to
archive-relative paths and includes the subject, art direction and clip status.

Images are PNGs with their original pixels and alpha. Linked subject references
are under references/. Atlases already included in a clip are not duplicated.

Animations are organized by motion and compass facing (N, NE, E, SE, S, SW, W, NW).
Each clip folder contains atlas.png, frames/frame-000.png onward, preview.gif
when available, and animation.json. Revisions have unique folder names.
Legacy clips without a saved facing use unspecified/; no direction is guessed.

Import atlas.png as a sprite texture and use animation.json frame rectangles
(in atlas pixels), source sizes and millisecond durations to build the animation.
Alternatively, import the numbered frame PNGs in order. JSON records FPS, loop,
motion, facingDirection and a normalized bottom-center pivot (0.5, 1.0).
A facing is relative to the screen, preserving the game's saved camera. Death,
attack and hit reactions normally play once: respect each clip's loop setting.
GIFs are previews only; PNGs retain full RGBA and JSON retains saved timing.
These files do not automatically configure an engine-specific importer.

Only completed clips with a full frame sequence are exported. Queued, failed,
cancelled, unknown or missing clips have a null path and a skipReason in the
manifest. isComplete=false means some requested clips are absent; download again
to a new filename after generation completes. A whole-subject pack covers the
currently visible library; animationSets record earlier requested coverage. A
set pack covers exactly that set's entries and its atlases/identity references.

Check animation playback for pose, identity and alignment before shipping.
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    async fn fixture(s: &Service, path: &Path) -> (Project, Character, Asset, Animation) {
        let project: Project = serde_json::from_value(
            s.dispatch(
                "projects/create",
                json!({"name":"Test game","style":{"name":"Ink","description":"Isometric ink"}}),
            )
            .await
            .unwrap(),
        )
        .unwrap();
        let subject: Character = serde_json::from_value(s.dispatch("characters/create", json!({"projectId":project.id,"name":"CON / Fiend: ../ 🎮","description":"A horned creature"})).await.unwrap()).unwrap();
        let png = path.join("source.png");
        let mut pixels = image::RgbaImage::new(32, 16);
        for y in 3..13 {
            for x in 3..13 {
                pixels.put_pixel(x, y, image::Rgba([40, 80, 160, 255]));
                pixels.put_pixel(x + 16, y, image::Rgba([160, 80, 40, 128]));
            }
        }
        pixels.save(&png).unwrap();
        let mut atlas: Asset = serde_json::from_value(s.dispatch("assets/import", json!({"projectId":project.id,"path":png,"name":"../CON: Walk","kind":"SPRITE_SHEET"})).await.unwrap()).unwrap();
        atlas.character_id = Some(subject.id.clone());
        s.store
            .put("asset", &atlas.id, Some(&project.id), &atlas)
            .unwrap();
        let clip: Animation = serde_json::from_value(s.dispatch("animations/setup", json!({"projectId":project.id,"characterId":subject.id,"assetId":atlas.id,"idempotencyKey":"setup","config":{"name":"../CON: Walk","motion":"WALK","direction":"NE","frameCount":2,"columns":2,"frameWidth":16,"frameHeight":16,"fps":10,"isLooping":false}})).await.unwrap()).unwrap();
        (project, subject, atlas, clip)
    }

    fn contents(zip: &mut zip::ZipArchive<fs::File>, path: &str) -> Vec<u8> {
        let mut bytes = vec![];
        zip.by_name(path).unwrap().read_to_end(&mut bytes).unwrap();
        bytes
    }
    fn manifest(zip: &mut zip::ZipArchive<fs::File>) -> Value {
        serde_json::from_slice(&contents(zip, "manifest.json")).unwrap()
    }

    #[tokio::test]
    async fn subject_download_exports_every_page_without_name_collisions_or_foreign_media() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Service::open(tmp.path().join("data")).unwrap();
        let (p, c, atlas, clip) = fixture(&s, tmp.path()).await;
        let source_bytes = fs::read(&atlas.path).unwrap();
        // More than the public list limit, with identical hostile display names.
        for i in 0..125 {
            let mut asset = atlas.clone();
            asset.id = format!("asset-{i}");
            asset.kind = AssetKind::Character;
            s.store
                .put("asset", &asset.id, Some(&p.id), &asset)
                .unwrap();
        }
        for i in 0..104 {
            let mut revision = clip.clone();
            revision.id = format!("clip-{i}");
            if i == 0 {
                revision.config.direction = None;
            }
            s.store
                .put("animation", &revision.id, Some(&p.id), &revision)
                .unwrap();
        }
        let other: Character = serde_json::from_value(
            s.dispatch(
                "characters/create",
                json!({"projectId":p.id,"name":c.name,"description":"Different identity"}),
            )
            .await
            .unwrap(),
        )
        .unwrap();
        let mut foreign = atlas.clone();
        foreign.id = "foreign-same-name".into();
        foreign.character_id = Some(other.id);
        s.store
            .put("asset", &foreign.id, Some(&p.id), &foreign)
            .unwrap();
        let mut reference = atlas.clone();
        reference.id = "reference".into();
        reference.character_id = None;
        s.store
            .put("asset", &reference.id, Some(&p.id), &reference)
            .unwrap();
        s.dispatch(
            "characters/references/update",
            json!({"id":c.id,"referenceAssetIds":[atlas.id,reference.id]}),
        )
        .await
        .unwrap();
        s.dispatch("assets/delete", json!({"id":"asset-50"}))
            .await
            .unwrap();
        let path = tmp.path().join("all.zip");
        let result = s
            .dispatch("library/subjects/export", json!({"id":c.id,"path":path}))
            .await
            .unwrap();
        assert_eq!(result["imageCount"], 126);
        assert_eq!(result["animationCount"], 105);
        assert_eq!(result["isComplete"], true);
        let mut zip = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        let manifest = manifest(&mut zip);
        assert_eq!(manifest["subject"]["id"], c.id);
        assert_eq!(
            manifest["project"]["style"],
            serde_json::to_value(&p.style).unwrap()
        );
        assert!(
            !String::from_utf8(contents(&mut zip, "manifest.json"))
                .unwrap()
                .contains(s.store.root.to_str().unwrap())
        );
        let mut paths = HashSet::new();
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index).unwrap();
            let name = entry.name().to_owned();
            assert!(paths.insert(name.clone()), "Duplicate ZIP entry: {name}");
            assert!(
                name.is_ascii()
                    && !name.starts_with('/')
                    && !name.contains('\\')
                    && !name.contains(':')
            );
            assert!(
                name.split('/')
                    .all(|part| !part.is_empty() && part != ".." && part != ".")
            );
            entry.read_to_end(&mut vec![]).unwrap(); // also verifies CRC
        }
        for image in manifest["images"].as_array().unwrap() {
            assert_ne!(image["id"], "foreign-same-name");
            assert_ne!(image["id"], "asset-50");
            assert_eq!(
                contents(&mut zip, image["path"].as_str().unwrap()),
                source_bytes
            );
            if image["id"] == "reference" {
                assert!(image["path"].as_str().unwrap().starts_with("references/"));
            }
        }
        for exported in manifest["animations"].as_array().unwrap() {
            let folder = exported["path"].as_str().unwrap();
            let metadata: Value =
                serde_json::from_slice(&contents(&mut zip, &format!("{folder}/animation.json")))
                    .unwrap();
            assert_eq!(metadata["meta"]["fps"], 10);
            assert_eq!(metadata["meta"]["loop"], false);
            assert_eq!(metadata["frames"][0]["duration"], 100);
            assert_eq!(metadata["meta"]["characterId"], c.id);
            assert_eq!(
                metadata["meta"]["facingDirection"],
                exported["facingDirection"]
            );
            for frame in &clip.frames {
                assert_eq!(
                    contents(
                        &mut zip,
                        &format!("{folder}/frames/frame-{:03}.png", frame.index)
                    ),
                    fs::read(&frame.path).unwrap()
                );
            }
            if exported["id"] == "clip-0" {
                assert!(folder.contains("/unspecified/"));
            }
        }
        assert_eq!(fs::read(&atlas.path).unwrap(), source_bytes);
        let saved = fs::read(&path).unwrap();
        assert_eq!(
            s.dispatch("library/subjects/export", json!({"id":c.id,"path":path}))
                .await
                .unwrap_err()
                .code,
            "EXPORT_ERROR"
        );
        assert_eq!(fs::read(&path).unwrap(), saved);
        s.dispatch("projects/delete", json!({"id":p.id}))
            .await
            .unwrap();
        let absent = tmp.path().join("deleted-game.zip");
        assert_eq!(
            s.dispatch("library/subjects/export", json!({"id":c.id,"path":absent}))
                .await
                .unwrap_err()
                .code,
            "NOT_FOUND"
        );
        assert!(!absent.exists());
    }

    #[tokio::test]
    async fn set_download_reports_incomplete_and_deleted_cells_with_authoritative_job_status() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Service::open(tmp.path().join("data")).unwrap();
        let (p, c, atlas, clip) = fixture(&s, tmp.path()).await;
        let request: CreateAnimationSet = serde_json::from_value(json!({"projectId":p.id,"characterId":c.id,"idempotencyKey":"set","motions":["ATTACK"],"directions":["N","S"],"frameSize":16})).unwrap();
        let (mut set, jobs) = s
            .store
            .claim_animation_set(&request, s.animation_set_requests(&request).unwrap())
            .unwrap();
        let mut failed_job = jobs[0].clone();
        failed_job.status = JobStatus::Failed;
        s.store.save_job(&failed_job).unwrap();
        // Stale entity even appears ready; the job is the source of truth.
        let mut stale = clip.clone();
        stale.id = failed_job.id.clone();
        stale.job_id = Some(failed_job.id.clone());
        stale.config = failed_job.request.animation.clone().unwrap();
        s.store
            .put("animation", &stale.id, Some(&p.id), &stale)
            .unwrap();
        set.entries.extend([
            AnimationSetEntry {
                motion: clip.config.motion,
                direction: FacingDirection::Northeast,
                animation_id: clip.id.clone(),
                is_reused: true,
            },
            AnimationSetEntry {
                motion: Motion::Death,
                direction: FacingDirection::East,
                animation_id: "deleted-cell".into(),
                is_reused: true,
            },
        ]);
        s.store
            .put("animation_set", &set.id, Some(&p.id), &set)
            .unwrap();
        let path = tmp.path().join("set.zip");
        let result = s
            .dispatch("animations/sets/export", json!({"id":set.id,"path":path}))
            .await
            .unwrap();
        assert_eq!(result["animationSetId"], set.id);
        assert_eq!(result["animationCount"], 1);
        assert_eq!(result["skippedAnimationCount"], 3);
        assert_eq!(result["isComplete"], false);
        let mut zip = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
        let manifest = manifest(&mut zip);
        assert_eq!(
            manifest["animationSets"][0],
            serde_json::to_value(set).unwrap()
        );
        let entries = manifest["animations"].as_array().unwrap();
        assert_eq!(entries.len(), 4);
        for (reason, status) in [
            ("FAILED", json!("FAILED")),
            ("QUEUED", json!("QUEUED")),
            ("DELETED_OR_MISSING", Value::Null),
        ] {
            let entry = entries.iter().find(|e| e["skipReason"] == reason).unwrap();
            assert!(entry["path"].is_null());
            assert_eq!(entry["status"], status);
        }
        assert_eq!(
            contents(&mut zip, manifest["images"][0]["path"].as_str().unwrap()),
            fs::read(atlas.path).unwrap()
        );
        assert!(
            zip.file_names()
                .all(|p| !p.contains("/attack/") && !p.contains("/death/"))
        );
    }

    #[tokio::test]
    async fn broken_sources_leave_no_partial_archive_or_overwritten_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Service::open(tmp.path().join("data")).unwrap();
        let (_, c, atlas, clip) = fixture(&s, tmp.path()).await;
        let source = fs::read(&atlas.path).unwrap();
        fs::remove_file(&clip.frames[1].path).unwrap();
        let path = tmp.path().join("broken.zip");
        assert!(
            s.dispatch("library/subjects/export", json!({"id":c.id,"path":path}))
                .await
                .is_err()
        );
        assert!(!path.exists());
        fs::write(&path, b"keep this export").unwrap();
        assert_eq!(
            s.dispatch("library/subjects/export", json!({"id":c.id,"path":path}))
                .await
                .unwrap_err()
                .code,
            "EXPORT_ERROR"
        );
        assert_eq!(fs::read(path).unwrap(), b"keep this export");
        assert_eq!(fs::read(&atlas.path).unwrap(), source);
        assert_eq!(
            s.dispatch(
                "library/subjects/export",
                json!({"id":c.id,"path":"", "pageSize":2})
            )
            .await
            .unwrap_err()
            .code,
            "VALIDATION_ERROR"
        );
    }
}
