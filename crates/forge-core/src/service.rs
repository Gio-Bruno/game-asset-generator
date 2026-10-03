use crate::{
    codex::Codex,
    contract::*,
    error::{ApiError, Result},
    prompt::generation_prompt,
    store::{Store, id, now},
};
use base64::Engine;
use image::{GenericImageView, ImageReader};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{Mutex, Semaphore, broadcast, watch};

pub fn default_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("AssetForge")
}

pub fn default_export_dir() -> PathBuf {
    [dirs::download_dir(), dirs::document_dir(), dirs::home_dir()]
        .into_iter()
        .flatten()
        .find(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from("."))
}
#[derive(Clone)]
pub struct Service {
    pub(crate) store: Arc<Store>,
    codex: Arc<Mutex<Option<Arc<Codex>>>>,
    queue: Arc<Semaphore>,
    pub(crate) cancellations: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
    pub(crate) events: broadcast::Sender<Event>,
}

impl Service {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let (events, _) = broadcast::channel(128);
        Ok(Self {
            store: Arc::new(Store::open(root.as_ref())?),
            codex: Arc::new(Mutex::new(None)),
            queue: Arc::new(Semaphore::new(1)),
            cancellations: Default::default(),
            events,
        })
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }
    fn event(&self, kind: &str, job: Option<&str>, message: impl Into<String>) {
        let _ = self.events.send(Event {
            kind: kind.into(),
            job_id: job.map(String::from),
            session_id: None,
            message: message.into(),
        });
    }
    pub(crate) async fn codex(&self) -> Result<Arc<Codex>> {
        let mut state = self.codex.lock().await;
        if let Some(client) = state.as_ref().filter(|c| c.is_alive()) {
            return Ok(client.clone());
        }
        let client = Codex::start(&self.store.root.join("work")).await?;
        let mut events = client.notifications.subscribe();
        let tx = self.events.clone();
        tokio::spawn(async move {
            while let Ok(message) = events.recv().await {
                if message["method"] == "account/login/completed" {
                    let success = message["params"]["success"].as_bool().unwrap_or(false);
                    let _ = tx.send(Event {
                        kind: if success {
                            "ACCOUNT_CONNECTED"
                        } else {
                            "LOGIN_FAILED"
                        }
                        .into(),
                        job_id: None,
                        session_id: None,
                        message: if success {
                            "Your Codex account is connected.".into()
                        } else {
                            message["params"]["error"]
                                .as_str()
                                .unwrap_or("Sign-in was cancelled.")
                                .into()
                        },
                    });
                }
            }
        });
        *state = Some(client.clone());
        Ok(client)
    }

    /// All transports use this contract. Inputs are validated once at this boundary.
    pub fn dispatch<'a>(
        &'a self,
        method: &'a str,
        params: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send + 'a>> {
        Box::pin(async move {
            match method {
                method if method.starts_with("animations/") => {
                    self.animation_dispatch(method, params).await
                }
                "styles/presets/list" => encode(crate::presets::all()),
                "assistant/message" => {
                    let input: AssistantInput = decode(params)?;
                    encode(self.start_assistant(input).await?)
                }
                "assistant/get" => {
                    let p: IdInput = decode(params)?;
                    encode(self.store.get::<AssistantSession>("assistant", &p.id)?)
                }
                "assistant/list" => {
                    let p: ListInput = decode(params)?;
                    encode(self.store.list::<AssistantSession>(
                        "assistant",
                        p.project_id.as_deref(),
                        p.page,
                        p.page_size,
                    )?)
                }
                "assistant/cancel" => {
                    let p: IdInput = decode(params)?;
                    let session: AssistantSession = self.store.get("assistant", &p.id)?;
                    if let Some(tx) = self
                        .cancellations
                        .lock()
                        .await
                        .get(&format!("guide:{}", p.id))
                    {
                        let _ = tx.send(true);
                    }
                    encode(session)
                }
                "system/info" => Ok(
                    json!({"apiVersion":1,"version":env!("CARGO_PKG_VERSION"),"dataDir":self.store.root,"transport":"stdio","capabilities":["2D","STYLE_REFERENCES","CHARACTER_REFERENCES","PNG_EXPORT","CANCELLATION","STYLE_PRESETS","AI_GUIDE","SPRITE_ANIMATION","ANIMATION_ZIP_EXPORT"]}),
                ),
                "account/read" => encode(self.codex().await?.account().await?),
                "account/login/start" => encode(self.codex().await?.login().await?),
                "account/login/cancel" => {
                    let p: IdInput = decode(params)?;
                    self.codex()
                        .await?
                        .request("account/login/cancel", json!({"loginId":p.id}))
                        .await?;
                    Ok(json!({"isCancelled":true}))
                }
                "projects/create" => {
                    let p: CreateProject = decode(params)?;
                    nonempty("name", &p.name, 120)?;
                    validate_style(&p.style)?;
                    if !p.style.reference_asset_ids.is_empty() {
                        return Err(ApiError::validation(
                            "Create the project, then import its style references.",
                        ));
                    }
                    let project = Project {
                        id: id(),
                        name: p.name,
                        style: p.style,
                        created_at: now(),
                    };
                    self.store
                        .put("project", &project.id, Some(&project.id), &project)?;
                    encode(project)
                }
                "projects/list" => {
                    let p: ListInput = decode(params)?;
                    encode(
                        self.store
                            .list::<Project>("project", None, p.page, p.page_size)?,
                    )
                }
                "projects/get" => {
                    let p: IdInput = decode(params)?;
                    encode(self.store.get::<Project>("project", &p.id)?)
                }
                "projects/style/update" => {
                    let p: UpdateStyle = decode(params)?;
                    validate_style(&p.style)?;
                    self.store
                        .validate_references(&p.project_id, &p.style.reference_asset_ids)?;
                    let mut project: Project = self.store.get("project", &p.project_id)?;
                    project.style = p.style;
                    self.store
                        .put("project", &project.id, Some(&project.id), &project)?;
                    encode(project)
                }
                "characters/create" => {
                    let p: CreateCharacter = decode(params)?;
                    nonempty("name", &p.name, 120)?;
                    nonempty("description", &p.description, 8000)?;
                    let _: Project = self.store.get("project", &p.project_id)?;
                    if p.reference_asset_ids.len() > 8 {
                        return Err(ApiError::validation("Use at most 8 character references."));
                    }
                    self.store
                        .validate_references(&p.project_id, &p.reference_asset_ids)?;
                    let c = Character {
                        id: id(),
                        project_id: p.project_id,
                        name: p.name,
                        description: p.description,
                        reference_asset_ids: p.reference_asset_ids,
                    };
                    self.store
                        .put("character", &c.id, Some(&c.project_id), &c)?;
                    encode(c)
                }
                "characters/references/update" => {
                    let p: UpdateReferences = decode(params)?;
                    let mut c: Character = self.store.get("character", &p.id)?;
                    if p.reference_asset_ids.len() > 8 {
                        return Err(ApiError::validation("Use at most 8 character references."));
                    }
                    self.store
                        .validate_references(&c.project_id, &p.reference_asset_ids)?;
                    c.reference_asset_ids = p.reference_asset_ids;
                    self.store
                        .put("character", &c.id, Some(&c.project_id), &c)?;
                    encode(c)
                }
                "characters/list" => {
                    let p: ListInput = decode(params)?;
                    encode(self.store.list::<Character>(
                        "character",
                        p.project_id.as_deref(),
                        p.page,
                        p.page_size,
                    )?)
                }
                "assets/list" => {
                    let p: ListInput = decode(params)?;
                    encode(self.store.list::<Asset>(
                        "asset",
                        p.project_id.as_deref(),
                        p.page,
                        p.page_size,
                    )?)
                }
                "assets/get" => {
                    let p: IdInput = decode(params)?;
                    encode(self.store.get::<Asset>("asset", &p.id)?)
                }
                "assets/import" => {
                    let p: ImportInput = decode(params)?;
                    nonempty("name", &p.name, 120)?;
                    let _: Project = self.store.get("project", &p.project_id)?;
                    let data = read_image_file(Path::new(&p.path))?;
                    encode(self.save_asset(
                        NewAsset {
                            project: &p.project_id,
                            job: None,
                            character: None,
                            kind: p.kind,
                            name: &p.name,
                        },
                        &data,
                        None,
                        false,
                    )?)
                }
                "assets/export" => {
                    let p: ExportInput = decode(params)?;
                    let asset: Asset = self.store.get("asset", &p.id)?;
                    // create_new prevents silent destruction of an existing game asset.
                    let mut target = fs::OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(&p.path)
                        .map_err(|_| {
                            ApiError::new(
                                "EXPORT_ERROR",
                                "Choose a writable filename that does not already exist.",
                            )
                        })?;
                    let mut source = fs::File::open(&asset.path).map_err(ApiError::storage)?;
                    std::io::copy(&mut source, &mut target).map_err(ApiError::storage)?;
                    target.sync_all().map_err(ApiError::storage)?;
                    Ok(json!({"path":p.path,"assetId":asset.id}))
                }
                "jobs/create" => {
                    let input: GenerateInput = decode(params)?;
                    input.validate()?;
                    let (job, is_new) = self.store.claim_job(input)?;
                    self.ensure_animation(&job)?;
                    if is_new {
                        let (tx, rx) = watch::channel(false);
                        self.cancellations.lock().await.insert(job.id.clone(), tx);
                        let service = self.clone();
                        let work = job.clone();
                        tokio::spawn(async move {
                            service.run_job(work, rx).await;
                        });
                    }
                    encode(job)
                }
                "jobs/get" => {
                    let p: IdInput = decode(params)?;
                    encode(self.store.job(&p.id)?)
                }
                "jobs/list" => {
                    let p: ListInput = decode(params)?;
                    if p.page == 0 || !(1..=100).contains(&p.page_size) || p.page > 1_000_000 {
                        return Err(ApiError::validation("Invalid pagination."));
                    }
                    let jobs = self.store.list_jobs(p.project_id.as_deref())?;
                    let total = jobs.len();
                    let data = jobs
                        .into_iter()
                        .skip((p.page - 1) * p.page_size)
                        .take(p.page_size)
                        .collect();
                    encode(Page {
                        data,
                        pagination: Pagination {
                            page: p.page,
                            page_size: p.page_size,
                            total_items: total,
                            total_pages: total.div_ceil(p.page_size),
                        },
                    })
                }
                "jobs/cancel" => {
                    let p: IdInput = decode(params)?;
                    let job = self.store.job(&p.id)?;
                    if let Some(tx) = self.cancellations.lock().await.get(&p.id) {
                        let _ = tx.send(true);
                    }
                    encode(job)
                }
                _ => Err(ApiError::new(
                    "METHOD_NOT_FOUND",
                    format!("Unknown API method: {method}"),
                )),
            }
        })
    }

    async fn run_job(&self, mut job: Job, mut cancelled: watch::Receiver<bool>) {
        let result = tokio::select! {
            biased;
            _=cancelled.changed()=>Err(ApiError::new("CANCELLED","Generation cancelled. Work already performed may count toward your subscription limits.")),
            result=self.generate(&mut job)=>result,
        };
        if let Err(error) = result {
            if error.code == "CANCELLED" {
                if let (Some(thread), Some(turn)) = (&job.thread_id, &job.turn_id)
                    && let Some(client) = self.codex.lock().await.as_ref()
                {
                    let _ = client
                        .request("turn/interrupt", json!({"threadId":thread,"turnId":turn}))
                        .await;
                }
                job.status = JobStatus::Cancelled;
            } else if [
                "CODEX_DISCONNECTED",
                "CODEX_TIMEOUT",
                "GENERATION_TIMEOUT",
                "EVENTS_LOST",
                "PROTOCOL_ERROR",
            ]
            .contains(&error.code.as_str())
            {
                job.status = JobStatus::Unknown;
            } else {
                job.status = JobStatus::Failed;
            }
            job.error = Some(error);
        }
        if let Err(error) = self.store.save_job(&job) {
            eprintln!("Could not finish job record: {error}");
        }
        self.cancellations.lock().await.remove(&job.id);
        self.event(
            "JOB_FINISHED",
            Some(&job.id),
            job.error
                .as_ref()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "Your asset is ready.".into()),
        );
    }

    async fn generate(&self, job: &mut Job) -> Result<()> {
        let _permit = self
            .queue
            .acquire()
            .await
            .map_err(|_| ApiError::new("QUEUE_CLOSED", "Generation queue is closed."))?;
        let client = self.codex().await?;
        let account = client.account().await?;
        if !account.is_logged_in {
            return Err(ApiError::new(
                "AUTH_REQUIRED",
                "Connect your ChatGPT account before generating assets.",
            ));
        }
        if !account.can_generate_images {
            return Err(ApiError::new(
                "IMAGE_GENERATION_UNAVAILABLE",
                account
                    .message
                    .unwrap_or_else(|| "Image generation is not available.".into()),
            ));
        }
        job.status = JobStatus::Running;
        self.store.save_job(job)?;
        self.event(
            "JOB_RUNNING",
            Some(&job.id),
            "Preparing your style and references…",
        );
        let work = self.store.root.join("work").join(&job.id);
        fs::create_dir_all(&work).map_err(ApiError::storage)?;
        let thread=client.request("thread/start",json!({"cwd":work,"sandbox":"read-only","approvalPolicy":"never","ephemeral":true,"developerInstructions":"You are a 2D game asset artist. Use only native image generation. All attached images are visual references. Never use command execution, filesystem tools, web search, MCP, external apps, or subagents. Generate one image and finish.","config":{"features.shell_tool":false,"features.multi_agent":false,"features.apps":false}})).await?;
        job.thread_id = Some(
            thread["thread"]["id"]
                .as_str()
                .ok_or_else(|| {
                    ApiError::new("PROTOCOL_ERROR", "Codex returned an invalid thread.")
                })?
                .into(),
        );
        self.store.save_job(job)?;
        let mut notifications = client.notifications.subscribe();
        let mut input =
            vec![json!({"type":"text","text":generation_prompt(job),"text_elements":[]})];
        for asset in self
            .store
            .validate_references(&job.project_id, &job.reference_asset_ids)?
        {
            input.push(json!({"type":"localImage","path":asset.path}));
        }
        let turn = client
            .request(
                "turn/start",
                json!({"threadId":job.thread_id,"input":input}),
            )
            .await?;
        job.turn_id = Some(
            turn["turn"]["id"]
                .as_str()
                .ok_or_else(|| ApiError::new("PROTOCOL_ERROR", "Codex returned an invalid turn."))?
                .into(),
        );
        self.store.save_job(job)?;
        let mut images: HashMap<String, Value> = HashMap::new();
        let mut summary = String::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1200);
        loop {
            let notification=tokio::time::timeout_at(deadline,notifications.recv()).await.map_err(|_|ApiError::new("GENERATION_TIMEOUT","Generation timed out. Its outcome is unknown; it will not be retried automatically."))?
                .map_err(|_|ApiError::new("EVENTS_LOST","Codex progress was lost. The generation outcome is unknown."))?;
            let method = notification["method"].as_str().unwrap_or("");
            let p = &notification["params"];
            if method == "forge/disconnected" {
                return Err(ApiError::new(
                    "CODEX_DISCONNECTED",
                    "Codex stopped during generation.",
                ));
            }
            if p["threadId"].as_str() != job.thread_id.as_deref() {
                continue;
            }
            if method == "item/started" && p["item"]["type"] == "imageGeneration" {
                self.event(
                    "IMAGE_GENERATING",
                    Some(&job.id),
                    "Rendering your game asset…",
                );
            }
            if method == "item/completed" {
                let item = &p["item"];
                if item["type"] == "imageGeneration"
                    && let Some(id) = item["id"].as_str()
                {
                    images.insert(id.into(), item.clone());
                }
                if item["type"] == "agentMessage" {
                    summary = item["text"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(1200)
                        .collect();
                }
            }
            if method == "turn/completed" && p["turn"]["id"].as_str() == job.turn_id.as_deref() {
                let status = p["turn"]["status"]
                    .as_str()
                    .ok_or_else(|| ApiError::new("PROTOCOL_ERROR", "Invalid Codex turn status."))?;
                if status != "completed" {
                    return Err(ApiError::new(
                        "GENERATION_FAILED",
                        p["turn"]["error"]["message"]
                            .as_str()
                            .unwrap_or("Codex did not complete the generation."),
                    ));
                }
                if let Some(items) = p["turn"]["items"].as_array() {
                    for item in items {
                        if item["type"] == "imageGeneration"
                            && let Some(id) = item["id"].as_str()
                        {
                            images.insert(id.into(), item.clone());
                        }
                    }
                }
                break;
            }
        }
        if images.is_empty() {
            return Err(ApiError::new(
                "NO_IMAGE_GENERATED",
                if summary.is_empty() {
                    "Codex completed without generating an image. Check image generation access and try a new brief.".into()
                } else {
                    summary
                },
            ));
        }
        for item in images.values() {
            if item["status"].as_str() != Some("completed") {
                return Err(ApiError::new(
                    "IMAGE_GENERATION_FAILED",
                    "The image tool did not complete its output.",
                ));
            }
            if !item["failure"].is_null() {
                return Err(ApiError::new(
                    "IMAGE_GENERATION_FAILED",
                    item["failure"]["message"]
                        .as_str()
                        .unwrap_or("The image tool failed."),
                ));
            }
            let data = if let Some(value) = item["result"].as_str().filter(|v| !v.is_empty()) {
                base64::engine::general_purpose::STANDARD
                    .decode(
                        value
                            .strip_prefix("data:image/png;base64,")
                            .unwrap_or(value),
                    )
                    .map_err(|_| {
                        ApiError::new("INVALID_IMAGE", "Codex returned invalid image data.")
                    })?
            } else if let Some(path) = item["savedPath"].as_str() {
                let canonical = fs::canonicalize(path).map_err(ApiError::storage)?;
                if !canonical.starts_with(&work) {
                    return Err(ApiError::new(
                        "INVALID_IMAGE",
                        "Codex saved the image outside the generation workspace.",
                    ));
                }
                read_image_file(&canonical)?
            } else {
                return Err(ApiError::new(
                    "INVALID_IMAGE",
                    "Codex did not return image pixels.",
                ));
            };
            let name = job
                .character_snapshot
                .as_ref()
                .map(|c| c.name.as_str())
                .unwrap_or("Game asset");
            let pixel_art = job
                .style_snapshot
                .description
                .to_lowercase()
                .contains("pixel");
            let normalized = match job
                .request
                .animation
                .as_ref()
                .map(|config| crate::animation::normalize_grid(&data, config, pixel_art))
                .transpose()
            {
                Ok(data) => data,
                Err(error) => {
                    // Preserve a valid source sheet even if it cannot become a playable clip.
                    if let Ok(asset) = self.save_asset(
                        NewAsset {
                            project: &job.project_id,
                            job: Some(&job.id),
                            character: job.request.character_id.as_deref(),
                            kind: job.request.kind,
                            name,
                        },
                        &data,
                        None,
                        pixel_art,
                    ) {
                        job.asset_ids.push(asset.id);
                    }
                    return Err(error);
                }
            };
            let asset = self.save_asset(
                NewAsset {
                    project: &job.project_id,
                    job: Some(&job.id),
                    character: job.request.character_id.as_deref(),
                    kind: job.request.kind,
                    name,
                },
                normalized.as_deref().unwrap_or(&data),
                Some((
                    job.request.width,
                    job.request.height,
                    job.request.transparent_background,
                )),
                pixel_art,
            )?;
            let missing_alpha = job.request.transparent_background && !asset.has_alpha;
            job.asset_ids.push(asset.id.clone());
            if missing_alpha {
                return Err(ApiError::new(
                    "TRANSPARENCY_UNAVAILABLE",
                    "Codex returned an opaque image. It is saved in the library, but did not meet your transparency request.",
                ));
            }
            if job.request.animation.is_some() {
                self.finish_animation(job, &asset)?;
            }
        }
        job.status = JobStatus::Succeeded;
        Ok(())
    }

    fn save_asset(
        &self,
        input: NewAsset<'_>,
        data: &[u8],
        size: Option<(u32, u32, bool)>,
        pixel_art: bool,
    ) -> Result<Asset> {
        if data.len() > 50 * 1024 * 1024 {
            return Err(ApiError::new(
                "INVALID_IMAGE",
                "Images must be smaller than 50 MB.",
            ));
        }
        let mut reader = ImageReader::new(Cursor::new(data))
            .with_guessed_format()
            .map_err(|_| ApiError::new("INVALID_IMAGE", "Unsupported image file."))?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        limits.max_alloc = Some(256 * 1024 * 1024);
        reader.limits(limits);
        let mut decoded = reader.decode().map_err(|_| {
            ApiError::new(
                "INVALID_IMAGE",
                "Use a valid PNG, JPEG or WebP image no larger than 8192 pixels per side.",
            )
        })?;
        if let Some((width, height, transparent)) = size
            && (decoded.dimensions() != (width, height) || !transparent)
        {
            let filter = if pixel_art {
                image::imageops::FilterType::Nearest
            } else {
                image::imageops::FilterType::Lanczos3
            };
            let resized = if input.kind == AssetKind::Scene {
                decoded.resize_to_fill(width, height, filter)
            } else {
                decoded.resize(width, height, filter)
            }
            .to_rgba8();
            let fill = if transparent {
                image::Rgba([0, 0, 0, 0])
            } else {
                let p = resized.get_pixel(0, 0);
                image::Rgba([p[0], p[1], p[2], 255])
            };
            let mut canvas = image::RgbaImage::from_pixel(width, height, fill);
            image::imageops::overlay(
                &mut canvas,
                &resized,
                ((width - resized.width()) / 2) as i64,
                ((height - resized.height()) / 2) as i64,
            );
            decoded = image::DynamicImage::ImageRgba8(canvas);
        }
        let has_alpha = decoded.to_rgba8().pixels().any(|p| p[3] < 255);
        let (width, height) = decoded.dimensions();
        let key = id();
        let path = self.store.root.join("assets").join(format!("{key}.png"));
        decoded
            .save_with_format(&path, image::ImageFormat::Png)
            .map_err(ApiError::storage)?;
        let asset = Asset {
            id: key,
            project_id: input.project.into(),
            job_id: input.job.map(String::from),
            character_id: input.character.map(String::from),
            kind: input.kind,
            name: input.name.into(),
            path: path.to_string_lossy().into(),
            width,
            height,
            has_alpha,
            created_at: now(),
        };
        self.store
            .put("asset", &asset.id, Some(input.project), &asset)?;
        Ok(asset)
    }
}
struct NewAsset<'a> {
    project: &'a str,
    job: Option<&'a str>,
    character: Option<&'a str>,
    kind: AssetKind,
    name: &'a str,
}

