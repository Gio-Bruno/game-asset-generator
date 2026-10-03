use crate::{Command, Response};
use forge_core::contract::{Animation, Asset};
use forge_core::{contract::*, error::Result};
use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    Disableable, Sizable,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::mpsc::Receiver,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const PAPER: u32 = 0xf8f6f0;
const INK: u32 = 0x272d29;
const MUTED: u32 = 0x737c72;
const LINE: u32 = 0xdadbd2;
const GREEN: u32 = 0x2f5548;
const PAGE_SIZE: usize = 24;

#[derive(Clone, Copy, PartialEq)]
enum View {
    Library,
    World,
}
#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Images,
    Animations,
}

pub struct Studio {
    commands: tokio::sync::mpsc::UnboundedSender<Command>,
    responses: Receiver<Response>,
    projects: Vec<Project>,
    projects_loaded: bool,
    project: Option<Project>,
    subjects: Vec<Character>,
    assets: Vec<Asset>,
    cache: HashMap<String, Asset>,
    selected: Option<String>,
    references: Vec<String>,
    animations: Vec<Animation>,
    animation_cache: HashMap<String, Animation>,
    animation_id: Option<String>,
    animations_loaded: bool,
    follow_new_clips: bool,
    guide_started_at: u64,
    ready_jobs: HashSet<String>,
    view: View,
    filter: Filter,
    asset_page: usize,
    asset_total: usize,
    animation_page: usize,
    animation_total: usize,
    subject_page: usize,
    subject_total: usize,
    account: Option<AccountStatus>,
    login: Option<Login>,
    job: Option<Job>,
    status: String,
    is_error: bool,
    guide: Option<AssistantSession>,
    guide_input: Entity<InputState>,
    guide_scroll: ScrollHandle,
    guide_stream: String,
    guide_actions: Vec<String>,
    guide_submitting: bool,
    show_atlas: bool,
    playing: bool,
    play_started: Instant,
    frame_index: usize,
}

fn label(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(MUTED))
        .child(text.into())
}

