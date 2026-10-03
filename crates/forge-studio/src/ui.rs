use crate::{Command, Response};
use forge_core::contract::{Animation, Asset};
use forge_core::updates::{self, CheckResult, PreparedUpdate, Progress};
use forge_core::{contract::*, error::Result};
use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    Disableable, Root, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    dialog::DialogButtonProps,
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
    subject_cache: HashMap<String, Character>,
    library_subjects: Vec<LibrarySubject>,
    subject_id: Option<String>,
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
    collection_jobs: HashSet<String>,
    collection_focus_job: Option<String>,
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
    active_jobs: Vec<Job>,
    status: String,
    is_error: bool,
    guide: Option<AssistantSession>,
    guide_input: Entity<InputState>,
    name_input: Entity<InputState>,
    renaming: bool,
    management_busy: bool,
    last_deletion: Option<Deletion>,
    guide_scroll: ScrollHandle,
    guide_scroll_pending: bool,
    guide_stream: String,
    guide_actions: Vec<String>,
    guide_submitting: bool,
    show_atlas: bool,
    playing: bool,
    play_started: Instant,
    frame_index: usize,
    update_label: String,
    update_busy: bool,
    update_inbox: Option<Receiver<Progress>>,
    ready_update: Option<PreparedUpdate>,
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
            subject_cache: HashMap::new(),
            library_subjects: vec![],
            subject_id: None,
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
            collection_jobs: HashSet::new(),
            collection_focus_job: None,
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
            active_jobs: vec![],
            status: "Tell Forge about your game to begin.".into(),
            is_error: false,
            guide: None,
            guide_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Ask Forge to create or change anything…")
                    .multi_line(true)
                    .rows(3)
            }),
            name_input: cx.new(|cx| InputState::new(window, cx).placeholder("Game name")),
            renaming: false,
            management_busy: false,
            last_deletion: None,
            guide_scroll: ScrollHandle::new(),
            guide_scroll_pending: false,
            guide_stream: String::new(),
            guide_actions: vec![],
            guide_submitting: false,
            show_atlas: false,
            playing: true,
            play_started: Instant::now(),
            frame_index: 0,
            update_label: "Update".into(),
            update_busy: false,
            update_inbox: None,
            ready_update: None,
        };
        let mut studio = studio;
        if let Some((message, error)) = updates::take_install_result() {
            studio.status = message;
            studio.is_error = error;
        }
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
        !self.active_jobs.is_empty() || self.job.as_ref().is_some_and(|j| !j.status.is_terminal())
    }
    fn update_application(&mut self, cx: &mut Context<Self>) {
        if self.update_busy {
            return;
        }
        if self.rendering() || self.guide_busy() || self.management_busy || self.renaming {
            self.message(
                "Finish your current work before updating Asset Forge.",
                false,
                cx,
            );
            return;
        }
        if self.ready_update.is_some() {
            if !self.guide_input.read(cx).value().trim().is_empty() {
                self.message(
                    "Send or clear your chat draft before restarting to install the update.",
                    false,
                    cx,
                );
                return;
            }
            self.management_busy = true;
            self.send("system/update/ready", json!({}));
            self.message("Preparing to restart and install the update…", false, cx);
            return;
        }
        let (sender, inbox) = std::sync::mpsc::channel();
        self.update_inbox = Some(inbox);
        self.update_busy = true;
        self.update_label = "Checking…".into();
        self.message("Checking for a newer Asset Forge release…", false, cx);
        std::thread::spawn(move || {
            let result = std::env::current_exe()
                .map_err(|_| {
                    forge_core::error::ApiError::new(
                        "UPDATE_FAILED",
                        "Could not locate the current application.",
                    )
                })
                .and_then(|exe| {
                    updates::prepare_latest(&exe, |progress| {
                        let _ = sender.send(progress);
                    })
                });
            let _ = sender.send(Progress::Finished(result));
        });
    }
    fn refresh(&self) {
        if let Some(id) = self.project_id() {
            self.send("assets/list", self.media_params(self.asset_page));
            self.send(
                "library/subjects/list",
                json!({"projectId":id,"page":self.subject_page,"pageSize":PAGE_SIZE}),
            );
            self.send("animations/list", self.media_params(self.animation_page));
            if let Some(clip) = &self.animation_id {
                self.send("animations/get", json!({"id":clip}));
            }
            self.send("jobs/list", json!({"projectId":id,"pageSize":100}));
        }
    }
    fn reset_library(&mut self) {
        self.subjects.clear();
        self.subject_cache.clear();
        self.library_subjects.clear();
        self.subject_id = None;
        self.assets.clear();
        self.cache.clear();
        self.selected = None;
        self.references.clear();
        self.animations.clear();
        self.animation_cache.clear();
        self.animation_id = None;
        self.animations_loaded = false;
        self.ready_jobs.clear();
        self.collection_jobs.clear();
        self.collection_focus_job = None;
        self.job = None;
        self.active_jobs.clear();
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
            self.renaming = false;
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
        self.renaming = false;
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
    fn confirm_delete(
        &mut self,
        kind: &str,
        id: String,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let method = match kind {
            "project" => "projects/delete",
            "animation" => "animations/delete",
            _ => "assets/delete",
        };
        let description = match kind {
            "project" => {
                "This removes the game and its contents from the lists. You can undo this deletion."
            }
            "animation" => {
                "This removes the clip from the library. Its source image stays available. You can undo this deletion."
            }
            _ => {
                "This removes the image and any clips made from it, and unpins it from saved references. You can undo removal; references can be pinned again in chat."
            }
        };
        let studio = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let studio = studio.clone();
            let on_close = studio.clone();
            let id = id.clone();
            dialog
                .confirm()
                .title(format!("Delete {name}?"))
                .child(div().text_sm().child(description))
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("Delete")
                        .cancel_text("Keep it"),
                )
                .on_ok(move |_, _, cx| {
                    let _ = studio.update(cx, |this, cx| {
                        this.management_busy = true;
                        this.send(method, json!({"id":id}));
                        cx.notify();
                    });
                    true
                })
                .on_close(move |_, _, cx| {
                    let _ = on_close.update(cx, |_, cx| cx.notify());
                })
        });
        cx.notify();
    }
    fn begin_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(project) = &self.project {
            let name = project.name.clone();
            self.name_input.update(cx, |input, cx| {
                input.set_value(name, window, cx);
                input.focus(window, cx);
            });
            self.renaming = true;
            cx.notify();
        }
    }
    fn save_name(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.project_id() {
            let name = self.name_input.read(cx).value().trim().to_string();
            if name.is_empty() {
                self.message("Enter a game name.", true, cx);
                return;
            }
            self.management_busy = true;
            self.send("projects/update", json!({"id":id,"name":name}));
            cx.notify();
        }
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
    fn media_params(&self, page: usize) -> Value {
        json!({"projectId":self.project_id(),"characterId":self.subject_id,"isUnassigned":self.subject_id.is_none(),"page":page,"pageSize":PAGE_SIZE})
    }
    fn subject(&self) -> Option<&Character> {
        self.subject_id
            .as_ref()
            .and_then(|id| self.subject_cache.get(id))
    }
    fn choose_subject(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        self.subject_id = id;
        self.selected = None;
        self.animation_id = None;
        self.assets.clear();
        self.animations.clear();
        self.asset_total = 0;
        self.animation_total = 0;
        self.asset_page = 1;
        self.animation_page = 1;
        self.filter = Filter::All;
        self.animations_loaded = false;
        self.view = View::Library;
        if let Some(id) = &self.subject_id
            && !self.subject_cache.contains_key(id)
        {
            self.send("characters/get", json!({"id":id}));
        }
        if let Some(subject) = self.subject() {
            for id in &subject.reference_asset_ids {
                if !self.cache.contains_key(id) {
                    self.send("assets/get", json!({"id":id}));
                }
            }
        }
        self.refresh();
        cx.notify();
    }
    fn browse_subject(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        self.animation_id = None;
        self.view = View::Library;
        cx.notify();
    }
    fn choose_asset(&mut self, id: String, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.animation_id = None;
        self.view = View::Library;
        cx.notify();
    }
    fn choose_clip(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(clip) = self.animation_cache.get(&id).cloned() {
            let owner = clip.character_id.clone();
            if self.subject_id != owner {
                self.subject_id = owner;
                self.asset_page = 1;
                self.animation_page = 1;
                self.refresh();
            }
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
        if self.guide_scroll_pending {
            self.guide_scroll_pending = false;
            self.guide_scroll.scroll_to_bottom();
            changed = true;
        }
        let updates: Vec<_> = self
            .update_inbox
            .as_ref()
            .map(|inbox| inbox.try_iter().collect())
            .unwrap_or_default();
        for update in updates {
            changed = true;
            match update {
                Progress::Downloading(percent) => {
                    self.update_label = format!("Downloading {percent}%")
                }
                Progress::Verifying => self.update_label = "Verifying…".into(),
                Progress::Finished(result) => {
                    self.update_busy = false;
                    self.update_inbox = None;
                    match result {
                        Ok(CheckResult::Current { installed, latest }) => {
                            self.update_label = "Update".into();
                            let message = if installed == latest {
                                format!(
                                    "You're up to date. Asset Forge {installed} is the latest release."
                                )
                            } else {
                                format!(
                                    "No newer release is available. You have {installed}; the latest published release is {latest}."
                                )
                            };
                            self.message(message, false, cx);
                        }
                        Ok(CheckResult::Ready(update)) => {
                            self.message(format!("Asset Forge {} is ready. Choose Restart & install to update and reopen the app.", update.version), false, cx);
                            self.ready_update = Some(update);
                            self.update_label = "Restart & install".into();
                        }
                        Err(error) => {
                            self.update_label = "Retry update".into();
                            self.message(error.message, true, cx);
                        }
                    }
                }
            }
        }
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
        if method == "system/update/ready" {
            self.management_busy = false;
            let result = result.and_then(|_| {
                self.ready_update
                    .as_ref()
                    .ok_or_else(|| {
                        forge_core::error::ApiError::new(
                            "UPDATE_FAILED",
                            "The prepared update is no longer available.",
                        )
                    })
                    .and_then(updates::launch_installer)
            });
            match result {
                Ok(()) => cx.quit(),
                Err(error) => {
                    self.send("system/update/cancel", json!({}));
                    self.message(error.message, true, cx);
                }
            }
            return;
        }
        if [
            "assets/list",
            "assets/import",
            "characters/list",
            "library/subjects/list",
            "animations/list",
            "jobs/list",
            "assistant/list",
        ]
        .contains(&method)
            && params["projectId"].as_str() != self.project_id().as_deref()
        {
            return;
        }
        if matches!(method, "assets/list" | "animations/list")
            && params["characterId"].as_str() != self.subject_id.as_deref()
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
                if error.code == "NOT_FOUND" && method == "assets/get" {
                    if params["id"].as_str() == self.selected.as_deref() {
                        self.selected = None;
                        self.refresh();
                    }
                    return;
                }
                if error.code == "NOT_FOUND" && method == "animations/get" {
                    if params["id"].as_str() == self.animation_id.as_deref() {
                        self.animation_id = None;
                        self.refresh();
                    }
                    return;
                }
                if method == "assistant/message" {
                    self.guide_submitting = false;
                }
                if method.ends_with("/delete")
                    || method.ends_with("/restore")
                    || method == "projects/update"
                {
                    self.management_busy = false;
                }
                self.message(error.message, true, cx);
                return;
            }
        };
        match method {
            "characters/get" => {
                if let Ok(subject) = serde_json::from_value::<Character>(data)
                    && Some(subject.project_id.as_str()) == self.project_id().as_deref()
                {
                    self.subject_cache.insert(subject.id.clone(), subject);
                }
            }
            "library/subjects/list" => {
                if let Ok(page) = serde_json::from_value::<Page<LibrarySubject>>(data) {
                    if params["page"].as_u64().unwrap_or(1) as usize != self.subject_page {
                        return;
                    }
                    self.subject_total = page.pagination.total_items;
                    self.subjects = page.data.iter().map(|s| s.subject.clone()).collect();
                    for entry in &page.data {
                        self.subject_cache
                            .insert(entry.subject.id.clone(), entry.subject.clone());
                        if let Some(asset) = &entry.preview {
                            self.cache.insert(asset.id.clone(), asset.clone());
                        }
                    }
                    self.library_subjects = page.data;
                }
            }
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
            "projects/update" | "projects/restore" | "projects/get" => {
                if let Ok(project) = serde_json::from_value::<Project>(data) {
                    if method != "projects/get" {
                        self.management_busy = false;
                        self.renaming = false;
                        if method == "projects/restore" {
                            self.last_deletion = None;
                        }
                        self.message(
                            if method == "projects/update" {
                                "Game renamed."
                            } else {
                                "Game restored."
                            },
                            false,
                            cx,
                        );
                    }
                    if let Some(saved) = self.projects.iter_mut().find(|p| p.id == project.id) {
                        *saved = project.clone();
                    } else {
                        self.projects.push(project.clone());
                    }
                    self.choose_project(project, cx);
                }
            }
            "projects/delete" | "assets/delete" | "animations/delete" => {
                self.management_busy = false;
                if let Ok(deletion) = serde_json::from_value::<Deletion>(data) {
                    if deletion.kind == "project" {
                        self.projects.retain(|p| p.id != deletion.id);
                        if self.project_id().as_deref() == Some(&deletion.id) {
                            self.new_game(window, cx);
                            if let Some(project) = self.projects.first().cloned() {
                                self.choose_project(project, cx);
                            }
                        }
                    } else if self.project_id().as_deref() == Some(&deletion.project_id) {
                        if deletion.kind == "asset" {
                            self.assets.retain(|a| a.id != deletion.id);
                            self.cache.remove(&deletion.id);
                            self.references.retain(|id| id != &deletion.id);
                            if self.selected.as_ref() == Some(&deletion.id) {
                                self.selected = None;
                            }
                            let clips: Vec<_> = self
                                .animation_cache
                                .values()
                                .filter(|c| c.source_asset_id.as_ref() == Some(&deletion.id))
                                .map(|c| c.id.clone())
                                .collect();
                            for id in clips {
                                self.animation_cache.remove(&id);
                                self.animations.retain(|c| c.id != id);
                                if self.animation_id.as_ref() == Some(&id) {
                                    self.animation_id = None;
                                }
                            }
                        } else {
                            self.animation_cache.remove(&deletion.id);
                            self.animations.retain(|c| c.id != deletion.id);
                            if self.animation_id.as_ref() == Some(&deletion.id) {
                                self.animation_id = None;
                            }
                        }
                        self.asset_page = 1;
                        self.animation_page = 1;
                        self.refresh();
                        self.send("projects/get", json!({"id":deletion.project_id}));
                    }
                    self.message(
                        format!("Deleted {}. Undo is available below.", deletion.name),
                        false,
                        cx,
                    );
                    self.last_deletion = Some(deletion);
                }
                self.send("projects/list", json!({"pageSize":100}));
            }
            "assets/restore" | "animations/restore" => {
                self.management_busy = false;
                self.last_deletion = None;
                self.refresh();
                self.message("Asset restored.", false, cx);
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
                                && !self.collection_jobs.contains(&clip.id)
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
                    let collection = self.collection_jobs.contains(&job.id)
                        || job.request.idempotency_key.starts_with("animation-set:");
                    if self.collection_focus_job.as_ref() == Some(&job.id) {
                        self.collection_focus_job = None;
                        self.choose_subject(job.request.character_id.clone(), cx);
                        self.follow_new_clips = false;
                    }
                    self.active_jobs.retain(|j| j.id != job.id);
                    if !job.status.is_terminal() {
                        self.active_jobs.push(job.clone());
                    }
                    if let Some(id) = job.asset_ids.first()
                        && self.ready_jobs.insert(job.id.clone())
                    {
                        if !collection {
                            self.subject_id = job.request.character_id.clone();
                        }
                        if let Some(subject) = &job.character_snapshot {
                            self.subject_cache
                                .insert(subject.id.clone(), subject.clone());
                        }
                        self.send("assets/get", json!({"id":id}));
                        if !collection && job.request.animation.is_some() {
                            self.animation_id = Some(job.id.clone());
                            self.selected = None;
                            self.show_atlas = false;
                            self.playing = true;
                            self.play_started = Instant::now();
                            self.frame_index = 0;
                            self.send("animations/get", json!({"id":job.id}));
                        } else if !collection {
                            self.selected = Some(id.clone());
                            self.animation_id = None;
                        }
                        if !collection {
                            self.asset_page = 1;
                            self.animation_page = 1;
                            self.view = View::Library;
                            self.filter = Filter::All;
                        }
                        self.refresh();
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
                    self.active_jobs = page
                        .data
                        .iter()
                        .filter(|j| !j.status.is_terminal())
                        .cloned()
                        .collect();
                    self.job = self
                        .active_jobs
                        .first()
                        .cloned()
                        .or_else(|| page.data.into_iter().next());
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
                    if session.turn_job_count > 1 {
                        let start = session
                            .generated_job_ids
                            .len()
                            .saturating_sub(session.turn_job_count as usize);
                        self.collection_jobs
                            .extend(session.generated_job_ids[start..].iter().cloned());
                        self.follow_new_clips = false;
                    }
                    let job = session
                        .generated_job_ids
                        .last()
                        .filter(|id| {
                            self.guide.as_ref().and_then(|g| g.generated_job_ids.last()) != Some(id)
                        })
                        .cloned();
                    if session.turn_job_count > 1 && job.is_some() {
                        self.collection_focus_job = job.clone();
                    }
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
                        && guide.turn_job_count > 1
                    {
                        let start = guide
                            .generated_job_ids
                            .len()
                            .saturating_sub(guide.turn_job_count as usize);
                        self.collection_jobs
                            .extend(guide.generated_job_ids[start..].iter().cloned());
                        self.follow_new_clips = false;
                    }
                    self.guide_scroll_pending = true;
                    if let Some(guide) = &self.guide
                        && guide.status == AssistantStatus::Thinking
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
        let answer = self
            .guide
            .as_ref()
            .and_then(|g| g.pending_question.as_ref())
            .map(|q| QuestionAnswer {
                question_id: q.id.clone(),
                option_id: None,
                skipped: false,
            });
        self.send_guide_message(text, answer, cx);
    }
    fn answer_choice(&mut self, option_id: Option<String>, skipped: bool, cx: &mut Context<Self>) {
        if self.guide_busy() {
            return;
        }
        let Some(question) = self
            .guide
            .as_ref()
            .and_then(|g| g.pending_question.as_ref())
        else {
            return;
        };
        let text = option_id
            .as_ref()
            .and_then(|id| question.options.iter().find(|o| &o.id == id))
            .map(|o| o.label.clone())
            .unwrap_or_else(|| "Skip this question and continue.".into());
        let answer = QuestionAnswer {
            question_id: question.id.clone(),
            option_id,
            skipped,
        };
        self.send_guide_message(text, Some(answer), cx);
    }
    fn send_guide_message(
        &mut self,
        text: String,
        answer: Option<QuestionAnswer>,
        cx: &mut Context<Self>,
    ) {
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
                "questionAnswer":answer,
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
        if asset.kind == AssetKind::ConceptSheet {
            return format!("{} · Reference", asset.name);
        }
        if let Some(subject) = asset
            .character_id
            .as_ref()
            .and_then(|id| self.subjects.iter().find(|s| &s.id == id))
        {
            format!("{} · {}", asset.name, subject.kind.label())
        } else {
            asset.name.clone()
        }
    }
    fn clip_title(&self, clip: &Animation) -> String {
        if let Some(subject) = clip
            .character_id
            .as_ref()
            .and_then(|id| self.subject_cache.get(id))
        {
            for separator in [" — ", " – ", " - "] {
                if let Some(title) = clip
                    .config
                    .name
                    .strip_prefix(&format!("{}{separator}", subject.name))
                {
                    return title.to_owned();
                }
            }
        }
        clip.config.name.clone()
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
    fn sidebar(&self, cx: &mut Context<Self>) -> Div {
        let busy = self.guide_busy() || self.management_busy;
        let mut games = div()
            .id("game-list")
            .max_h(px(190.))
            .min_h(px(40.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();
        for project in &self.projects {
            let active = self.project_id().as_deref() == Some(&project.id);
            let selected = project.clone();
            let deleted = project.clone();
            games = games.child(
                div()
                    .flex()
                    .items_center()
                    .rounded_md()
                    .when(active, |d| d.bg(rgb(0xe3e9de)))
                    .child(
                        div()
                            .id(SharedString::from(format!("game-{}", project.id)))
                            .flex_1()
                            .min_w_0()
                            .px_2()
                            .py_2()
                            .cursor_pointer()
                            .child(div().text_sm().truncate().child(project.name.clone()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.guide_busy() && !this.management_busy {
                                    this.choose_project(selected.clone(), cx);
                                }
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("delete-game-{}", project.id)))
                            .label("×")
                            .tooltip("Delete game")
                            .xsmall()
                            .ghost()
                            .disabled(busy || (active && self.rendering()))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.confirm_delete(
                                    "project",
                                    deleted.id.clone(),
                                    deleted.name.clone(),
                                    window,
                                    cx,
                                )
                            })),
                    ),
            );
        }
        let mut catalog = div()
            .id("subject-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();
        for (kind, title) in [
            (SubjectKind::Character, "CHARACTERS"),
            (SubjectKind::Structure, "STRUCTURES"),
            (SubjectKind::Prop, "PROPS"),
            (SubjectKind::Scene, "SCENES"),
        ] {
            let entries: Vec<_> = self
                .library_subjects
                .iter()
                .filter(|s| s.subject.kind == kind)
                .collect();
            if entries.is_empty() {
                continue;
            }
            catalog = catalog.child(label(title).mt_3().mb_1());
            for entry in entries {
                let id = entry.subject.id.clone();
                let mut thumbnail = div()
                    .size(px(36.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(GREEN))
                    .child("✦");
                if let Some(asset) = &entry.preview {
                    thumbnail = div().size(px(36.)).flex_shrink_0().child(
                        img(PathBuf::from(&asset.path))
                            .size_full()
                            .object_fit(ObjectFit::Contain),
                    );
                }
                let rendering = self
                    .active_jobs
                    .iter()
                    .any(|j| j.request.character_id.as_ref() == Some(&entry.subject.id));
                catalog = catalog.child(
                    div()
                        .id(SharedString::from(format!("subject-{id}")))
                        .p_2()
                        .rounded_md()
                        .flex()
                        .gap_2()
                        .items_center()
                        .cursor_pointer()
                        .when(self.subject_id.as_ref() == Some(&id), |d| {
                            d.bg(rgb(0xe3e9de))
                        })
                        .child(thumbnail)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(div().text_sm().truncate().child(entry.subject.name.clone()))
                                .child(div().text_size(px(10.)).text_color(rgb(MUTED)).child(
                                    if rendering {
                                        "Rendering…".into()
                                    } else {
                                        format!(
                                            "{} image{} · {} animation{}",
                                            entry.image_count,
                                            if entry.image_count == 1 { "" } else { "s" },
                                            entry.animation_count,
                                            if entry.animation_count == 1 { "" } else { "s" }
                                        )
                                    },
                                )),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.choose_subject(Some(id.clone()), cx)
                        })),
                );
            }
        }
        if self.subject_total == 0 {
            catalog = catalog.child(
                div()
                    .p_2()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("Ask Forge to create your characters, structures and scenery."),
            );
        }
        catalog = catalog.child(label("GAME FILES").mt_4()).child(
            Button::new("game-files")
                .label("References & other assets")
                .small()
                .ghost()
                .when(self.subject_id.is_none(), |b| b.bg(rgb(0xe3e9de)))
                .on_click(cx.listener(|this, _, _, cx| this.choose_subject(None, cx))),
        );
        let pages = div()
            .flex()
            .items_center()
            .justify_between()
            .child(label(format!("{} SUBJECTS", self.subject_total)))
            .child(
                div()
                    .flex()
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
        div()
            .w(px(220.))
            .flex_shrink_0()
            .h_full()
            .p_3()
            .border_r_1()
            .border_color(rgb(LINE))
            .bg(rgb(0xf1f2e9))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(label("GAMES"))
                    .child(
                        Button::new("new-game")
                            .label("+ New")
                            .small()
                            .ghost()
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, window, cx| this.new_game(window, cx))),
                    ),
            )
            .child(games)
            .child(
                div()
                    .mt_2()
                    .pt_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(label("THIS GAME'S LIBRARY")),
            )
            .child(catalog)
            .child(pages)
    }

    fn media_browser(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let busy = self.guide_busy() || self.management_busy || self.rendering();
        let mut header = div().flex().gap_4().items_center();
        let subject = self.subject();
        if let Some(subject) = subject {
            let preview = self
                .library_subjects
                .iter()
                .find(|s| s.subject.id == subject.id)
                .and_then(|s| s.preview.as_ref())
                .or_else(|| {
                    self.cache
                        .values()
                        .filter(|a| {
                            a.character_id.as_ref() == Some(&subject.id)
                                && a.kind != AssetKind::SpriteSheet
                        })
                        .min_by_key(|a| a.created_at)
                });
            if let Some(asset) = preview {
                header = header.child(
                    div()
                        .size(px(84.))
                        .flex_shrink_0()
                        .rounded_md()
                        .bg(rgb(0xe9eae1))
                        .child(
                            img(PathBuf::from(&asset.path))
                                .size_full()
                                .object_fit(ObjectFit::Contain),
                        ),
                );
            }
        }
        header = header.child(div().flex_1().min_w_0().flex().flex_col().gap_2()
            .child(label(subject.map(|s| s.kind.label()).unwrap_or("GAME FILES")))
            .child(div().font_family("Lora").text_size(px(28.)).child(subject.map(|s| s.name.clone()).unwrap_or_else(|| "References & other assets".into())))
            .child(div().text_sm().text_color(rgb(MUTED)).child(subject.map(|s| s.description.clone()).unwrap_or_else(|| "Concept boards and files for the whole game. Character assets live with their character.".into()))));
        if let Some(subject) = subject {
            let saved = subject.clone();
            header = header.child(
                Button::new("subject-chat")
                    .label("Add to chat")
                    .small()
                    .ghost()
                    .disabled(self.guide_busy())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.discuss_subject(saved.clone(), window, cx)
                    })),
            );
        }
        let mut filters = div().flex().items_center().gap_2();
        for (filter, name) in [
            (Filter::All, "All"),
            (Filter::Images, "Images"),
            (Filter::Animations, "Animations"),
        ] {
            filters = filters.child(
                Button::new(SharedString::from(format!("media-filter-{name}")))
                    .label(name)
                    .small()
                    .ghost()
                    .when(self.filter == filter, |b| b.bg(rgb(0xe3e9de)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.filter = filter;
                        cx.notify();
                    })),
            );
        }
        let mut content = div()
            .id("subject-media")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_3();
        for job in self
            .active_jobs
            .iter()
            .filter(|j| j.request.character_id == self.subject_id)
        {
            let id = job.id.clone();
            content = content.child(
                div()
                    .p_3()
                    .rounded_md()
                    .bg(rgb(0xe9eae1))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child(format!(
                            "{} · {}",
                            job.request
                                .animation
                                .as_ref()
                                .map(|c| c.name.as_str())
                                .unwrap_or("New asset"),
                            if job.status == JobStatus::Running {
                                "Rendering…"
                            } else {
                                "Queued"
                            }
                        )))
                    .child(
                        Button::new(SharedString::from(format!("cancel-{id}")))
                            .label("Cancel")
                            .small()
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, _| {
                                this.send("jobs/cancel", json!({"id":id}))
                            })),
                    ),
            );
        }
        if let Some(subject) = subject {
            let references: Vec<_> = subject
                .reference_asset_ids
                .iter()
                .filter_map(|id| self.cache.get(id))
                .filter(|a| a.character_id.as_ref() != Some(&subject.id))
                .collect();
            if !references.is_empty() {
                let mut links = div().flex().flex_wrap().gap_2();
                for asset in references {
                    let id = asset.id.clone();
                    links =
                        links.child(
                            Button::new(SharedString::from(format!("reference-{id}")))
                                .label(asset.name.clone())
                                .small()
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.choose_asset(id.clone(), cx)
                                })),
                        );
                }
                content = content.child(label("LINKED REFERENCES")).child(links);
            }
        }
        if self.filter != Filter::Animations {
            content = content.child(label(format!("IMAGES / {}", self.asset_total)));
            let mut cards = div().flex().flex_wrap().gap_3();
            for asset in &self.assets {
                let id = asset.id.clone();
                let deleted = asset.clone();
                let identity_ref =
                    subject.is_some_and(|s| s.reference_asset_ids.contains(&asset.id));
                let title = if asset.kind == AssetKind::SpriteSheet {
                    self.animation_cache
                        .values()
                        .find(|c| c.source_asset_id.as_ref() == Some(&asset.id))
                        .map(|c| format!("{} · atlas", self.clip_title(c)))
                        .unwrap_or_else(|| "Sprite atlas".into())
                } else {
                    asset.name.clone()
                };
                cards = cards.child(
                    div()
                        .w(px(184.))
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(LINE))
                        .bg(rgb(0xffffff))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .id(SharedString::from(format!("open-image-{id}")))
                                .p_3()
                                .cursor_pointer()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(
                                    img(PathBuf::from(&asset.path))
                                        .w_full()
                                        .h(px(106.))
                                        .object_fit(ObjectFit::Contain),
                                )
                                .child(div().text_sm().child(title))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.choose_asset(id.clone(), cx)
                                })),
                        )
                        .child(
                            div()
                                .px_3()
                                .pb_2()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(div().text_size(px(10.)).text_color(rgb(MUTED)).child(
                                    if identity_ref {
                                        "Identity reference".into()
                                    } else {
                                        format!("{} × {} · PNG", asset.width, asset.height)
                                    },
                                ))
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "delete-image-{}",
                                        asset.id
                                    )))
                                    .label("×")
                                    .tooltip("Delete image")
                                    .xsmall()
                                    .ghost()
                                    .disabled(busy)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.confirm_delete(
                                            "asset",
                                            deleted.id.clone(),
                                            deleted.name.clone(),
                                            window,
                                            cx,
                                        )
                                    })),
                                ),
                        ),
                );
            }
            content = content.child(cards);
        }
        if self.filter != Filter::Images {
            content = content.child(label(format!("ANIMATIONS / {}", self.animation_total)));
            let mut cards = div().flex().flex_wrap().gap_3();
            for clip in &self.animations {
                let id = clip.id.clone();
                let deleted = clip.clone();
                let status = match clip.status {
                    JobStatus::Queued => "Queued",
                    JobStatus::Running => "Rendering",
                    JobStatus::Succeeded => "Ready",
                    JobStatus::Failed => "Failed",
                    JobStatus::Cancelled => "Cancelled",
                    JobStatus::Unknown => "Needs review",
                };
                let mut thumbnail = div()
                    .w_full()
                    .h(px(106.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(MUTED))
                    .child("▶");
                if let Some(frame) = clip.frames.first() {
                    thumbnail = div()
                        .w_full()
                        .h(px(106.))
                        .flex_shrink_0()
                        .overflow_hidden()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            img(PathBuf::from(&frame.path))
                                .size(px(96.))
                                .flex_shrink_0()
                                .object_fit(ObjectFit::Contain),
                        );
                }
                cards = cards.child(
                    div()
                        .w(px(184.))
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(LINE))
                        .bg(rgb(0xffffff))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .id(SharedString::from(format!("open-clip-{id}")))
                                .p_3()
                                .cursor_pointer()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(thumbnail)
                                .child(div().text_sm().child(self.clip_title(clip)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.choose_clip(id.clone(), cx)
                                })),
                        )
                        .child(
                            div()
                                .px_3()
                                .pb_2()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(rgb(MUTED))
                                        .child(format!("{} · {} FPS", status, clip.config.fps)),
                                )
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "delete-clip-{}",
                                        clip.id
                                    )))
                                    .label("×")
                                    .tooltip("Delete animation")
                                    .xsmall()
                                    .ghost()
                                    .disabled(busy)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.confirm_delete(
                                            "animation",
                                            deleted.id.clone(),
                                            deleted.config.name.clone(),
                                            window,
                                            cx,
                                        )
                                    })),
                                ),
                        ),
                );
            }
            content = content.child(cards);
        }
        if self.asset_total + self.animation_total == 0 {
            content = content.child(
                div()
                    .p_4()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child("Ask Forge to make an image or animation. It will appear here."),
            );
        }
        let pages = div()
            .flex()
            .items_center()
            .justify_between()
            .child(label(format!(
                "{} IMAGES · {} ANIMATIONS",
                self.asset_total, self.animation_total
            )))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        Button::new("media-prev")
                            .label("←")
                            .small()
                            .ghost()
                            .disabled(!self.has_previous_page())
                            .on_click(cx.listener(|this, _, _, cx| this.turn_page(false, cx))),
                    )
                    .child(
                        Button::new("media-next")
                            .label("→")
                            .small()
                            .ghost()
                            .disabled(!self.has_next_page())
                            .on_click(cx.listener(|this, _, _, cx| this.turn_page(true, cx))),
                    ),
            );
        div()
            .id("subject-workspace")
            .flex_1()
            .min_h_0()
            .p_5()
            .flex()
            .flex_col()
            .gap_4()
            .child(header)
            .child(filters)
            .child(content)
            .child(pages)
    }
    fn library(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        if self.selected.is_none() && self.animation_id.is_none() {
            return self.media_browser(cx);
        }
        let clip = self.clip();
        let asset = self.selected_asset();
        let preview = if let Some(clip) = clip
            && !self.show_atlas
        {
            clip.frames
                .get(self.frame_index.min(clip.frames.len().saturating_sub(1)))
                .map(|f| f.path.clone())
                .or_else(|| asset.map(|a| a.path.clone()))
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
                    if ready { self.frame_index + 1 } else { 0 },
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
        div()
            .id("library-workspace")
            .flex_1()
            .min_h_0()
            .p_5()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                Button::new("back-to-subject")
                    .label(format!(
                        "← {}",
                        self.subject()
                            .map(|s| s.name.as_str())
                            .unwrap_or("Game files")
                    ))
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.browse_subject(cx))),
            )
            .child(tools)
            .when_some(clip.and_then(|c| c.error.as_ref()), |d, error| {
                d.child(
                    div()
                        .p_3()
                        .rounded_md()
                        .bg(rgb(0xf3e5dc))
                        .text_sm()
                        .child(error.message.clone()),
                )
            })
            .child(canvas)
            .when(clip.is_some(), |d| d.child(playback))
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
                (SubjectKind::Scene, "SCENES"),
            ] {
                let subjects: Vec<_> = self.subjects.iter().filter(|s| s.kind == kind).collect();
                if subjects.is_empty() {
                    continue;
                }
                content = content.child(label(format!("{title} / {}", subjects.len())));
                for subject in subjects {
                    let saved = subject.clone();
                    let browse_id = subject.id.clone();
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
                                                "subject-browse-{}",
                                                subject.id
                                            )))
                                            .label("View assets")
                                            .xsmall()
                                            .ghost()
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.choose_subject(Some(browse_id.clone()), cx)
                                            })),
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
                .child(div().text_sm().line_height(px(23.)).text_color(rgb(MUTED)).child("Describe your game or the next change. We'll choose its direction together, then create the assets."))
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
        if let Some(question) = self
            .guide
            .as_ref()
            .and_then(|g| g.pending_question.as_ref())
        {
            let mut choices = div().flex().flex_wrap().gap_2();
            for option in &question.options {
                let option_id = option.id.clone();
                choices = choices.child(
                    Button::new(SharedString::from(format!(
                        "answer-{}-{}",
                        question.id, option.id
                    )))
                    .label(option.label.clone())
                    .small()
                    .rounded_full()
                    .bg(rgb(0xe7ece2))
                    .disabled(self.guide_busy() || self.management_busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.answer_choice(Some(option_id.clone()), false, cx)
                    })),
                );
            }
            conversation = conversation.child(
                div()
                    .p_3()
                    .rounded_lg()
                    .bg(rgb(0xffffff))
                    .border_1()
                    .border_color(rgb(LINE))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(label("YOUR CHOICE"))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(question.prompt.clone()),
                    )
                    .child(choices)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(MUTED))
                                    .child("Or type your own answer below."),
                            )
                            .child(
                                Button::new("skip-question")
                                    .label("Skip →")
                                    .small()
                                    .ghost()
                                    .disabled(self.guide_busy() || self.management_busy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.answer_choice(None, true, cx)
                                    })),
                            ),
                    ),
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
                .child(Input::new(&self.guide_input).h(px(90.)).disabled(self.guide_busy()||self.management_busy))
                .child(div().flex().gap_2().items_center()
                    .child(Button::new("attach-image").label("+ Image").small().ghost().disabled(self.project.is_none()||self.references.len()>=8||self.guide_busy())
                        .on_click(cx.listener(|this,_,window,cx|this.attach_image(window,cx))))
                    .child(Button::new("send").label(if self.guide_busy(){"Working…"}else if self.guide.as_ref().is_some_and(|g|g.pending_question.is_some()) {"Send answer →"}else{"Send to Forge →"}).primary().flex_1().disabled(!connected||self.guide_busy()||self.management_busy)
                        .on_click(cx.listener(|this,_,_,cx|this.ask_guide(cx)))))
                .when(self.guide_busy()||self.rendering(),|d|d.child(Button::new("stop").label("Stop").small().ghost().w_full()
                    .on_click(cx.listener(|this,_,_,_|{
                        if let Some(guide)=&this.guide && guide.status==AssistantStatus::Thinking {this.send("assistant/cancel",json!({"id":guide.id}));}
                        for job in &this.active_jobs {this.send("jobs/cancel",json!({"id":job.id}));}
                    }))))
                .child(div().text_size(px(10.)).text_color(rgb(MUTED)).child(if connected{"Add an asset to chat to revise it. Forge keeps your saved style and references."}else{"Connect Codex above to begin."})))
    }
}

