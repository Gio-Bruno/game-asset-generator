use crate::{
    contract::*,
    error::{ApiError, Result},
};
use fs2::FileExt;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub struct Store {
    pub root: PathBuf,
    conn: Mutex<Connection>,
    _lease: fs::File,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn id() -> String {
    Uuid::new_v4().to_string()
}

impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root.join("assets")).map_err(ApiError::storage)?;
        fs::create_dir_all(root.join("work")).map_err(ApiError::storage)?;
        let lease = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("workspace.lock"))
            .map_err(ApiError::storage)?;
        lease.try_lock_exclusive().map_err(|_| {
            ApiError::new(
                "WORKSPACE_BUSY",
                "This workspace is already open. Close its other app or API process first.",
            )
        })?;
        let conn = Connection::open(root.join("workspace.sqlite")).map_err(ApiError::storage)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(ApiError::storage)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS entities(id TEXT PRIMARY KEY, kind TEXT NOT NULL, project_id TEXT, data TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS entities_project ON entities(kind, project_id);
            CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY, project_id TEXT NOT NULL, intent_key TEXT NOT NULL, request_hash TEXT NOT NULL, data TEXT NOT NULL, UNIQUE(project_id, intent_key));
            CREATE TABLE IF NOT EXISTS effects(intent_key TEXT PRIMARY KEY, request_hash TEXT NOT NULL, response TEXT);").map_err(ApiError::storage)?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS deleted_entities(id TEXT PRIMARY KEY, group_id TEXT NOT NULL, deleted_at INTEGER NOT NULL);")
            .map_err(ApiError::storage)?;
        let store = Self {
            root: fs::canonicalize(root).map_err(ApiError::storage)?,
            conn: Mutex::new(conn),
            _lease: lease,
        };
        // The workspace lease proves the previous owner exited. Never retry an uncertain inference.
        for mut job in store.list_jobs(None)? {
            if !job.status.is_terminal() {
                job.status = JobStatus::Unknown;
                job.error = Some(ApiError::new(
                    "OUTCOME_UNKNOWN",
                    "The app closed during generation. The request may have used subscription capacity. Start a new request deliberately to try again.",
                ));
                store.save_job(&job)?;
            }
        }
        let mut page = 1;
        loop {
            let sessions = store.list::<AssistantSession>("assistant", None, page, 100)?;
            for mut session in sessions.data {
                if session.status == AssistantStatus::Thinking {
                    session.status = AssistantStatus::Unknown;
                    session.error = Some(ApiError::new(
                        "OUTCOME_UNKNOWN",
                        "The guide closed mid-request. Check your workspace before asking it to continue.",
                    ));
                    store.put(
                        "assistant",
                        &session.id,
                        session.project_id.as_deref(),
                        &session,
                    )?;
                }
            }
            if page >= sessions.pagination.total_pages {
                break;
            }
            page += 1;
        }
        Ok(store)
    }

    pub fn put<T: Serialize>(
        &self,
        kind: &str,
        key: &str,
        project: Option<&str>,
        item: &T,
    ) -> Result<()> {
        let data = serde_json::to_string(item).map_err(ApiError::storage)?;
        self.conn.lock().unwrap().execute("INSERT INTO entities(id,kind,project_id,data) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET data=excluded.data,project_id=excluded.project_id", params![key,kind,project,data]).map_err(ApiError::storage)?;
        Ok(())
    }

    pub fn get<T: DeserializeOwned>(&self, kind: &str, key: &str) -> Result<T> {
        let data: Option<String> = self
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT data FROM entities e WHERE id=?1 AND kind=?2 AND NOT EXISTS (SELECT 1 FROM deleted_entities d WHERE d.id=e.id OR d.id=e.project_id)",
                params![key, kind],
                |r| r.get(0),
            )
            .optional()
            .map_err(ApiError::storage)?;
        serde_json::from_str(
            &data.ok_or_else(|| ApiError::new("NOT_FOUND", format!("{kind} was not found.")))?,
        )
        .map_err(ApiError::storage)
    }

    pub fn list<T: DeserializeOwned>(
        &self,
        kind: &str,
        project: Option<&str>,
        page: usize,
        page_size: usize,
    ) -> Result<Page<T>> {
        if page == 0 || !(1..=100).contains(&page_size) || page > 1_000_000 {
            return Err(ApiError::validation(
                "page must be positive and pageSize must be between 1 and 100.",
            ));
        }
        let conn = self.conn.lock().unwrap();
        let total: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM entities e WHERE kind=?1 AND (?2 IS NULL OR project_id=?2) AND NOT EXISTS (SELECT 1 FROM deleted_entities d WHERE d.id=e.id OR d.id=e.project_id)",
                params![kind, project],
                |r| r.get(0),
            )
            .map_err(ApiError::storage)?;
        let mut query = conn.prepare("SELECT data FROM entities e WHERE kind=?1 AND (?2 IS NULL OR project_id=?2) AND NOT EXISTS (SELECT 1 FROM deleted_entities d WHERE d.id=e.id OR d.id=e.project_id) ORDER BY rowid DESC LIMIT ?3 OFFSET ?4").map_err(ApiError::storage)?;
        let rows = query
            .query_map(
                params![kind, project, page_size, (page - 1) * page_size],
                |r| r.get::<_, String>(0),
            )
            .map_err(ApiError::storage)?;
        let mut data = Vec::new();
        for row in rows {
            data.push(
                serde_json::from_str(&row.map_err(ApiError::storage)?)
                    .map_err(ApiError::storage)?,
            );
        }
        Ok(Page {
            data,
            pagination: Pagination {
                page,
                page_size,
                total_items: total,
                total_pages: total.div_ceil(page_size),
            },
        })
    }

    pub fn validate_references(&self, project: &str, references: &[String]) -> Result<Vec<Asset>> {
        references
            .iter()
            .map(|key| {
                let asset: Asset = self.get("asset", key)?;
                if asset.project_id != project {
                    return Err(ApiError::validation(
                        "Reference images must belong to the same project.",
                    ));
                }
                Ok(asset)
            })
            .collect()
    }

    pub fn delete_item(&self, deletion: &Deletion) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(ApiError::storage)?;
        // A running guide or render owns snapshots of these records. Finish or stop it first.
        let jobs = {
            let mut q = tx
                .prepare("SELECT data FROM jobs WHERE project_id=?1")
                .map_err(ApiError::storage)?;
            q.query_map([&deletion.project_id], |r| r.get::<_, String>(0))
                .map_err(ApiError::storage)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(ApiError::storage)?
        };
        for data in jobs {
            let job: Job = serde_json::from_str(&data).map_err(ApiError::storage)?;
            if !job.status.is_terminal() {
                return Err(ApiError::new(
                    "PROJECT_BUSY",
                    "Finish or stop generation before deleting from this game.",
                ));
            }
        }
        let rows = {
            let mut q = tx.prepare("SELECT id,kind,data FROM entities e WHERE project_id=?1 AND NOT EXISTS (SELECT 1 FROM deleted_entities d WHERE d.id=e.id OR d.id=e.project_id)").map_err(ApiError::storage)?;
            q.query_map([&deletion.project_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(ApiError::storage)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(ApiError::storage)?
        };
        for (_, kind, data) in &rows {
            if kind == "assistant" {
                let session: AssistantSession =
                    serde_json::from_str(data).map_err(ApiError::storage)?;
                if session.status == AssistantStatus::Thinking {
                    return Err(ApiError::new(
                        "PROJECT_BUSY",
                        "Finish or stop Forge before deleting from this game.",
                    ));
                }
            }
        }
        if !rows
            .iter()
            .any(|(key, kind, _)| key == &deletion.id && kind == &deletion.kind)
        {
            return Err(ApiError::new("NOT_FOUND", "This item was already deleted."));
        }
        let mut hidden = vec![deletion.id.clone()];
        if deletion.kind == "asset" {
            for (key, kind, data) in rows {
                match kind.as_str() {
                    "animation" => {
                        let clip: Animation =
                            serde_json::from_str(&data).map_err(ApiError::storage)?;
                        if clip.source_asset_id.as_ref() == Some(&deletion.id) {
                            hidden.push(key);
                        }
                    }
                    "project" => {
                        let mut p: Project =
                            serde_json::from_str(&data).map_err(ApiError::storage)?;
                        p.style.reference_asset_ids.retain(|id| id != &deletion.id);
                        tx.execute(
                            "UPDATE entities SET data=?2 WHERE id=?1",
                            params![key, serde_json::to_string(&p).map_err(ApiError::storage)?],
                        )
                        .map_err(ApiError::storage)?;
                    }
                    "character" => {
                        let mut subject: Character =
                            serde_json::from_str(&data).map_err(ApiError::storage)?;
                        subject.reference_asset_ids.retain(|id| id != &deletion.id);
                        tx.execute(
                            "UPDATE entities SET data=?2 WHERE id=?1",
                            params![
                                key,
                                serde_json::to_string(&subject).map_err(ApiError::storage)?
                            ],
                        )
                        .map_err(ApiError::storage)?;
                    }
                    "assistant" => {
                        let mut session: AssistantSession =
                            serde_json::from_str(&data).map_err(ApiError::storage)?;
                        session.reference_asset_ids.retain(|id| id != &deletion.id);
                        tx.execute(
                            "UPDATE entities SET data=?2 WHERE id=?1",
                            params![
                                key,
                                serde_json::to_string(&session).map_err(ApiError::storage)?
                            ],
                        )
                        .map_err(ApiError::storage)?;
                    }
                    _ => {}
                }
            }
        }
        for key in hidden {
            tx.execute(
                "INSERT INTO deleted_entities(id,group_id,deleted_at) VALUES(?1,?2,?3)",
                params![key, deletion.id, now()],
            )
            .map_err(ApiError::storage)?;
        }
        tx.commit().map_err(ApiError::storage)
    }

    pub fn restore_item(&self, kind: &str, key: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(ApiError::storage)?;
        let project: Option<String> = tx
            .query_row(
                "SELECT project_id FROM entities WHERE id=?1 AND kind=?2",
                params![key, kind],
                |r| r.get(0),
            )
            .optional()
            .map_err(ApiError::storage)?;
        let project = project.ok_or_else(|| ApiError::new("NOT_FOUND", "Item was not found."))?;
        if kind != "project"
            && tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM deleted_entities WHERE id=?1)",
                    [&project],
                    |r| r.get::<_, bool>(0),
                )
                .map_err(ApiError::storage)?
        {
            return Err(ApiError::new(
                "PROJECT_DELETED",
                "Restore this game before restoring its assets.",
            ));
        }
        // Only the original delete target can restore its group (e.g. atlas and dependent clips).
        tx.execute("DELETE FROM deleted_entities WHERE group_id=?1", [key])
            .map_err(ApiError::storage)?;
        tx.commit().map_err(ApiError::storage)
    }

    pub fn claim_job(&self, request: GenerateInput) -> Result<(Job, bool)> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(ApiError::storage)?;
        let result = Self::claim_job_in(&tx, request)?;
        tx.commit().map_err(ApiError::storage)?;
        Ok(result)
    }

    pub fn claim_batch(
        &self,
        input: &GenerateBatchInput,
        requests: Vec<GenerateInput>,
    ) -> Result<(GenerationBatch, Vec<Job>)> {
        let key = format!("asset-batch:{}:{}", input.project_id, input.idempotency_key);
        let hash = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(input).map_err(ApiError::storage)?)
        );
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(ApiError::storage)?;
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT request_hash,response FROM effects WHERE intent_key=?1",
                [&key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(ApiError::storage)?;
        if let Some((previous_hash, data)) = existing {
            if previous_hash != hash {
                return Err(ApiError::new(
                    "IDEMPOTENCY_CONFLICT",
                    "This batch key was already used with different items.",
                ));
            }
            return Ok((
                serde_json::from_str(&data).map_err(ApiError::storage)?,
                vec![],
            ));
        }
        let mut jobs = Vec::with_capacity(requests.len());
        let mut new_jobs = vec![];
        for request in requests {
            let (job, is_new) = Self::claim_job_in(&tx, request)?;
            if is_new {
                new_jobs.push(job.clone());
            }
            jobs.push(job);
        }
        let batch = GenerationBatch {
            id: id(),
            project_id: input.project_id.clone(),
            job_ids: jobs.iter().map(|j| j.id.clone()).collect(),
            created_at: now(),
        };
        let data = serde_json::to_string(&batch).map_err(ApiError::storage)?;
        tx.execute(
            "INSERT INTO effects(intent_key,request_hash,response) VALUES(?1,?2,?3)",
            params![key, hash, data],
        )
        .map_err(ApiError::storage)?;
        tx.execute(
            "INSERT INTO entities(id,kind,project_id,data) VALUES(?1,'batch',?2,?3)",
            params![batch.id, batch.project_id, data],
        )
        .map_err(ApiError::storage)?;
        tx.commit().map_err(ApiError::storage)?;
        Ok((batch, new_jobs))
    }

    fn claim_job_in(tx: &rusqlite::Transaction<'_>, request: GenerateInput) -> Result<(Job, bool)> {
        let hash = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&request).map_err(ApiError::storage)?)
        );
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT request_hash,data FROM jobs WHERE project_id=?1 AND intent_key=?2",
                params![request.project_id, request.idempotency_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(ApiError::storage)?;
        if let Some((previous_hash, data)) = existing {
            if hash != previous_hash {
                return Err(ApiError::new(
                    "IDEMPOTENCY_CONFLICT",
                    "This idempotency key was already used with a different request.",
                ));
            }
            return Ok((
                serde_json::from_str(&data).map_err(ApiError::storage)?,
                false,
            ));
        }
        let project_data: String = tx
            .query_row(
                "SELECT data FROM entities WHERE kind='project' AND id=?1 AND NOT EXISTS(SELECT 1 FROM deleted_entities WHERE id=?1)",
                [&request.project_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(ApiError::storage)?
            .ok_or_else(|| ApiError::new("NOT_FOUND", "Project was not found."))?;
        let project: Project = serde_json::from_str(&project_data).map_err(ApiError::storage)?;
        let character = if let Some(key) = &request.character_id {
            let data: String = tx
                .query_row(
                    "SELECT data FROM entities WHERE kind='character' AND id=?1 AND NOT EXISTS(SELECT 1 FROM deleted_entities WHERE id=?1)",
                    [key],
                    |r| r.get(0),
                )
                .optional()
                .map_err(ApiError::storage)?
                .ok_or_else(|| ApiError::new("NOT_FOUND", "Character was not found."))?;
            let value: Character = serde_json::from_str(&data).map_err(ApiError::storage)?;
            if value.project_id != project.id {
                return Err(ApiError::validation(
                    "The character must belong to this project.",
                ));
            }
            Some(value)
        } else {
            None
        };
        let mut references = project.style.reference_asset_ids.clone();
        if let Some(character) = &character {
            references.extend(character.reference_asset_ids.clone());
        }
        references.extend(request.reference_asset_ids.clone());
        let mut seen = std::collections::HashSet::new();
        references.retain(|r| seen.insert(r.clone()));
        if references.len() > 8 {
            return Err(ApiError::validation(
                "The combined style, character and request references exceed 8 images.",
            ));
        }
        for key in &references {
            let owner: Option<String> = tx
                .query_row(
                    "SELECT project_id FROM entities WHERE kind='asset' AND id=?1 AND NOT EXISTS(SELECT 1 FROM deleted_entities WHERE id=?1)",
                    [key],
                    |r| r.get(0),
                )
                .optional()
                .map_err(ApiError::storage)?;
            if owner.as_deref() != Some(&project.id) {
                return Err(ApiError::validation(
                    "Reference images must exist in this project.",
                ));
            }
        }
        let job = Job {
            id: id(),
            project_id: project.id,
            status: JobStatus::Queued,
            style_snapshot: project.style,
            character_snapshot: character,
            reference_asset_ids: references,
            request,
            thread_id: None,
            turn_id: None,
            asset_ids: vec![],
            error: None,
            created_at: now(),
        };
        tx.execute(
            "INSERT INTO jobs(id,project_id,intent_key,request_hash,data) VALUES(?1,?2,?3,?4,?5)",
            params![
                job.id,
                job.project_id,
                job.request.idempotency_key,
                hash,
                serde_json::to_string(&job).map_err(ApiError::storage)?
            ],
        )
        .map_err(ApiError::storage)?;
        Ok((job, true))
    }

    pub fn save_job(&self, job: &Job) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE jobs SET data=?2 WHERE id=?1",
                params![
                    job.id,
                    serde_json::to_string(job).map_err(ApiError::storage)?
                ],
            )
            .map_err(ApiError::storage)?;
        Ok(())
    }
    pub fn pin_initial_asset(&self, job: &Job, asset: &Asset) -> Result<()> {
        let Some(snapshot) = &job.character_snapshot else {
            return Ok(());
        };
        if !matches!(
            (snapshot.kind, asset.kind),
            (SubjectKind::Character, AssetKind::Character)
                | (SubjectKind::Structure | SubjectKind::Prop, AssetKind::Prop)
                | (SubjectKind::Scene, AssetKind::Scene)
        ) {
            return Ok(());
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(ApiError::storage)?;
        let data: Option<String> = tx.query_row(
            "SELECT data FROM entities e WHERE id=?1 AND kind='character' AND project_id=?2 AND NOT EXISTS (SELECT 1 FROM deleted_entities d WHERE d.id=e.id OR d.id=e.project_id)",
            params![snapshot.id,asset.project_id], |r|r.get(0),
        ).optional().map_err(ApiError::storage)?;
        if let Some(data) = data {
            let mut subject: Character = serde_json::from_str(&data).map_err(ApiError::storage)?;
            // Do not replace existing references or pin a render of an identity edited while it ran.
            if subject.reference_asset_ids.is_empty()
                && subject.name == snapshot.name
                && subject.description == snapshot.description
                && subject.kind == snapshot.kind
            {
                subject.reference_asset_ids.push(asset.id.clone());
                tx.execute(
                    "UPDATE entities SET data=?2 WHERE id=?1",
                    params![
                        subject.id,
                        serde_json::to_string(&subject).map_err(ApiError::storage)?
                    ],
                )
                .map_err(ApiError::storage)?;
            }
        }
        tx.commit().map_err(ApiError::storage)
    }
    pub fn job(&self, key: &str) -> Result<Job> {
        let data: Option<String> = self
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT data FROM jobs WHERE id=?1", [key], |r| r.get(0))
            .optional()
            .map_err(ApiError::storage)?;
        serde_json::from_str(&data.ok_or_else(|| ApiError::new("NOT_FOUND", "Job was not found."))?)
            .map_err(ApiError::storage)
    }
    pub fn list_jobs(&self, project: Option<&str>) -> Result<Vec<Job>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn
            .prepare(
                "SELECT data FROM jobs j WHERE (?1 IS NULL OR project_id=?1) AND NOT EXISTS(SELECT 1 FROM deleted_entities d WHERE d.id=j.project_id) ORDER BY rowid DESC",
            )
            .map_err(ApiError::storage)?;
        let rows = q
            .query_map([project], |r| r.get::<_, String>(0))
            .map_err(ApiError::storage)?;
        rows.map(|r| {
            serde_json::from_str(&r.map_err(ApiError::storage)?).map_err(ApiError::storage)
        })
        .collect()
    }

    /// Atomic effect ledger for agent actions and message retries. Pending outcomes never replay.
    pub fn claim_effect(
        &self,
        key: &str,
        payload: &serde_json::Value,
    ) -> Result<Option<serde_json::Value>> {
        let hash = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(payload).map_err(ApiError::storage)?)
        );
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(ApiError::storage)?;
        let existing: Option<(String, Option<String>)> = tx
            .query_row(
                "SELECT request_hash,response FROM effects WHERE intent_key=?1",
                [key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(ApiError::storage)?;
        if let Some((previous, response)) = existing {
            if previous != hash {
                return Err(ApiError::new(
                    "IDEMPOTENCY_CONFLICT",
                    "This request key was reused with different input.",
                ));
            }
            return response.map(|r|serde_json::from_str(&r).map_err(ApiError::storage)).transpose()?.map(Some).ok_or_else(||ApiError::new("OUTCOME_UNKNOWN","This action is in progress or its result was not recorded. It will not be repeated automatically."));
        }
        tx.execute(
            "INSERT INTO effects(intent_key,request_hash) VALUES(?1,?2)",
            params![key, hash],
        )
        .map_err(ApiError::storage)?;
        tx.commit().map_err(ApiError::storage)?;
        Ok(None)
    }
    pub fn finish_effect(&self, key: &str, response: &serde_json::Value) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE effects SET response=?2 WHERE intent_key=?1",
                params![
                    key,
                    serde_json::to_string(response).map_err(ApiError::storage)?
                ],
            )
            .map_err(ApiError::storage)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, Store, Project) {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path()).unwrap();
        let project = Project {
            id: id(),
            name: "Test".into(),
            style: StyleGuide {
                name: "Ink".into(),
                description: "Black outlines".into(),
                ..Default::default()
            },
            created_at: now(),
        };
        store
            .put("project", &project.id, Some(&project.id), &project)
            .unwrap();
        (tmp, store, project)
    }
    fn input(project: &Project) -> GenerateInput {
        GenerateInput {
            project_id: project.id.clone(),
            idempotency_key: "one-intent".into(),
            prompt: "A scout".into(),
            kind: AssetKind::Character,
            character_id: None,
            reference_asset_ids: vec![],
            width: 512,
            height: 512,
            transparent_background: true,
            animation: None,
        }
    }
    #[test]
    fn retries_replay_and_payload_collisions_fail() {
        let (_tmp, store, p) = setup();
        let r = input(&p);
        let (a, new) = store.claim_job(r.clone()).unwrap();
        assert!(new);
        let (b, new) = store.claim_job(r.clone()).unwrap();
        assert!(!new);
        assert_eq!(a.id, b.id);
        let mut changed = r;
        changed.prompt = "A knight".into();
        assert_eq!(
            store.claim_job(changed).unwrap_err().code,
            "IDEMPOTENCY_CONFLICT"
        );
    }
    #[test]
    fn styles_are_snapshotted_and_restart_preserves_unknown() {
        let (tmp, store, mut p) = setup();
        let (job, _) = store.claim_job(input(&p)).unwrap();
        p.style.description = "Watercolor".into();
        store.put("project", &p.id, Some(&p.id), &p).unwrap();
        assert_eq!(
            store.job(&job.id).unwrap().style_snapshot.description,
            "Black outlines"
        );
        drop(store);
        let reopened = Store::open(tmp.path()).unwrap();
        assert_eq!(reopened.job(&job.id).unwrap().status, JobStatus::Unknown);
        let (_, new) = reopened.claim_job(input(&p)).unwrap();
        assert!(!new);
    }
    #[test]
    fn other_projects_cannot_supply_references() {
        let (_tmp, store, p) = setup();
        let mut r = input(&p);
        r.reference_asset_ids = vec!["missing".into()];
        assert_eq!(store.claim_job(r).unwrap_err().code, "VALIDATION_ERROR");
    }
    #[test]
    fn lease_prevents_second_owner() {
        let (tmp, _store, _p) = setup();
        assert_eq!(
            Store::open(tmp.path()).err().unwrap().code,
            "WORKSPACE_BUSY"
        );
    }
    #[test]
    fn interrupted_guide_and_claimed_effect_are_never_retried() {
        let (tmp, store, p) = setup();
        let session = AssistantSession {
            id: id(),
            project_id: Some(p.id.clone()),
            status: AssistantStatus::Thinking,
            messages: vec![],
            thread_id: None,
            turn_id: None,
            allow_generation: true,
            reference_asset_ids: vec![],
            generated_job_ids: vec![],
            turn_job_count: 0,
            pending_question: None,
            setup_approved: false,
            error: None,
            created_at: now(),
        };
        store
            .put("assistant", &session.id, Some(&p.id), &session)
            .unwrap();
        let payload = serde_json::json!({"tool":"generate_asset","prompt":"Mira idle"});
        assert!(
            store
                .claim_effect("crash-action", &payload)
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .claim_effect("completed-action", &payload)
                .unwrap()
                .is_none()
        );
        let result = serde_json::json!({"result":{"id":"already-applied"}});
        store.finish_effect("completed-action", &result).unwrap();
        drop(store);
        let reopened = Store::open(tmp.path()).unwrap();
        assert_eq!(
            reopened
                .get::<AssistantSession>("assistant", &session.id)
                .unwrap()
                .status,
            AssistantStatus::Unknown
        );
        assert_eq!(
            reopened
                .claim_effect("crash-action", &payload)
                .unwrap_err()
                .code,
            "OUTCOME_UNKNOWN"
        );
        assert_eq!(
            reopened.claim_effect("completed-action", &payload).unwrap(),
            Some(result)
        );
        assert_eq!(
            reopened
                .claim_effect("completed-action", &serde_json::json!({"prompt":"Changed"}))
                .unwrap_err()
                .code,
            "IDEMPOTENCY_CONFLICT"
        );
    }
}
