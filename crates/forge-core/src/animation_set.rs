use crate::{
    Service, animation,
    contract::*,
    error::{ApiError, Result},
};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

impl Service {
    pub(crate) fn animation_set_requests(
        &self,
        input: &CreateAnimationSet,
    ) -> Result<Vec<GenerateInput>> {
        nonempty("idempotencyKey", &input.idempotency_key, 128)?;
        if input.motions.is_empty()
            || input.motions.len() > 7
            || input.directions.is_empty()
            || input.directions.len() > 8
        {
            return Err(ApiError::validation(
                "Choose 1–7 motions and 1–8 directions for an animation set.",
            ));
        }
        if input.motions.iter().collect::<HashSet<_>>().len() != input.motions.len()
            || input.directions.iter().collect::<HashSet<_>>().len() != input.directions.len()
        {
            return Err(ApiError::validation(
                "Include each motion and direction once.",
            ));
        }
        if input.motions.contains(&Motion::Custom) {
            return Err(ApiError::validation(
                "Use animations/create for a custom motion; sets use named motion presets.",
            ));
        }
        let project: Project = self.store.get("project", &input.project_id)?;
        let subject: Character = self.store.get("character", &input.character_id)?;
        if subject.project_id != project.id {
            return Err(ApiError::validation("Choose a subject in this game."));
        }
        let references: HashSet<_> = project
            .style
            .reference_asset_ids
            .iter()
            .chain(&subject.reference_asset_ids)
            .chain(&input.reference_asset_ids)
            .collect();
        if references.len() > 8 {
            return Err(ApiError::validation(
                "The combined style, subject and request references exceed 8 images.",
            ));
        }
        self.store.validate_references(
            &input.project_id,
            &references.into_iter().cloned().collect::<Vec<_>>(),
        )?;
        let mut requests = vec![];
        for motion in &input.motions {
            for direction in &input.directions {
                let mut config = animation::defaults(&project.style, *motion);
                config.direction = Some(*direction);
                config.name = format!(
                    "{} — {} {}",
                    subject.name.chars().take(70).collect::<String>(),
                    animation::motion_label(*motion),
                    animation::facing_label(*direction)
                );
                if let Some(count) = input.frame_count {
                    config.frame_count = count;
                    config.columns = config.columns.min(count);
                }
                if let Some(size) = input.frame_size {
                    config.frame_width = size;
                    config.frame_height = size;
                }
                if let Some(fps) = input.fps {
                    config.fps = fps;
                }
                let (width, height) = config.atlas_size();
                let key = format!(
                    "animation-set:{:x}:{}",
                    Sha256::digest(input.idempotency_key.as_bytes()),
                    requests.len()
                );
                let request = GenerateInput {
                    project_id: input.project_id.clone(),
                    character_id: Some(input.character_id.clone()),
                    idempotency_key: key,
                    prompt: if input.prompt.trim().is_empty() {
                        animation::motion_direction(*motion).into()
                    } else {
                        input.prompt.clone()
                    },
                    kind: AssetKind::SpriteSheet,
                    width,
                    height,
                    transparent_background: true,
                    reference_asset_ids: input.reference_asset_ids.clone(),
                    animation: Some(config),
                };
                request.validate()?;
                requests.push(request);
            }
        }
        Ok(requests)
    }