fn read_image_file(path: &Path) -> Result<Vec<u8>> {
    let meta = fs::metadata(path)
        .map_err(|_| ApiError::validation("Reference image could not be opened."))?;
    if !meta.is_file() || meta.len() > 50 * 1024 * 1024 {
        return Err(ApiError::validation(
            "Use an image file smaller than 50 MB.",
        ));
    }
    fs::read(path).map_err(ApiError::storage)
}
fn decode<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|e| ApiError::validation(e.to_string()))
}
fn encode<T: Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(ApiError::storage)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdInput {
    id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateStyle {
    project_id: String,
    style: StyleGuide,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateReferences {
    id: String,
    reference_asset_ids: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImportInput {
    project_id: String,
    path: String,
    name: String,
    #[serde(default)]
    kind: AssetKind,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportInput {
    id: String,
    path: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn boundary_rejects_unknown_fields_and_invalid_palette() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Service::open(tmp.path()).unwrap();
        assert_eq!(s.dispatch("projects/create",json!({"name":"Game","style":{"name":"Style","description":"Ink","palette":["red"]}})).await.unwrap_err().code,"VALIDATION_ERROR");
        assert_eq!(
            s.dispatch("projects/list", json!({"pageSize":0}))
                .await
                .unwrap_err()
                .code,
            "VALIDATION_ERROR"
        );
        assert_eq!(
            s.dispatch("projects/list", json!({"typo":1}))
                .await
                .unwrap_err()
                .code,
            "VALIDATION_ERROR"
        );
    }
    #[tokio::test]
    async fn import_and_export_preserve_pixels_and_existing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Service::open(tmp.path().join("data")).unwrap();
        let p = s
            .dispatch(
                "projects/create",
                json!({"name":"Game","style":{"name":"Style","description":"Ink"}}),
            )
            .await
            .unwrap();
        let source = tmp.path().join("reference.png");
        image::RgbaImage::from_pixel(32, 48, image::Rgba([200, 40, 10, 128]))
            .save(&source)
            .unwrap();
        let asset = s
            .dispatch(
                "assets/import",
                json!({"projectId":p["id"],"path":source,"name":"Scout"}),
            )
            .await
            .unwrap();
        assert_eq!(asset["width"], 32);
        assert_eq!(asset["hasAlpha"], true);
        let out = tmp.path().join("export.png");
        s.dispatch("assets/export", json!({"id":asset["id"],"path":out}))
            .await
            .unwrap();
        assert_eq!(
            s.dispatch("assets/export", json!({"id":asset["id"],"path":out}))
                .await
                .unwrap_err()
                .code,
            "EXPORT_ERROR"
        );
        assert_eq!(image::open(out).unwrap().dimensions(), (32, 48));
    }
}