impl Studio {
    pub fn new(
        commands: tokio::sync::mpsc::UnboundedSender<Command>,
        responses: Receiver<Response>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let studio = Self {
            commands,
            responses,
            projects: vec![],
            projects_loaded: false,
            project: None,
            subjects: vec![],
            assets: vec![],
            cache: HashMap::new(),
            selected: None,
            references: vec![],
            animations: vec![],
            animation_cache: HashMap::new(),
            animation_id: None,
            animations_loaded: false,
            follow_new_clips: false,
            guide_started_at: 0,
            ready_jobs: HashSet::new(),
            view: View::Library,
            filter: Filter::All,
            asset_page: 1,
            asset_total: 0,
            animation_page: 1,
            animation_total: 0,
            subject_page: 1,
            subject_total: 0,
            account: None,
            login: None,
            job: None,
            status: "Tell Forge about your game to begin.".into(),
            is_error: false,
            guide: None,
            guide_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Ask Forge to create or change anything…")
                    .multi_line(true)
                    .rows(3)
            }),
            guide_scroll: ScrollHandle::new(),
            guide_stream: String::new(),
            guide_actions: vec![],
            guide_submitting: false,
            show_atlas: false,
            playing: true,
            play_started: Instant::now(),
            frame_index: 0,
        };
        studio.send("projects/list", json!({"pageSize":100}));
        studio.send("account/read", json!({}));
        cx.spawn_in(window, async move |entity, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                if entity
                    .update_in(cx, |studio, window, cx| studio.receive(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        studio
    }
    fn send(&self, method: &str, params: Value) {
        let _ = self.commands.send(Command {
            method: method.into(),
            params,
        });
    }
    fn message(&mut self, text: impl Into<String>, error: bool, cx: &mut Context<Self>) {
        self.status = text.into();
        self.is_error = error;
        cx.notify();
    }
    fn project_id(&self) -> Option<String> {
        self.project.as_ref().map(|p| p.id.clone())
    }
    fn guide_busy(&self) -> bool {
        self.guide_submitting
            || self
                .guide
                .as_ref()
                .is_some_and(|g| g.status == AssistantStatus::Thinking)
    }
    fn rendering(&self) -> bool {
        self.job.as_ref().is_some_and(|j| !j.status.is_terminal())
    }
    fn refresh(&self) {
        if let Some(id) = self.project_id() {
            self.send(
                "assets/list",
                json!({"projectId":id,"page":self.asset_page,"pageSize":PAGE_SIZE}),
            );
            self.send(
                "characters/list",
                json!({"projectId":id,"page":self.subject_page,"pageSize":PAGE_SIZE}),
            );
            self.send(
                "animations/list",
                json!({"projectId":id,"page":self.animation_page,"pageSize":PAGE_SIZE}),
            );
            if let Some(clip) = &self.animation_id {
                self.send("animations/get", json!({"id":clip}));
            }
            self.send("jobs/list", json!({"projectId":id,"pageSize":1}));
        }
    }
    fn reset_library(&mut self) {
        self.subjects.clear();
        self.assets.clear();
        self.cache.clear();
        self.selected = None;
        self.references.clear();
        self.animations.clear();
        self.animation_cache.clear();
        self.animation_id = None;
        self.animations_loaded = false;
        self.ready_jobs.clear();
        self.job = None;
        self.asset_page = 1;
        self.asset_total = 0;
        self.animation_page = 1;
        self.animation_total = 0;
        self.subject_page = 1;
        self.subject_total = 0;
        self.view = View::Library;
        self.filter = Filter::All;
    }
    fn choose_project(&mut self, project: Project, cx: &mut Context<Self>) {
        let changed = self.project_id().as_deref() != Some(&project.id);
        if changed {
            self.reset_library();
            if self
                .guide
                .as_ref()
                .is_some_and(|g| g.project_id.as_deref() != Some(&project.id))
            {
                self.guide = None;
                self.guide_stream.clear();
                self.guide_actions.clear();
            }
        }
        self.project = Some(project);
        if changed && self.guide.is_none() {
            self.send(
                "assistant/list",
                json!({"projectId":self.project_id(),"pageSize":1}),
            );
        }
        self.refresh();
        cx.notify();
    }
    fn new_game(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reset_library();
        self.project = None;
        self.projects_loaded = true;
        self.guide = None;
        self.follow_new_clips = false;
        self.guide_stream.clear();
        self.guide_actions.clear();
        self.guide_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.message(
            "Tell Forge about your next game. Your other games are saved.",
            false,
            cx,
        );
    }
    fn clip(&self) -> Option<&Animation> {
        self.animation_id
            .as_ref()
            .and_then(|id| self.animation_cache.get(id))
    }
    fn selected_asset(&self) -> Option<&Asset> {
        let id = self
            .clip()
            .and_then(|a| a.source_asset_id.as_ref())
            .or(self.selected.as_ref())?;
        self.cache.get(id)
    }
    fn choose_asset(&mut self, id: String, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.animation_id = None;
        self.view = View::Library;
        cx.notify();
    }
    fn choose_clip(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(clip) = self.animation_cache.get(&id) {
            if let Some(source) = &clip.source_asset_id {
                self.send("assets/get", json!({"id":source}));
            }
            self.animation_id = Some(id);
            self.selected = None;
            self.show_atlas = false;
            self.playing = true;
            self.play_started = Instant::now();
            self.frame_index = 0;
            self.view = View::Library;
            cx.notify();
        }
    }
    fn update_clip(&mut self, clip: Animation, cx: &mut Context<Self>) {
        let before = self.animation_cache.get(&clip.id).cloned();
        let selected = self.animation_id.as_ref() == Some(&clip.id);
        self.animation_cache.insert(clip.id.clone(), clip.clone());
        if selected {
            if let Some(source) = &clip.source_asset_id
                && !self.cache.contains_key(source)
            {
                self.send("assets/get", json!({"id":source}));
            }
            if before
                .as_ref()
                .is_some_and(|old| old.status != JobStatus::Succeeded)
                && clip.status == JobStatus::Succeeded
            {
                self.choose_clip(clip.id, cx);
            } else if before.as_ref().is_some_and(|old| {
                old.config.fps != clip.config.fps || old.config.is_looping != clip.config.is_looping
            }) {
                self.play_started = Instant::now();
                self.frame_index = 0;
            }
        }
    }
    fn receive(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut changed = false;
        while let Ok(response) = self.responses.try_recv() {
            changed = true;
            match response {
                Response::Reply(method, params, result) => {
                    self.reply(&method, &params, result, window, cx)
                }
                Response::Event(event) => {
                    match event.kind.as_str() {
                        "ACCOUNT_CONNECTED" => {
                            self.login = None;
                            self.send("account/read", json!({}));
                            self.message(
                                "Codex connected. Tell Forge what you want to make.",
                                false,
                                cx,
                            );
                        }
                        "LOGIN_FAILED" => {
                            self.login = None;
                            self.message(event.message.clone(), true, cx);
                        }
                        "RESYNC_REQUIRED" => self.refresh(),
                        "JOB_RUNNING" | "JOB_FINISHED" => {
                            self.refresh();
                            if let Some(id) = &event.job_id {
                                self.send("jobs/get", json!({"id":id}));
                            }
                        }
                        _ => {}
                    }
                    if event.session_id.as_deref() == self.guide.as_ref().map(|g| g.id.as_str()) {
                        match event.kind.as_str() {
                            "ASSISTANT_DELTA" => {
                                self.guide_stream.push_str(&event.message);
                                self.guide_scroll.scroll_to_bottom();
                            }
                            "ASSISTANT_ACTION" => {
                                self.guide_actions.push(event.message);
                                self.send("assistant/get", json!({"id":event.session_id}));
                            }
                            "ASSISTANT_FINISHED" => {
                                self.send("assistant/get", json!({"id":event.session_id}));
                                self.refresh();
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        if self.view == View::Library
            && self.playing
            && !self.show_atlas
            && let Some(clip) = self.clip()
            && !clip.frames.is_empty()
        {
            let elapsed =
                (self.play_started.elapsed().as_secs_f64() * clip.config.fps as f64) as usize;
            let next = if clip.config.is_looping {
                elapsed % clip.frames.len()
            } else {
                elapsed.min(clip.frames.len() - 1)
            };
            let finished = !clip.config.is_looping && elapsed >= clip.frames.len();
            changed |= next != self.frame_index || finished;
            self.frame_index = next;
            if finished {
                self.playing = false;
            }
        }
        if changed {
            cx.notify();
        }
    }
    fn reply(
        &mut self,
        method: &str,
        params: &Value,
        result: Result<Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if [
            "assets/list",
            "assets/import",
            "characters/list",
            "animations/list",
            "jobs/list",
            "assistant/list",
        ]
        .contains(&method)
            && params["projectId"].as_str() != self.project_id().as_deref()
        {
            return;
        }
        if method == "assistant/get"
            && params["id"].as_str() != self.guide.as_ref().map(|g| g.id.as_str())
        {
            return;
        }
        if method == "projects/get"
            && params["id"].as_str() != self.project_id().as_deref()
            && params["id"].as_str() != self.guide.as_ref().and_then(|g| g.project_id.as_deref())
        {
            return;
        }
        let data = match result {
            Ok(data) => data,
            Err(error) => {
                if method == "assistant/message" {
                    self.guide_submitting = false;
                }
                self.message(error.message, true, cx);
                return;
            }
        };
        match method {
            "projects/list" => {
                if let Ok(page) = serde_json::from_value::<Page<Project>>(data) {
                    self.projects = page.data;
                    let initial_load = !self.projects_loaded;
                    self.projects_loaded = true;
                    if initial_load
                        && self.project.is_none()
                        && self.guide.is_none()
                        && let Some(project) = self.projects.first().cloned()
                    {
                        self.choose_project(project, cx);
                    }
                }
            }
            "projects/get" => {
                if let Ok(project) = serde_json::from_value::<Project>(data) {
                    if let Some(saved) = self.projects.iter_mut().find(|p| p.id == project.id) {
                        *saved = project.clone();
                    } else {
                        self.projects.push(project.clone());
                    }
                    self.choose_project(project, cx);
                }
            }
            "characters/list" => {
                if let Ok(page) = serde_json::from_value::<Page<Character>>(data) {
                    if params["page"].as_u64().unwrap_or(1) as usize != self.subject_page {
                        return;
                    }
                    self.subject_total = page.pagination.total_items;
                    self.subjects = page.data;
                }
            }
            "assets/list" => {
                if let Ok(page) = serde_json::from_value::<Page<Asset>>(data) {
                    if params["page"].as_u64().unwrap_or(1) as usize != self.asset_page {
                        return;
                    }
                    self.asset_total = page.pagination.total_items;
                    for asset in &page.data {
                        self.cache.insert(asset.id.clone(), asset.clone());
                    }
                    self.assets = page.data;
                    if self.selected.is_none() && self.animation_id.is_none() {
                        self.selected = self.assets.first().map(|a| a.id.clone());
                    }
                }
            }
            "assets/get" | "assets/import" => {
                if let Ok(asset) = serde_json::from_value::<Asset>(data) {
                    if Some(asset.project_id.as_str()) != self.project_id().as_deref() {
                        return;
                    }
                    let id = asset.id.clone();
                    self.cache.insert(id.clone(), asset);
                    if method == "assets/import" {
                        self.add_to_chat(id, cx);
                        self.asset_page = 1;
                        self.refresh();
                        self.message(
                            "Image added to chat. Tell Forge how to use or revise it.",
                            false,
                            cx,
                        );
                    }
                }
            }
            "animations/list" => {
                if let Ok(page) = serde_json::from_value::<Page<Animation>>(data) {
                    if params["page"].as_u64().unwrap_or(1) as usize != self.animation_page {
                        return;
                    }
                    let new_clip = page
                        .data
                        .iter()
                        .find(|clip| {
                            self.follow_new_clips
                                && clip.created_at >= self.guide_started_at
                                && !self.animation_cache.contains_key(&clip.id)
                                && (self.animations_loaded || self.guide_busy())
                        })
                        .map(|clip| clip.id.clone());
                    self.animations_loaded = true;
                    self.animation_total = page.pagination.total_items;
                    self.animations = page.data.clone();
                    for clip in page.data {
                        self.update_clip(clip, cx);
                    }
                    if let Some(id) = new_clip {
                        self.follow_new_clips = false;
                        self.choose_clip(id, cx);
                    }
                }
            }
            "animations/get" => {
                if let Ok(clip) = serde_json::from_value::<Animation>(data)
                    && Some(clip.project_id.as_str()) == self.project_id().as_deref()
                {
                    self.update_clip(clip, cx);
                }
            }
            "jobs/get" => {
                if let Ok(job) = serde_json::from_value::<Job>(data) {
                    if Some(job.project_id.as_str()) != self.project_id().as_deref() {
                        return;
                    }
                    if let Some(id) = job.asset_ids.first()
                        && self.ready_jobs.insert(job.id.clone())
                    {
                        self.send("assets/get", json!({"id":id}));
                        if job.request.animation.is_some() {
                            self.animation_id = Some(job.id.clone());
                            self.selected = None;
                            self.show_atlas = false;
                            self.playing = true;
                            self.play_started = Instant::now();
                            self.frame_index = 0;
                            self.send("animations/get", json!({"id":job.id}));
                        } else {
                            self.selected = Some(id.clone());
                            self.animation_id = None;
                        }
                        self.asset_page = 1;
                        self.animation_page = 1;
                        self.refresh();
                        self.view = View::Library;
                        self.filter = Filter::All;
                    }
                    if let Some(error) = &job.error {
                        self.message(&error.message, true, cx);
                    } else if job.status == JobStatus::Succeeded {
                        self.message("Your asset is ready.", false, cx);
                    } else if !job.status.is_terminal() {
                        self.message("Forge is creating your asset…", false, cx);
                    }
                    self.job = Some(job);
                }
            }
            "jobs/list" => {
                if let Ok(page) = serde_json::from_value::<Page<Job>>(data) {
                    self.job = page.data.into_iter().next();
                }
            }
            "assistant/message" | "assistant/get" => {
                if let Ok(session) = serde_json::from_value::<AssistantSession>(data) {
                    if method == "assistant/message" {
                        self.guide_submitting = false;
                        if let Ok(submitted) = serde_json::from_value::<Vec<String>>(
                            params["referenceAssetIds"].clone(),
                        ) {
                            self.references.retain(|id| !submitted.contains(id));
                        }
                        self.guide_input
                            .update(cx, |input, cx| input.set_value("", window, cx));
                    }
                    let project = session.project_id.clone();
                    let job = session
                        .generated_job_ids
                        .last()
                        .filter(|id| {
                            self.guide.as_ref().and_then(|g| g.generated_job_ids.last()) != Some(id)
                        })
                        .cloned();
                    if session.status != AssistantStatus::Thinking {
                        self.guide_stream.clear();
                    }
                    if let Some(error) = &session.error {
                        self.message(&error.message, true, cx);
                    }
                    // Preserve the active conversation before following its newly created game.
                    self.guide = Some(session);
                    if let Some(id) = project {
                        self.send("projects/get", json!({"id":id}));
                    }
                    if let Some(id) = job {
                        self.send("jobs/get", json!({"id":id}));
                    }
                    self.guide_scroll.scroll_to_bottom();
                }
            }
            "assistant/list" => {
                if let Ok(page) = serde_json::from_value::<Page<AssistantSession>>(data)
                    && self.guide.is_none()
                {
                    self.guide = page.data.into_iter().next();
                    if let Some(guide) = &self.guide
                        && let Some(id) = guide.generated_job_ids.last()
                    {
                        self.send("jobs/get", json!({"id":id}));
                    }
                }
            }
            "account/read" => {
                if let Ok(account) = serde_json::from_value(data) {
                    self.account = Some(account);
                }
            }
            "account/login/start" => {
                if let Ok(login) = serde_json::from_value::<Login>(data) {
                    match open::that(&login.auth_url) {
                        Ok(()) => {
                            self.message("Complete Codex sign-in in your browser.", false, cx)
                        }
                        Err(error) => self.message(
                            format!(
                                "Could not open sign-in: {error}. Click Connect Codex to retry."
                            ),
                            true,
                            cx,
                        ),
                    }
                    self.login = Some(login);
                }
            }
            "account/login/cancel" => {
                self.login = None;
                self.message("Sign-in cancelled.", false, cx);
            }
            "assets/export" => self.message("PNG saved to your chosen folder.", false, cx),
            "animations/export" => self.message(
                "Clip saved with atlas, frames, GIF and timing JSON.",
                false,
                cx,
            ),
            _ => {}
        }
    }
    fn ask_guide(&mut self, cx: &mut Context<Self>) {
        let text = self.guide_input.read(cx).value().to_string();
        if self.guide_busy() || text.trim().is_empty() {
            return;
        }
        self.guide_stream.clear();
        self.guide_actions.clear();
        self.guide_submitting = true;
        self.animation_page = 1;
        self.follow_new_clips = true;
        self.guide_started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.send(
            "assistant/message",
            json!({
                "requestId":uuid::Uuid::new_v4().to_string(),
                "sessionId":self.guide.as_ref().map(|g|&g.id),
                "projectId":self.project_id(),
                "message":text,
                "allowGeneration":true,
                "referenceAssetIds":self.references,
            }),
        );
        self.refresh();
        cx.notify();
    }
    fn add_to_chat(&mut self, id: String, cx: &mut Context<Self>) {
        if self.references.contains(&id) {
            return;
        }
        if self.references.len() >= 8 {
            self.message(
                "Chat can include up to eight images. Remove one before adding another.",
                true,
                cx,
            );
            return;
        }
        if !self.cache.contains_key(&id) {
            self.send("assets/get", json!({"id":id}));
        }
        self.references.push(id);
        self.message(
            "Added to chat. Describe the change you want Forge to make.",
            false,
            cx,
        );
    }
    fn discuss_subject(&mut self, subject: Character, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.guide_input.read(cx).value().to_string();
        self.guide_input.update(cx, |input, cx| {
            input.set_value(
                format!(
                    "For {} ({}): {}",
                    subject.name,
                    subject.kind.label().to_lowercase(),
                    draft
                ),
                window,
                cx,
            )
        });
        for id in subject.reference_asset_ids {
            self.add_to_chat(id, cx);
        }
        cx.notify();
    }
    fn attach_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project_id() else {
            self.message(
                "Ask Forge to create your game first, then attach its reference images.",
                false,
                cx,
            );
            return;
        };
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Add a PNG, JPEG or WebP image to chat".into()),
        });
        cx.spawn_in(window, async move |entity, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.first()
            {
                let path = path.clone();
                let _ = entity.update_in(cx, |studio, _, _| {
                    studio.send(
                        "assets/import",
                        json!({
                            "projectId":project,"path":path,
                            "name":path.file_stem().unwrap_or_default().to_string_lossy(),
                            "kind":"PROP",
                        }),
                    );
                });
            }
        })
        .detach();
    }
    fn download(&mut self, clip: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (method, id, name, extension) = if clip {
            let Some(clip) = self.clip() else { return };
            (
                "animations/export",
                clip.id.clone(),
                clip.config.name.clone(),
                "zip",
            )
        } else {
            let Some(asset) = self.selected_asset() else {
                return;
            };
            ("assets/export", asset.id.clone(), asset.name.clone(), "png")
        };
        let filename = format!(
            "{}-{}.{}",
            name.replace(|c: char| !c.is_alphanumeric(), "-"),
            &id[..8],
            extension
        );
        let picker = cx.prompt_for_new_path(&forge_core::default_export_dir(), Some(&filename));
        cx.spawn_in(window, async move |entity, cx| {
            if let Ok(Ok(Some(path))) = picker.await {
                let _ = entity.update_in(cx, |studio, _, _| {
                    studio.send(method, json!({"id":id,"path":path}))
                });
            }
        })
        .detach();
    }
    fn asset_name(&self, asset: &Asset) -> String {
        if let Some(subject) = asset
            .character_id
            .as_ref()
            .and_then(|id| self.subjects.iter().find(|s| &s.id == id))
        {
            format!("{} · {}", subject.name, subject.kind.label())
        } else {
            asset.name.clone()
        }
    }
    fn has_previous_page(&self) -> bool {
        (self.filter != Filter::Animations && self.asset_page > 1)
            || (self.filter != Filter::Images && self.animation_page > 1)
    }
    fn has_next_page(&self) -> bool {
        (self.filter != Filter::Animations && self.asset_page * PAGE_SIZE < self.asset_total)
            || (self.filter != Filter::Images
                && self.animation_page * PAGE_SIZE < self.animation_total)
    }
    fn turn_page(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.filter != Filter::Animations {
            if forward && self.asset_page * PAGE_SIZE < self.asset_total {
                self.asset_page += 1;
            } else if !forward {
                self.asset_page = self.asset_page.saturating_sub(1).max(1);
            }
        }
        if self.filter != Filter::Images {
            if forward && self.animation_page * PAGE_SIZE < self.animation_total {
                self.animation_page += 1;
            } else if !forward {
                self.animation_page = self.animation_page.saturating_sub(1).max(1);
            }
        }
        self.refresh();
        cx.notify();
    }
    fn library(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let clip = self.clip();
        let asset = self.selected_asset();
        let preview = if let Some(clip) = clip
            && !self.show_atlas
        {
            clip.frames
                .get(self.frame_index.min(clip.frames.len().saturating_sub(1)))
                .map(|f| f.path.clone())
        } else {
            asset.map(|a| a.path.clone())
        };
        let title = clip
            .map(|a| a.config.name.clone())
            .or_else(|| asset.map(|a| self.asset_name(a)))
            .unwrap_or_else(|| "Your asset library".into());
        let detail = clip
            .map(|a| {
                format!(
                    "{} FRAMES · {} FPS · {}",
                    a.frames.len(),
                    a.config.fps,
                    if a.config.is_looping {
                        "LOOP"
                    } else {
                        "PLAY ONCE"
                    }
                )
            })
            .or_else(|| asset.map(|a| format!("{} × {} · PNG", a.width, a.height)))
            .unwrap_or_else(|| "CREATED WITH FORGE".into());
        let mut tools = div().flex().items_center().justify_between().gap_3().child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .font_family("Lora")
                        .text_size(px(24.))
                        .truncate()
                        .child(title),
                )
                .child(label(detail)),
        );
        if let Some(asset) = asset {
            let id = asset.id.clone();
            let added = self.references.contains(&id);
            tools = tools.child(
                Button::new("preview-chat")
                    .label(if added { "✓ In chat" } else { "Add to chat" })
                    .small()
                    .disabled(added || self.guide_busy())
                    .on_click(cx.listener(move |this, _, _, cx| this.add_to_chat(id.clone(), cx))),
            );
        }
        if clip.is_some() {
            tools = tools.child(
                Button::new("download-clip")
                    .label("Download clip")
                    .small()
                    .primary()
                    .disabled(clip.is_none_or(|c| c.status != JobStatus::Succeeded))
                    .on_click(cx.listener(|this, _, window, cx| this.download(true, window, cx))),
            );
        } else if asset.is_some() {
            tools = tools.child(
                Button::new("download-png")
                    .label("Download PNG")
                    .small()
                    .primary()
                    .on_click(cx.listener(|this, _, window, cx| this.download(false, window, cx))),
            );
        }
        let mut canvas = div()
            .flex_1()
            .min_h(px(180.))
            .rounded_lg()
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(0xe9eae1))
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden();
        if let Some(path) = preview {
            canvas = canvas.child(
                img(PathBuf::from(path))
                    .size_full()
                    .object_fit(ObjectFit::Contain),
            );
        } else {
            canvas=canvas.child(div().p_6().flex().flex_col().gap_3().items_center()
                .child(div().text_color(rgb(GREEN)).text_size(px(35.)).child("✦"))
                .child(div().font_family("Lora").text_size(px(30.)).child(if self.rendering(){"Bringing your world to life…"}else{"A world starts with an idea."}))
                .child(div().max_w(px(390.)).text_center().text_color(rgb(MUTED)).child(
                    if self.project.is_some(){"Ask Forge to create a character, structure, scene or animation. Everything you make appears here."}
                    else{"Tell Forge about your game. It will save your art direction, create reusable subjects and make your first assets."})));
        }
        let mut playback = div().flex().items_center().gap_2();
        if let Some(clip) = clip {
            let ready = !clip.frames.is_empty();
            playback = playback
                .child(
                    Button::new("play")
                        .label(if self.playing { "Pause" } else { "Play" })
                        .small()
                        .disabled(!ready)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.playing = !this.playing;
                            if this.playing {
                                this.play_started = Instant::now();
                                this.frame_index = 0;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("step")
                        .label("Next frame")
                        .small()
                        .ghost()
                        .disabled(!ready)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.playing = false;
                            if let Some(clip) = this.clip()
                                && !clip.frames.is_empty()
                            {
                                this.frame_index = (this.frame_index + 1) % clip.frames.len();
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("atlas")
                        .label(if self.show_atlas {
                            "Play frames"
                        } else {
                            "View atlas"
                        })
                        .small()
                        .ghost()
                        .disabled(!ready)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_atlas = !this.show_atlas;
                            cx.notify();
                        })),
                )
                .child(label(format!(
                    "FRAME {} / {}",
                    self.frame_index + 1,
                    clip.frames.len()
                )))
                .when(asset.is_some(), |d| {
                    d.child(
                        Button::new("download-atlas")
                            .label("Download atlas")
                            .xsmall()
                            .ghost()
                            .on_click(
                                cx.listener(|this, _, window, cx| this.download(false, window, cx)),
                            ),
                    )
                });
        }
        let mut filters = div().flex().gap_1();
        for (filter, name) in [
            (Filter::All, "All"),
            (Filter::Images, "Images"),
            (Filter::Animations, "Animations"),
        ] {
            filters = filters.child(
                Button::new(name)
                    .label(name)
                    .small()
                    .ghost()
                    .when(self.filter == filter, |b| b.bg(rgb(0xe7ece2)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.filter = filter;
                        cx.notify();
                    })),
            );
        }
        let library_header = div()
            .flex()
            .justify_between()
            .items_center()
            .child(filters)
            .child(
                div()
                    .flex()
                    .gap_1()
                    .items_center()
                    .child(label(format!(
                        "{} IMAGES · {} CLIPS",
                        self.asset_total, self.animation_total
                    )))
                    .child(
                        Button::new("prev")
                            .label("←")
                            .xsmall()
                            .ghost()
                            .disabled(!self.has_previous_page())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.turn_page(false, cx);
                            })),
                    )
                    .child(
                        Button::new("next")
                            .label("→")
                            .xsmall()
                            .ghost()
                            .disabled(!self.has_next_page())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.turn_page(true, cx);
                            })),
                    ),
            );
        let mut gallery = div()
            .id("asset-gallery")
            .h(px(141.))
            .flex_shrink_0()
            .flex()
            .gap_3()
            .overflow_x_scroll();
        if self.filter != Filter::Animations {
            for asset in &self.assets {
                let id = asset.id.clone();
                let active = self.animation_id.is_none() && self.selected.as_ref() == Some(&id);
                let added = self.references.contains(&id);
                gallery = gallery.child(
                    div()
                        .w(px(122.))
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .id(SharedString::from(format!("image-{id}")))
                                .h(px(83.))
                                .rounded_md()
                                .border_2()
                                .border_color(rgb(if active { GREEN } else { LINE }))
                                .bg(rgb(0xe9eae1))
                                .overflow_hidden()
                                .cursor_pointer()
                                .child(
                                    img(PathBuf::from(&asset.path))
                                        .size_full()
                                        .object_fit(ObjectFit::Contain),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.choose_asset(id.clone(), cx)
                                })),
                        )
                        .child(div().text_xs().truncate().child(self.asset_name(asset)))
                        .child({
                            let id = asset.id.clone();
                            Button::new(SharedString::from(format!("chat-{id}")))
                                .label(if added {
                                    "✓ In chat"
                                } else {
                                    "+ Add to chat"
                                })
                                .xsmall()
                                .ghost()
                                .disabled(added || self.guide_busy())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.add_to_chat(id.clone(), cx)
                                }))
                        }),
                );
            }
        }
        if self.filter != Filter::Images {
            for clip in &self.animations {
                let id = clip.id.clone();
                let active = self.animation_id.as_ref() == Some(&id);
                let thumbnail = clip.frames.first().map(|f| f.path.clone()).or_else(|| {
                    clip.source_asset_id
                        .as_ref()
                        .and_then(|id| self.cache.get(id))
                        .map(|a| a.path.clone())
                });
                let mut card = div()
                    .id(SharedString::from(format!("clip-{id}")))
                    .h(px(83.))
                    .rounded_md()
                    .border_2()
                    .border_color(rgb(if active { GREEN } else { LINE }))
                    .bg(rgb(0xe9eae1))
                    .overflow_hidden()
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_center();
                if let Some(path) = thumbnail {
                    card = card.child(
                        img(PathBuf::from(path))
                            .size_full()
                            .object_fit(ObjectFit::Contain),
                    );
                } else {
                    card = card.child(label(if clip.status.is_terminal() {
                        "CLIP"
                    } else {
                        "CREATING…"
                    }));
                }
                gallery = gallery.child(
                    div()
                        .w(px(122.))
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(card.on_click(
                            cx.listener(move |this, _, _, cx| this.choose_clip(id.clone(), cx)),
                        ))
                        .child(
                            div()
                                .text_xs()
                                .truncate()
                                .child(format!("▶ {}", clip.config.name)),
                        )
                        .when_some(clip.source_asset_id.clone(), |d, source| {
                            let added = self.references.contains(&source);
                            d.child(
                                Button::new(SharedString::from(format!("clip-chat-{}", clip.id)))
                                    .label(if added {
                                        "✓ In chat"
                                    } else {
                                        "+ Add to chat"
                                    })
                                    .xsmall()
                                    .ghost()
                                    .disabled(added || self.guide_busy())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.add_to_chat(source.clone(), cx)
                                    })),
                            )
                        }),
                );
            }
        }
        div()
            .id("library-workspace")
            .flex_1()
            .min_h_0()
            .p_5()
            .flex()
            .flex_col()
            .gap_3()
            .child(tools)
            .child(canvas)
            .when(clip.is_some(), |d| d.child(playback))
            .child(library_header)
            .child(gallery)
    }
    fn world(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut content = div()
            .id("world-catalog")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_5()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .font_family("Lora")
                    .text_size(px(30.))
                    .child("Your world's identity."),
            );
        if let Some(project) = &self.project {
            let mut palette = div().flex().gap_2();
            for hex in &project.style.palette {
                if let Ok(color) = u32::from_str_radix(hex.trim_start_matches('#'), 16) {
                    palette = palette.child(div().size(px(20.)).rounded_full().bg(rgb(color)));
                }
            }
            content = content.child(
                div()
                    .p_5()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(0xffffff))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(label("ART DIRECTION"))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_lg()
                            .child(project.style.name.clone()),
                    )
                    .child(palette)
                    .child(
                        div()
                            .text_sm()
                            .line_height(px(22.))
                            .child(project.style.description.clone()),
                    )
                    .when(!project.style.perspective.is_empty(), |d| {
                        d.child(label(project.style.perspective.clone()))
                    })
                    .when(!project.style.lighting.is_empty(), |d| {
                        d.child(label(project.style.lighting.clone()))
                    })
                    .child(
                        Button::new("discuss-style")
                            .label("Discuss in chat")
                            .small()
                            .ghost()
                            .disabled(self.guide_busy())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.guide_input.update(cx, |input, cx| {
                                    input.set_value(
                                        "I'd like to change our art direction: ",
                                        window,
                                        cx,
                                    )
                                });
                                cx.notify();
                            })),
                    ),
            );
            for (kind, title) in [
                (SubjectKind::Character, "CHARACTERS"),
                (SubjectKind::Structure, "STRUCTURES"),
                (SubjectKind::Prop, "PROPS"),
            ] {
                let subjects: Vec<_> = self.subjects.iter().filter(|s| s.kind == kind).collect();
                if subjects.is_empty() {
                    continue;
                }
                content = content.child(label(format!("{title} / {}", subjects.len())));
                for subject in subjects {
                    let saved = subject.clone();
                    content =
                        content.child(
                            div()
                                .p_4()
                                .rounded_lg()
                                .border_1()
                                .border_color(rgb(LINE))
                                .bg(rgb(0xffffff))
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(subject.name.clone()),
                                        )
                                        .child(
                                            Button::new(SharedString::from(format!(
                                                "subject-chat-{}",
                                                subject.id
                                            )))
                                            .label("Add to chat")
                                            .xsmall()
                                            .ghost()
                                            .disabled(self.guide_busy())
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.discuss_subject(saved.clone(), window, cx)
                                            })),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .line_height(px(22.))
                                        .text_color(rgb(MUTED))
                                        .child(subject.description.clone()),
                                ),
                        );
                }
            }
            if self.subjects.is_empty() {
                content=content.child(div().text_color(rgb(MUTED)).child("Ask Forge to create your cast, structures and props. Their reusable identities will appear here."));
            }
            content = content.child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(label(format!("{} SAVED SUBJECTS", self.subject_total)))
                    .child(
                        Button::new("subjects-prev")
                            .label("←")
                            .xsmall()
                            .ghost()
                            .disabled(self.subject_page == 1)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.subject_page = this.subject_page.saturating_sub(1).max(1);
                                this.refresh();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("subjects-next")
                            .label("→")
                            .xsmall()
                            .ghost()
                            .disabled(self.subject_page * PAGE_SIZE >= self.subject_total)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.subject_page += 1;
                                this.refresh();
                                cx.notify();
                            })),
                    ),
            );
        } else {
            content=content.child(div().text_color(rgb(MUTED)).child("Tell Forge about your game. Your saved art direction and subjects will appear here."));
        }
        content
    }
    fn guide_panel(&self, cx: &mut Context<Self>) -> Div {
        let connected = self.account.as_ref().is_some_and(|a| a.is_logged_in);
        let mut conversation = div()
            .id("conversation")
            .track_scroll(&self.guide_scroll)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_5()
            .flex()
            .flex_col()
            .gap_4();
        if self.guide.as_ref().is_none_or(|g| g.messages.is_empty()) {
            conversation=conversation.child(div().font_family("Lora").text_size(px(28.)).child("What are we making?"))
                .child(div().text_sm().line_height(px(23.)).text_color(rgb(MUTED)).child("Describe your game or the next change. I'll save the style, develop your subjects and create the assets."))
                .child(label("TRY AN IDEA"));
            let ideas = if self.project.is_some() {
                [
                    (
                        "Create the next asset",
                        "Create a useful next asset for this game, using our saved art direction and subjects.",
                    ),
                    (
                        "Animate a character",
                        "Make a walk animation for one of our saved characters, preserving its identity.",
                    ),
                ]
            } else {
                [
                    (
                        "Isometric tower defense",
                        "Create a new isometric tower defense game. Save an art direction, three defense structures and two enemy characters, then make one concept sheet.",
                    ),
                    (
                        "Cozy forest adventure",
                        "Create a cozy forest adventure game with a warm storybook style. Save a scout and a woodland cottage, then create the scout's first sprite.",
                    ),
                ]
            };
            for (name, text) in ideas {
                conversation = conversation.child(
                    Button::new(name)
                        .label(name)
                        .small()
                        .w_full()
                        .disabled(!connected || self.guide_busy())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.guide_input
                                .update(cx, |input, cx| input.set_value(text, window, cx));
                            cx.notify();
                        })),
                );
            }
        }
        if let Some(guide) = &self.guide {
            for (index, message) in guide.messages.iter().enumerate() {
                let mut bubble = div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .when(message.role == "USER", |d| d.bg(rgb(0xe7ece2)))
                    .child(label(if message.role == "USER" {
                        "YOU"
                    } else {
                        "FORGE"
                    }));
                if !message.reference_asset_ids.is_empty() {
                    bubble = bubble.child(label(format!(
                        "{} IMAGE{} INCLUDED",
                        message.reference_asset_ids.len(),
                        if message.reference_asset_ids.len() == 1 {
                            ""
                        } else {
                            "S"
                        }
                    )));
                }
                conversation = conversation.child(
                    bubble
                        .id(SharedString::from(format!("message-{index}")))
                        .child(
                            div()
                                .text_sm()
                                .line_height(px(22.))
                                .child(message.text.clone()),
                        ),
                );
            }
            if let Some(error) = &guide.error {
                conversation = conversation.child(
                    div()
                        .p_3()
                        .rounded_md()
                        .bg(rgb(0xf7e9e1))
                        .text_sm()
                        .child(error.message.clone()),
                );
            }
        }
        if self.guide_busy() {
            conversation = conversation.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(label("FORGE IS WORKING"))
                    .child(div().text_sm().line_height(px(22.)).child(
                        if self.guide_stream.is_empty() {
                            "Shaping your world…".into()
                        } else {
                            self.guide_stream.clone()
                        },
                    )),
            );
        }
        for action in &self.guide_actions {
            conversation = conversation.child(
                div()
                    .text_xs()
                    .text_color(rgb(GREEN))
                    .child(format!("✓ {action}")),
            );
        }
        let mut attachments = div().flex().gap_1().flex_wrap();
        for id in &self.references {
            let asset = self.cache.get(id);
            let id = id.clone();
            let mut chip = div().rounded_md().bg(rgb(0xe7ece2)).flex().items_center();
            if let Some(asset) = asset {
                chip = chip.child(
                    img(PathBuf::from(&asset.path))
                        .size(px(28.))
                        .object_fit(ObjectFit::Contain),
                );
            }
            attachments = attachments.child(
                chip.child(
                    Button::new(SharedString::from(format!("remove-{id}")))
                        .label(format!(
                            "{} ×",
                            asset.map(|a| a.name.as_str()).unwrap_or("Image")
                        ))
                        .xsmall()
                        .ghost()
                        .disabled(self.guide_busy())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.references.retain(|r| r != &id);
                            cx.notify();
                        })),
                ),
            );
        }
        div().w(px(360.)).flex_shrink_0().h_full().border_l_1().border_color(rgb(LINE)).bg(rgb(0xf1f2e9)).flex().flex_col()
            .child(div().h(px(58.)).flex_shrink_0().px_5().flex().items_center().justify_between().border_b_1().border_color(rgb(LINE))
                .child(div().flex().gap_2().items_center().child(div().size(px(27.)).rounded_full().bg(rgb(GREEN)).text_color(rgb(0xffffff)).flex().items_center().justify_center().child("✦"))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Forge")))
                .child(label("YOUR ART DIRECTOR")))
            .child(conversation)
            .child(div().p_4().border_t_1().border_color(rgb(LINE)).flex().flex_col().gap_2()
                .when(!self.references.is_empty(),|d|d.child(attachments))
                .child(Input::new(&self.guide_input).h(px(90.)).disabled(self.guide_busy()))
                .child(div().flex().gap_2().items_center()
                    .child(Button::new("attach-image").label("+ Image").small().ghost().disabled(self.project.is_none()||self.references.len()>=8||self.guide_busy())
                        .on_click(cx.listener(|this,_,window,cx|this.attach_image(window,cx))))
                    .child(Button::new("send").label(if self.guide_busy(){"Working…"}else{"Send to Forge →"}).primary().flex_1().disabled(!connected||self.guide_busy())
                        .on_click(cx.listener(|this,_,_,cx|this.ask_guide(cx)))))
                .when(self.guide_busy()||self.rendering(),|d|d.child(Button::new("stop").label("Stop").small().ghost().w_full()
                    .on_click(cx.listener(|this,_,_,_|{
                        if let Some(guide)=&this.guide && guide.status==AssistantStatus::Thinking {this.send("assistant/cancel",json!({"id":guide.id}));}
                        if let Some(job)=&this.job && !job.status.is_terminal() {this.send("jobs/cancel",json!({"id":job.id}));}
                    }))))
                .child(div().text_size(px(10.)).text_color(rgb(MUTED)).child(if connected{"Add an asset to chat to revise it. Forge keeps your saved style and references."}else{"Connect Codex above to begin."})))
    }
}