impl Render for Studio {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dialog_layer = Root::render_dialog_layer(window, cx);
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
            .h(px(60.))
            .flex_shrink_0()
            .px_5()
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
                            .text_size(px(23.))
                            .child("Asset Forge"),
                    )
                    .child(label("2D WORKSHOP")),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        Button::new("update")
                            .label(self.update_label.clone())
                            .disabled(self.update_busy)
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| this.update_application(cx))),
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
                            .tooltip("Refresh games, assets and Codex")
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, _| {
                                this.refresh();
                                this.send("projects/list", json!({"pageSize":100}));
                                this.send("account/read", json!({}));
                            })),
                    ),
            );
        let mut game_title = div()
            .h(px(60.))
            .px_5()
            .flex_shrink_0()
            .flex()
            .gap_2()
            .items_center();
        if self.renaming {
            game_title = game_title
                .child(
                    Input::new(&self.name_input)
                        .flex_1()
                        .disabled(self.management_busy),
                )
                .child(
                    Button::new("save-name")
                        .label("Save")
                        .small()
                        .primary()
                        .disabled(self.management_busy)
                        .on_click(cx.listener(|this, _, _, cx| this.save_name(cx))),
                )
                .child(
                    Button::new("cancel-name")
                        .label("Cancel")
                        .small()
                        .ghost()
                        .disabled(self.management_busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.renaming = false;
                            cx.notify();
                        })),
                );
        } else {
            game_title = game_title
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_family("Lora")
                        .text_size(px(25.))
                        .truncate()
                        .child(
                            self.project
                                .as_ref()
                                .map(|p| p.name.clone())
                                .unwrap_or_else(|| "New game".into()),
                        ),
                )
                .when(self.project.is_some(), |d| {
                    d.child(
                        Button::new("rename-game")
                            .label("Rename")
                            .small()
                            .ghost()
                            .disabled(self.guide_busy() || self.management_busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.begin_rename(window, cx)),
                            ),
                    )
                });
        }
        let mut tabs = div()
            .h(px(43.))
            .px_5()
            .flex_shrink_0()
            .border_b_1()
            .border_color(rgb(LINE))
            .flex()
            .items_center()
            .gap_1();
        for (view, name) in [(View::Library, "Library"), (View::World, "World")] {
            tabs = tabs.child(
                Button::new(name)
                    .label(name)
                    .small()
                    .ghost()
                    .when(self.view == view, |b| b.bg(rgb(0xe7ece2)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.view = view;
                        cx.notify();
                    })),
            );
        }
        let workspace = match self.view {
            View::Library => self.library(cx).into_any_element(),
            View::World => self.world(cx).into_any_element(),
        };
        let login = div()
            .px_5()
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
            .px_5()
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
            .child(div().flex_1().truncate().child(self.status.clone()))
            .when_some(self.last_deletion.as_ref(), |d, deletion| {
                let method = match deletion.kind.as_str() {
                    "project" => "projects/restore",
                    "animation" => "animations/restore",
                    _ => "assets/restore",
                };
                let id = deletion.id.clone();
                d.child(
                    Button::new("undo-delete")
                        .label("Undo deletion")
                        .xsmall()
                        .ghost()
                        .disabled(self.management_busy || self.guide_busy() || self.rendering())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.management_busy = true;
                            this.send(method, json!({"id":id}));
                            cx.notify();
                        })),
                )
            });
        div()
            .size_full()
            .relative()
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
                    .child(self.sidebar(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .child(game_title)
                            .child(tabs)
                            .child(workspace),
                    )
                    .child(self.guide_panel(cx)),
            )
            .child(footer)
            .children(dialog_layer)
    }
}