    pub(crate) async fn create_animation_set(
        &self,
        input: CreateAnimationSet,
    ) -> Result<AnimationSet> {
        let requests = self.animation_set_requests(&input)?;
        // Every cell, identity snapshot and replay receipt commits before rendering starts.
        let (set, jobs) = self.store.claim_animation_set(&input, requests)?;
        for job in jobs {
            self.launch_job(job).await;
        }
        Ok(set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::now;
    use serde_json::json;

    async fn fixture(s: &Service) -> (Project, Character) {
        let p = s.dispatch("projects/create", json!({"name":"Test game","style":{"name":"Isometric","description":"Dark isometric art"}})).await.unwrap();
        let c = s
            .dispatch(
                "characters/create",
                json!({"projectId":p["id"],"name":"Fiend","description":"Asymmetric horned demon"}),
            )
            .await
            .unwrap();
        (
            serde_json::from_value(p).unwrap(),
            serde_json::from_value(c).unwrap(),
        )
    }
    fn input(p: &Project, c: &Character) -> CreateAnimationSet {
        CreateAnimationSet {
            project_id: p.id.clone(),
            character_id: c.id.clone(),
            idempotency_key: "full-set".into(),
            motions: vec![
                Motion::Idle,
                Motion::Walk,
                Motion::Run,
                Motion::Attack,
                Motion::HitReaction,
                Motion::Death,
            ],
            directions: animation::ALL_DIRECTIONS.to_vec(),
            prompt: String::new(),
            reference_asset_ids: vec![],
            frame_count: None,
            frame_size: None,
            fps: None,
        }
    }

    #[tokio::test]
    async fn full_directional_set_is_atomic_replayable_and_preserves_identity() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("workspace");
        let s = Service::open(&root).unwrap();
        let (p, c) = fixture(&s).await;
        let request = input(&p, &c);
        let prepared = s.animation_set_requests(&request).unwrap();
        assert_eq!(prepared.len(), 48);
        let mut bad = prepared.clone();
        bad.last_mut().unwrap().reference_asset_ids = vec!["absent".into()];
        assert!(s.store.claim_animation_set(&request, bad).is_err());
        assert!(s.store.list_jobs(None).unwrap().is_empty());
        assert_eq!(
            s.store
                .list::<Animation>("animation", None, 1, 100)
                .unwrap()
                .pagination
                .total_items,
            0
        );
        let (set, jobs) = s.store.claim_animation_set(&request, prepared).unwrap();
        assert_eq!(set.entries.len(), 48);
        assert_eq!(set.job_ids.len(), 48);
        let mut cells = HashSet::new();
        for job in &jobs {
            let cfg = job.request.animation.as_ref().unwrap();
            assert!(cells.insert((cfg.motion, cfg.direction.unwrap())));
            assert_eq!(job.character_snapshot.as_ref().unwrap().id, c.id);
            assert_eq!(job.style_snapshot.description, p.style.description);
            let prompt = animation::sheet_direction(cfg);
            assert!(!prompt.contains("side-view"));
            assert!(prompt.contains("FACING "));
            let clip: Animation = s.store.get("animation", &job.id).unwrap();
            assert_eq!(clip.config.direction, cfg.direction);
            if matches!(
                cfg.motion,
                Motion::Attack | Motion::HitReaction | Motion::Death
            ) {
                assert!(!cfg.is_looping);
            }
            if cfg.motion == Motion::Death {
                assert!(prompt.contains("hold the final pose"));
            }
        }
        let (replay, workers) = s.store.claim_animation_set(&request, vec![]).unwrap();
        assert_eq!(replay.job_ids, set.job_ids);
        assert!(workers.is_empty());
        let mut changed = request.clone();
        changed.motions.pop();
        assert_eq!(
            s.store
                .claim_animation_set(&changed, vec![])
                .unwrap_err()
                .code,
            "IDEMPOTENCY_CONFLICT"
        );
        let mut duplicate = request.clone();
        duplicate.directions.push(FacingDirection::North);
        assert!(s.animation_set_requests(&duplicate).is_err());
        duplicate = request.clone();
        duplicate.motions.push(Motion::Walk);
        assert!(s.animation_set_requests(&duplicate).is_err());
        duplicate = request.clone();
        duplicate.motions = vec![Motion::Custom];
        assert!(s.animation_set_requests(&duplicate).is_err());
        let (other, _) = fixture(&s).await;
        duplicate = request.clone();
        duplicate.project_id = other.id;
        assert!(s.animation_set_requests(&duplicate).is_err());
        let old = AnimationConfig::default();
        assert!(
            serde_json::to_value(old)
                .unwrap()
                .get("direction")
                .is_none(),
            "Legacy intent hashes must retain omitted direction"
        );
        assert!(serde_json::from_value::<CreateAnimationSet>(json!({"projectId":p.id,"characterId":c.id,"idempotencyKey":"invalid","motions":["WALK"],"directions":["UP"]})).is_err());
        drop(s);
        // Other parallel tests fork subprocesses; their inherited lease closes at exec.
        let mut reopened = Service::open(&root);
        for _ in 0..50 {
            if !reopened.as_ref().is_err_and(|e| e.code == "WORKSPACE_BUSY") {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            reopened = Service::open(&root);
        }
        let s = reopened.unwrap();
        let (replay, workers) = s.store.claim_animation_set(&request, vec![]).unwrap();
        assert_eq!(replay.job_ids, set.job_ids);
        assert!(workers.is_empty());
        assert_eq!(
            s.dispatch("animations/sets/get", json!({"id":set.id}))
                .await
                .unwrap()["entries"]
                .as_array()
                .unwrap()
                .len(),
            48
        );
    }

    #[tokio::test]
    async fn sets_reuse_explicit_coverage_and_requeue_failed_or_deleted_cells() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Service::open(tmp.path()).unwrap();
        let (p, c) = fixture(&s).await;
        let mut request = input(&p, &c);
        request.motions = vec![Motion::Walk];
        let old = Animation {
            id: "legacy-name-is-not-facing-metadata".into(),
            project_id: p.id.clone(),
            character_id: Some(c.id.clone()),
            job_id: None,
            source_asset_id: None,
            config: AnimationConfig {
                name: "Walk North (N)".into(),
                motion: Motion::Walk,
                ..Default::default()
            },
            status: JobStatus::Succeeded,
            frames: vec![],
            preview_path: None,
            error: None,
            created_at: now(),
        };
        s.store
            .put("animation", &old.id, Some(&p.id), &old)
            .unwrap();
        let (set, jobs) = s
            .store
            .claim_animation_set(&request, s.animation_set_requests(&request).unwrap())
            .unwrap();
        assert_eq!(
            jobs.len(),
            8,
            "Legacy names must not satisfy any explicit facing"
        );
        let mut next = request.clone();
        next.idempotency_key = "reuse".into();
        let (reused, jobs) = s
            .store
            .claim_animation_set(&next, s.animation_set_requests(&next).unwrap())
            .unwrap();
        assert!(jobs.is_empty());
        assert!(reused.entries.iter().all(|e| e.is_reused));
        // Clip data is still QUEUED; its authoritative job has failed.
        let failed = &set.entries[0];
        let mut job = s.store.job(&failed.animation_id).unwrap();
        job.status = JobStatus::Failed;
        s.store.save_job(&job).unwrap();
        for entry in &set.entries[1..] {
            let mut job = s.store.job(&entry.animation_id).unwrap();
            job.status = JobStatus::Succeeded;
            s.store.save_job(&job).unwrap();
        }
        let deleted = &set.entries[1];
        s.store
            .delete_item(&Deletion {
                id: deleted.animation_id.clone(),
                kind: "animation".into(),
                name: "Test clip".into(),
                project_id: p.id.clone(),
            })
            .unwrap();
        next.idempotency_key = "fill-missing".into();
        let (filled, jobs) = s
            .store
            .claim_animation_set(&next, s.animation_set_requests(&next).unwrap())
            .unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(filled.entries.iter().filter(|e| e.is_reused).count(), 6);
        assert_ne!(filled.entries[0].animation_id, failed.animation_id);
        assert_ne!(filled.entries[1].animation_id, deleted.animation_id);
        assert!(s.store.get::<Animation>("animation", &old.id).is_ok());
    }
}