impl Render for Studio {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let account_label = self
            .account
            .as_ref()
            .map(|a| {
                if a.is_logged_in {
                    format!("Codex · {}", a.plan.as_deref().unwrap_or("Connected"))
                } else {
                    "Connect Codex".into()
                }
            })
            .unwrap_or_else(|| "Checking Codex…".into());
        let header = div()
            .h(px(67.))
            .flex_shrink_0()
            .px_6()
            .border_b_1()
            .border_color(rgb(LINE))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .font_family("Lora")
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(25.))
                            .child("Asset Forge"),
                    )
                    .child(label("2D WORKSHOP")),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        Button::new("library-view")
                            .label("Library")
                            .ghost()
                            .when(self.view == View::Library, |b| b.bg(rgb(0xe7ece2)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.view = View::Library;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("world-view")
                            .label("World")
                            .ghost()
                            .when(self.view == View::World, |b| b.bg(rgb(0xe7ece2)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.view = View::World;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        Button::new("update")
                            .label("Update")
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                match open::that(crate::RELEASE_PAGE) {
                                    Ok(()) => this.message(
                                        concat!(
                                            "Opened the latest release. You have Asset Forge ",
                                            env!("CARGO_PKG_VERSION"),
                                            "."
                                        ),
                                        false,
                                        cx,
                                    ),
                                    Err(error) => this.message(
                                        format!("Could not open the release page: {error}"),
                                        true,
                                        cx,
                                    ),
                                }
                            })),
                    )
                    .child(
                        Button::new("account")
                            .label(account_label)
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.account.as_ref().is_some_and(|a| a.is_logged_in) {
                                    this.send("account/read", json!({}));
                                } else if let Some(login) = &this.login {
                                    if let Err(error) = open::that(&login.auth_url) {
                                        this.message(
                                            format!("Could not open sign-in: {error}"),
                                            true,
                                            cx,
                                        );
                                    }
                                } else {
                                    this.send("account/login/start", json!({}));
                                }
                            })),
                    )
                    .child(
                        Button::new("refresh")
                            .label("↻")
                            .tooltip("Refresh the library and Codex account")
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, _| {
                                this.refresh();
                                this.send("projects/list", json!({"pageSize":100}));
                                this.send("account/read", json!({}));
                            })),
                    ),
            );
        let mut projects = div()
            .id("project-picker")
            .flex_1()
            .min_w_0()
            .flex()
            .gap_1()
            .items_center()
            .overflow_x_scroll();
        for project in &self.projects {
            let project = project.clone();
            projects = projects.child(
                Button::new(SharedString::from(project.id.clone()))
                    .label(project.name.clone())
                    .small()
                    .ghost()
                    .when(self.project_id().as_deref() == Some(&project.id), |b| {
                        b.bg(rgb(0xe7ece2))
                    })
                    .disabled(self.guide_busy())
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.choose_project(project.clone(), cx)),
                    ),
            );
        }
        projects = projects.child(
            Button::new("new-game")
                .label("+ New game")
                .small()
                .ghost()
                .disabled(self.guide_busy())
                .on_click(cx.listener(|this, _, window, cx| this.new_game(window, cx))),
        );
        let projectbar = div()
            .h(px(49.))
            .px_5()
            .flex_shrink_0()
            .border_b_1()
            .border_color(rgb(LINE))
            .flex()
            .gap_3()
            .items_center()
            .child(projects)
            .when_some(self.project.as_ref(), |d, p| {
                d.child(label(format!(
                    "{} · {} SUBJECTS",
                    p.style.name, self.subject_total
                )))
            });
        let workspace = match self.view {
            View::Library => self.library(cx).into_any_element(),
            View::World => self.world(cx).into_any_element(),
        };
        let login = div()
            .px_6()
            .py_2()
            .bg(rgb(0xe4ebdf))
            .flex()
            .justify_between()
            .items_center()
            .child("Finish connecting Codex in your browser.")
            .child(
                Button::new("cancel-login")
                    .label("Cancel sign-in")
                    .small()
                    .on_click(cx.listener(|this, _, _, _| {
                        if let Some(login) = &this.login {
                            this.send("account/login/cancel", json!({"id":login.login_id}));
                        }
                    })),
            );
        let footer = div()
            .h(px(34.))
            .px_6()
            .flex_shrink_0()
            .border_t_1()
            .border_color(rgb(LINE))
            .flex()
            .items_center()
            .gap_2()
            .text_xs()
            .text_color(rgb(if self.is_error { 0xa1432f } else { MUTED }))
            .child(div().size(px(5.)).rounded_full().bg(rgb(if self.is_error {
                0xa1432f
            } else {
                GREEN
            })))
            .child(div().truncate().child(self.status.clone()));
        div()
            .size_full()
            .font_family("IBM Plex Sans")
            .text_color(rgb(INK))
            .bg(rgb(PAPER))
            .flex()
            .flex_col()
            .child(header)
            .when(self.login.is_some(), |d| d.child(login))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .child(projectbar)
                            .child(workspace),
                    )
                    .child(self.guide_panel(cx)),
            )
            .child(footer)
    }
}
