use crate::{
    Service,
    contract::*,
    error::{ApiError, Result},
    presets,
};
use sha2::{Digest, Sha256};

impl Service {
    pub(crate) async fn generate_batch(
        &self,
        input: GenerateBatchInput,
    ) -> Result<GenerationBatch> {
        nonempty("idempotencyKey", &input.idempotency_key, 128)?;
        if !(2..=12).contains(&input.items.len()) {
            return Err(ApiError::validation(
                "A batch contains 2–12 separate subjects.",
            ));
        }
        let project: Project = self.store.get("project", &input.project_id)?;
        let mut subjects = std::collections::HashSet::new();
        let mut requests = Vec::with_capacity(input.items.len());
        for (index, item) in input.items.iter().enumerate() {
            if !subjects.insert(&item.character_id) {
                return Err(ApiError::validation(
                    "Include each subject once in a batch.",
                ));
            }
            let subject: Character = self.store.get("character", &item.character_id)?;
            if subject.project_id != input.project_id {
                return Err(ApiError::validation(
                    "Every subject must belong to this game.",
                ));
            }
            let kind = match subject.kind {
                SubjectKind::Character => AssetKind::Character,
                SubjectKind::Structure | SubjectKind::Prop => AssetKind::Prop,
                SubjectKind::Scene => AssetKind::Scene,
            };
            let (width, height) = presets::output_size(&project.style, kind);
            let key = format!(
                "batch:{:x}:{index}",
                Sha256::digest(input.idempotency_key.as_bytes())
            );
            let request = GenerateInput {
                project_id: input.project_id.clone(),
                idempotency_key: key,
                prompt: item.prompt.clone(),
                kind,
                character_id: Some(subject.id),
                reference_asset_ids: input.reference_asset_ids.clone(),
                width: item.width.unwrap_or(width),
                height: item.height.unwrap_or(height),
                transparent_background: kind != AssetKind::Scene,
                animation: None,
            };
            request.validate()?;
            requests.push(request);
        }
        // All jobs and the replay receipt commit together, before any worker is launched.
        let (batch, jobs) = self.store.claim_batch(&input, requests)?;
        for job in jobs {
            self.launch_job(job).await;
        }
        Ok(batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn batch_claim_is_atomic_and_replay_survives_restart() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        let service = Service::open(&root).unwrap();
        let make = |name| json!({"name":name,"style":{"name":"Ink","description":"Gray ink"}});
        let p = service
            .dispatch("projects/create", make("Game"))
            .await
            .unwrap();
        let other = service
            .dispatch("projects/create", make("Other"))
            .await
            .unwrap();
        let mut subjects = vec![];
        for project in [&p, &p, &other] {
            subjects.push(service.dispatch("characters/create", json!({"projectId":project["id"],"name":"Tower","description":"Stone tower","kind":"STRUCTURE"})).await.unwrap());
        }
        let input: GenerateBatchInput = serde_json::from_value(json!({"projectId":p["id"],"idempotencyKey":"separate-towers","items":[{"characterId":subjects[0]["id"],"prompt":"First tower"},{"characterId":subjects[1]["id"],"prompt":"Second tower"}]})).unwrap();
        let request = |subject: &serde_json::Value, key: &str| GenerateInput {
            project_id: p["id"].as_str().unwrap().into(),
            idempotency_key: key.into(),
            prompt: "One isolated tower".into(),
            kind: AssetKind::Prop,
            character_id: Some(subject["id"].as_str().unwrap().into()),
            reference_asset_ids: vec![],
            width: 512,
            height: 512,
            transparent_background: true,
            animation: None,
        };
        assert!(
            service
                .store
                .claim_batch(
                    &input,
                    vec![
                        request(&subjects[0], "first"),
                        request(&subjects[2], "second")
                    ]
                )
                .is_err()
        );
        assert!(
            service.store.list_jobs(None).unwrap().is_empty(),
            "An invalid last item must roll back the first job"
        );
        let (batch, workers) = service
            .store
            .claim_batch(
                &input,
                vec![
                    request(&subjects[0], "first"),
                    request(&subjects[1], "second"),
                ],
            )
            .unwrap();
        assert_eq!(workers.len(), 2);
        assert_ne!(batch.job_ids[0], batch.job_ids[1]);
        assert_eq!(
            workers[0].character_snapshot.as_ref().unwrap().id,
            subjects[0]["id"]
        );
        let (replayed, workers) = service.store.claim_batch(&input, vec![]).unwrap();
        assert_eq!(replayed.job_ids, batch.job_ids);
        assert!(workers.is_empty());
        let mut changed = input.clone();
        changed.items.pop();
        assert_eq!(
            service
                .store
                .claim_batch(&changed, vec![])
                .unwrap_err()
                .code,
            "IDEMPOTENCY_CONFLICT"
        );
        drop(service);
        let service = Service::open(root).unwrap();
        let (again, workers) = service.store.claim_batch(&input, vec![]).unwrap();
        assert_eq!(again.job_ids, batch.job_ids);
        assert!(
            workers.is_empty(),
            "Interrupted work must not be resubmitted"
        );
        assert_eq!(service.store.list_jobs(None).unwrap().len(), 2);
        assert_eq!(
            service
                .dispatch("jobs/batch/get", json!({"id":batch.id}))
                .await
                .unwrap()["jobIds"],
            json!(batch.job_ids)
        );
    }

    #[tokio::test]
    async fn concept_reference_reclassification_keeps_pixels_and_clears_false_identity() {
        let tmp = tempfile::tempdir().unwrap();
        let service = Service::open(tmp.path().join("data")).unwrap();
        let p = service
            .dispatch(
                "projects/create",
                json!({"name":"Game","style":{"name":"Ink","description":"Ink"}}),
            )
            .await
            .unwrap();
        let path = tmp.path().join("sheet.png");
        image::RgbaImage::from_pixel(64, 64, image::Rgba([90, 90, 90, 255]))
            .save(&path)
            .unwrap();
        let value = service
            .dispatch(
                "assets/import",
                json!({"projectId":p["id"],"path":path,"name":"Wrong character","kind":"SCENE"}),
            )
            .await
            .unwrap();
        let mut asset: Asset = serde_json::from_value(value).unwrap();
        asset.character_id = Some("incorrect-single-subject".into());
        service
            .store
            .put("asset", &asset.id, Some(&asset.project_id), &asset)
            .unwrap();
        let before = std::fs::read(&asset.path).unwrap();
        let updated = service
            .dispatch(
                "assets/update",
                json!({"id":asset.id,"name":"Concept reference","kind":"CONCEPT_SHEET"}),
            )
            .await
            .unwrap();
        assert!(updated["characterId"].is_null());
        assert_eq!(updated["name"], "Concept reference");
        assert_eq!(std::fs::read(&asset.path).unwrap(), before);
        let bad:GenerateInput=serde_json::from_value(json!({"projectId":p["id"],"idempotencyKey":"bad","prompt":"A board","kind":"CONCEPT_SHEET","characterId":"incorrect-single-subject"})).unwrap();
        assert_eq!(bad.validate().unwrap_err().code, "VALIDATION_ERROR");
        let duplicate:GenerateBatchInput=serde_json::from_value(json!({"projectId":p["id"],"idempotencyKey":"duplicate","items":[{"characterId":"absent","prompt":"One"}]})).unwrap();
        assert_eq!(
            service.generate_batch(duplicate).await.unwrap_err().code,
            "VALIDATION_ERROR"
        );
        assert!(service.store.list_jobs(None).unwrap().is_empty());
    }
}
