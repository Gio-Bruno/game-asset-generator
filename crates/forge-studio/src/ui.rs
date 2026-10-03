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
    path::PathBuf,
    sync::mpsc::Receiver,
    time::{Duration, Instant},
};

const PAPER: u32 = 0xf8f6f0;
const INK: u32 = 0x272d29;
const MUTED: u32 = 0x737c72;
const LINE: u32 = 0xdadbd2;
const GREEN: u32 = 0x2f5548;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Create,
    Style,
    Characters,
    Animate,
}

pub struct Studio {
    commands: tokio::sync::mpsc::UnboundedSender<Command>,
    responses: Receiver<Response>,
    projects: Vec<Project>,
    project: Option<Project>,
    characters: Vec<Character>,
    assets: Vec<Asset>,
    character: Option<String>,
    selected: Option<String>,
    references: Vec<String>,
    account: Option<AccountStatus>,
    login: Option<Login>,
    job: Option<Job>,
    is_submitting: bool,
    tab: Tab,
    kind: AssetKind,
    width: u32,
    height: u32,
    transparent: bool,
    status: String,
    is_error: bool,
    project_name: Entity<InputState>,
    style_name: Entity<InputState>,
    direction: Entity<InputState>,
    palette: Entity<InputState>,
    perspective: Entity<InputState>,
    lighting: Entity<InputState>,
    character_name: Entity<InputState>,
    identity: Entity<InputState>,
    brief: Entity<InputState>,
    previews: Vec<std::sync::Arc<Image>>,
    presets: Vec<StylePreset>,
    preset: String,
    advanced: bool,
    guide: Option<AssistantSession>,
    guide_input: Entity<InputState>,
    guide_scroll: ScrollHandle,
    guide_stream: String,
    guide_actions: Vec<String>,
    guide_submitting: bool,
    asset_page: usize,
    asset_total: usize,
    animations: Vec<Animation>,
    animation_id: Option<String>,
    animation_atlas: Option<Asset>,
    animation_config: AnimationConfig,
    animation_brief: Entity<InputState>,
    frame_width_input: Entity<InputState>,
    frame_height_input: Entity<InputState>,
    margin_input: Entity<InputState>,
    spacing_input: Entity<InputState>,
    animation_advanced: bool,
    show_atlas: bool,
    playing: bool,
    play_started: Instant,
    frame_index: usize,
}

fn field(
    window: &mut Window,
    cx: &mut App,
    placeholder: &str,
    multiline: bool,
) -> Entity<InputState> {
    cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(placeholder.to_owned())
            .multi_line(multiline)
            .rows(if multiline { 4 } else { 1 })
    })
}
fn value(field: &Entity<InputState>, cx: &App) -> String {
    field.read(cx).value().to_string()
}
fn set(
    field: &Entity<InputState>,
    text: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
) {
    let text = text.into();
    field.update(cx, |field, cx| field.set_value(text, window, cx));
}
fn label(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(MUTED))
        .child(text.into())
}
fn section(title: &str, field: &Entity<InputState>, hint: &str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(label(title.to_owned()))
        .child(Input::new(field).when(
            ["THE BRIEF", "ART DIRECTION", "IDENTITY"].contains(&title),
            |input| input.h(px(110.)),
        ))
        .when(!hint.is_empty(), |d| {
            d.child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(hint.to_owned()),
            )
        })
}

impl Studio {
    pub fn new(
        commands: tokio::sync::mpsc::UnboundedSender<Command>,
        responses: Receiver<Response>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut studio = Self {
            commands,
            responses,
            projects: vec![],
            project: None,
            characters: vec![],
            assets: vec![],
            character: None,
            selected: None,
            references: vec![],
            account: None,
            login: None,
            job: None,
            is_submitting: false,
            tab: Tab::Style,
            kind: AssetKind::Character,
            width: 512,
            height: 512,
            transparent: true,
            status: "Give your game a visual language. Save a style to begin.".into(),
            is_error: false,
            project_name: field(window, cx, "Your game's name", false),
            style_name: field(window, cx, "e.g. Woodland ink", false),
            direction: field(
                window,
                cx,
                "Describe line work, shapes, textures and mood…",
                true,
            ),
            palette: field(window, cx, "#315C4B, #D7AD70, #F1E9D5", false),
            perspective: field(window, cx, "e.g. Side view, orthographic", false),
            lighting: field(window, cx, "e.g. Soft light from upper left", false),
            character_name: field(window, cx, "e.g. Mira, the forest scout", false),
            identity: field(window, cx, "Face, silhouette, costume, proportions…", true),
            brief: field(
                window,
                cx,
                "Describe the asset or the next variation…",
                true,
            ),
            previews: vec![
                std::sync::Arc::new(Image::from_bytes(
                    ImageFormat::Png,
                    include_bytes!("../../../examples/generated/woodland-scene.png").to_vec(),
                )),
                std::sync::Arc::new(Image::from_bytes(
                    ImageFormat::Svg,
                    include_bytes!("../../../assets/presets/pixel.svg").to_vec(),
                )),
                std::sync::Arc::new(Image::from_bytes(
                    ImageFormat::Svg,
                    include_bytes!("../../../assets/presets/flat.svg").to_vec(),
                )),
                std::sync::Arc::new(Image::from_bytes(
                    ImageFormat::Svg,
                    include_bytes!("../../../assets/presets/ink.svg").to_vec(),
                )),
                std::sync::Arc::new(Image::from_bytes(
                    ImageFormat::Svg,
                    include_bytes!("../../../assets/presets/paint.svg").to_vec(),
                )),
                std::sync::Arc::new(Image::from_bytes(
                    ImageFormat::Svg,
                    include_bytes!("../../../assets/presets/isometric.svg").to_vec(),
                )),
            ],
            presets: forge_core::presets::all(),
            preset: "woodland".into(),
            advanced: false,
            guide: None,
            guide_input: field(window, cx, "Tell Forge what you want to make…", true),
            guide_scroll: ScrollHandle::new(),
            guide_stream: String::new(),
            guide_actions: vec![],
            guide_submitting: false,
            asset_page: 1,
            asset_total: 0,
            animations: vec![],
            animation_id: None,
            animation_atlas: None,
            animation_config: AnimationConfig::default(),
            animation_brief: field(
                window,
                cx,
                "Optional: facing direction, expression or motion detail…",
                true,
            ),
            frame_width_input: field(window, cx, "Width", false),
            frame_height_input: field(window, cx, "Height", false),
            margin_input: field(window, cx, "Margin", false),
            spacing_input: field(window, cx, "Spacing", false),
            animation_advanced: false,
            show_atlas: false,
            playing: true,
            play_started: Instant::now(),
            frame_index: 0,
        };
        set(&studio.project_name, "My game", window, cx);
        studio.apply_preset("woodland", window, cx);
        studio.reset_motion(Motion::Idle, window, cx);
        studio.send("projects/list", json!({}));
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
    fn message(&mut self, message: impl Into<String>, error: bool, cx: &mut Context<Self>) {
        self.status = message.into();
        self.is_error = error;
        cx.notify();
    }
    fn project_id(&self) -> Option<String> {
        self.project.as_ref().map(|p| p.id.clone())
    }
    fn refresh(&self) {
        if let Some(id) = self.project_id() {
            self.send(
                "assets/list",
                json!({"projectId":id,"page":self.asset_page,"pageSize":24}),
            );
            self.send("characters/list", json!({"projectId":id}));
            self.send("jobs/list", json!({"projectId":id,"pageSize":1}));
            self.send("animations/list", json!({"projectId":id}));
        }
    }
    fn choose_project(&mut self, project: Project, window: &mut Window, cx: &mut Context<Self>) {
        set(&self.project_name, project.name.clone(), window, cx);
        set(&self.style_name, project.style.name.clone(), window, cx);
        set(
            &self.direction,
            project.style.description.clone(),
            window,
            cx,
        );
        set(&self.palette, project.style.palette.join(", "), window, cx);
        set(
            &self.perspective,
            project.style.perspective.clone(),
            window,
            cx,
        );
        set(&self.lighting, project.style.lighting.clone(), window, cx);
        let same = self.project.as_ref().is_some_and(|p| p.id == project.id);
        let explicit_size = same
            && self.project.as_ref().is_some_and(|p| {
                forge_core::presets::output_size(&p.style, self.kind) != (self.width, self.height)
            });
        if !explicit_size {
            (self.width, self.height) = forge_core::presets::output_size(&project.style, self.kind);
        }
        if self
            .guide
            .as_ref()
            .is_some_and(|g| g.project_id.as_deref() != Some(&project.id))
        {
            self.guide = None;
            self.guide_stream.clear();
            self.guide_actions.clear();
        }
        self.preset = forge_core::presets::selected_id(&project.style).unwrap_or_default();
        self.project = Some(project);
        if same {
            self.refresh();
            cx.notify();
            return;
        }
        self.send(
            "assistant/list",
            json!({"projectId":self.project_id(),"pageSize":1}),
        );
        self.characters.clear();
        self.character = None;
        self.selected = None;
        self.references.clear();
        self.assets.clear();
        self.job = None;
        self.animations.clear();
        self.animation_id = None;
        self.animation_atlas = None;
        self.reset_motion(Motion::Idle, window, cx);
        self.asset_page = 1;
        self.refresh();
        if self.status == "Give your game a visual language. Save a style to begin." {
            self.status = "Workspace ready. Choose an asset or ask Forge for the next step.".into();
        }
        cx.notify();
    }

    fn receive(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut changed = false;
        while let Ok(response) = self.responses.try_recv() {
            changed = true;
            match response {
                Response::Event(event) => {
                    if event.kind == "ACCOUNT_CONNECTED" {
                        self.login = None;
                        self.message("Codex connected. You can start creating assets.", false, cx);
                        self.send("account/read", json!({}));
                    }
                    if event.kind == "LOGIN_FAILED" {
                        self.login = None;
                        self.message(event.message, true, cx);
                        continue;
                    }
                    if event.kind == "RESYNC_REQUIRED" {
                        self.refresh();
                    }
                    if event.session_id.is_some()
                        && event.session_id.as_deref() == self.guide.as_ref().map(|g| g.id.as_str())
                    {
                        match event.kind.as_str() {
                            "ASSISTANT_DELTA" => {
                                self.guide_stream.push_str(&event.message);
                                self.guide_scroll.scroll_to_bottom();
                            }
                            "ASSISTANT_ACTION" => {
                                self.guide_actions.push(event.message.clone());
                                self.send("assistant/get", json!({"id":event.session_id}));
                            }
                            "ASSISTANT_FINISHED" => {
                                self.send("assistant/get", json!({"id":event.session_id}));
                                self.refresh();
                            }
                            _ => {}
                        }
                    }
                    if event.kind == "JOB_RUNNING" || event.kind == "JOB_FINISHED" {
                        self.refresh();
                    }
                    if event.job_id.is_some()
                        && event.job_id.as_deref() == self.job.as_ref().map(|j| j.id.as_str())
                    {
                        self.status = event.message;
                        self.is_error = false;
                        if event.kind == "JOB_FINISHED" {
                            if let Some(job) = &self.job {
                                self.send("jobs/get", json!({"id":job.id}));
                            }
                            self.refresh();
                        }
                    }
                }
                Response::Reply(method, params, result) => {
                    self.reply(&method, &params, result, window, cx)
                }
            }
        }
        if self.tab == Tab::Animate
            && self.playing
            && !self.show_atlas
            && let Some(clip) = self
                .animations
                .iter()
                .find(|a| Some(&a.id) == self.animation_id.as_ref())
            && !clip.frames.is_empty()
        {
            let elapsed =
                (self.play_started.elapsed().as_secs_f64() * clip.config.fps as f64) as usize;
            let next = if clip.config.is_looping {
                elapsed % clip.frames.len()
            } else {
                elapsed.min(clip.frames.len() - 1)
            };
            changed |= next != self.frame_index;
            self.frame_index = next;
            if !clip.config.is_looping && elapsed >= clip.frames.len() {
                self.playing = false;
                changed = true;
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
        // Ignore read responses that arrive after the user switches projects/sessions.
        if [
            "assets/list",
            "assets/import",
            "characters/list",
            "jobs/list",
            "assistant/list",
            "animations/list",
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
        let v = match result {
            Ok(v) => v,
            Err(e) => {
                if method == "jobs/create" || method == "animations/create" {
                    self.is_submitting = false;
                }
                if method == "assistant/message" {
                    self.guide_submitting = false;
                }
                self.message(e.message, true, cx);
                return;
            }
        };
        match method {
            "projects/list" => {
                if let Ok(page) = serde_json::from_value::<Page<Project>>(v) {
                    self.projects = page.data;
                    if self.project.is_none()
                        && self.guide.is_none()
                        && let Some(project) = self.projects.first().cloned()
                    {
                        self.choose_project(project, window, cx);
                        self.tab = Tab::Create;
                    }
                }
            }
            "projects/create" | "projects/style/update" | "projects/get" => {
                if let Ok(project) = serde_json::from_value::<Project>(v) {
                    if let Some(p) = self.projects.iter_mut().find(|p| p.id == project.id) {
                        *p = project.clone();
                    } else {
                        self.projects.push(project.clone());
                    }
                    if method == "projects/style/update"
                        && self.project_id().as_deref() != Some(&project.id)
                    {
                        return;
                    }
                    if method == "projects/create" || method == "projects/get" {
                        self.choose_project(project, window, cx);
                    } else {
                        self.project = Some(project);
                        self.refresh();
                    }
                    if method != "projects/get" {
                        self.tab = Tab::Create;
                    }
                    if method != "projects/get" {
                        self.message(
                            "Style saved. Every new asset will follow these visual rules.",
                            false,
                            cx,
                        );
                    }
                }
            }
            "account/read" => {
                if let Ok(account) = serde_json::from_value::<AccountStatus>(v) {
                    self.account = Some(account);
                }
            }
            "account/login/start" => {
                if let Ok(login) = serde_json::from_value::<Login>(v) {
                    if let Err(e) = open::that(&login.auth_url) {
                        self.message(
                            format!(
                                "Could not open the browser: {e}. Use Open sign-in to try again."
                            ),
                            true,
                            cx,
                        );
                    } else {
                        self.message("Finish connecting your account in the browser.", false, cx);
                    }
                    self.login = Some(login);
                }
            }
            "account/login/cancel" => {
                self.login = None;
                self.message("Sign-in cancelled.", false, cx);
            }
            "characters/list" => {
                if let Ok(page) = serde_json::from_value::<Page<Character>>(v) {
                    let default_character = self.characters.is_empty() && self.character.is_none();
                    self.characters = page.data;
                    if default_character {
                        self.character = self.characters.first().map(|c| c.id.clone());
                    }
                }
            }
            "characters/create" | "characters/references/update" => {
                if let Ok(c) = serde_json::from_value::<Character>(v) {
                    if self.project_id().as_deref() != Some(&c.project_id) {
                        return;
                    }
                    self.character = Some(c.id.clone());
                    self.send("characters/list", json!({"projectId":c.project_id}));
                    self.message(
                        "Character saved. Choose it when creating a new pose or scene.",
                        false,
                        cx,
                    );
                }
            }
            "assets/list" => {
                if let Ok(page) = serde_json::from_value::<Page<Asset>>(v) {
                    self.assets = page.data;
                    self.asset_total = page.pagination.total_items;
                    if self.selected.is_none() {
                        self.selected = self.assets.first().map(|a| a.id.clone());
                    }
                }
            }
            "assets/import" => {
                if let Ok(asset) = serde_json::from_value::<Asset>(v) {
                    let sheet = asset.kind == AssetKind::SpriteSheet;
                    if sheet || self.tab != Tab::Animate {
                        self.selected = Some(asset.id.clone());
                    }
                    if !sheet && !self.references.contains(&asset.id) {
                        self.references.push(asset.id);
                    }
                    self.asset_page = 1;
                    self.refresh();
                    if sheet && self.tab == Tab::Animate {
                        self.animation_id = None;
                        self.show_atlas = true;
                        self.message(
                            "Sprite sheet imported. Fit its grid, then extract frames.",
                            false,
                            cx,
                        );
                    } else {
                        self.message("Reference added to your next generation. Pin it to a style or character to reuse it every time.",false,cx);
                    }
                }
            }
            "assets/get" => {
                if let Ok(asset) = serde_json::from_value::<Asset>(v)
                    && self
                        .animations
                        .iter()
                        .find(|a| Some(&a.id) == self.animation_id.as_ref())
                        .and_then(|a| a.source_asset_id.as_ref())
                        == Some(&asset.id)
                {
                    self.animation_atlas = Some(asset);
                }
            }
            "assets/export" => self.message("PNG exported to your chosen folder.", false, cx),
            "animations/export" => self.message(
                "Animation exported: atlas, frames, GIF preview and timing JSON.",
                false,
                cx,
            ),
            "animations/list" => {
                if let Ok(page) = serde_json::from_value::<Page<Animation>>(v) {
                    let selected = page
                        .data
                        .iter()
                        .find(|a| Some(&a.id) == self.animation_id.as_ref())
                        .cloned();
                    let refresh_selection = selected.as_ref().is_some_and(|a| {
                        self.animations
                            .iter()
                            .find(|old| old.id == a.id)
                            .is_none_or(|old| {
                                old.status != JobStatus::Succeeded
                                    && a.status == JobStatus::Succeeded
                            })
                    });
                    if let Some(updated) = &selected
                        && let Some(old) = self.animations.iter().find(|a| a.id == updated.id)
                    {
                        if self.animation_config.fps == old.config.fps {
                            self.animation_config.fps = updated.config.fps;
                        }
                        if self.animation_config.is_looping == old.config.is_looping {
                            self.animation_config.is_looping = updated.config.is_looping;
                        }
                    }
                    self.animations = page.data;
                    if self.animation_id.is_none() {
                        if let Some(clip) = self.animations.first().cloned() {
                            self.choose_animation(clip, window, cx);
                        }
                    } else if refresh_selection && let Some(clip) = selected {
                        self.choose_animation(clip, window, cx);
                    }
                }
            }
            "animations/create" | "animations/setup" | "animations/timing/update" => {
                self.is_submitting = false;
                if let Ok(clip) = serde_json::from_value::<Animation>(v) {
                    if self.project_id().as_deref() != Some(&clip.project_id) {
                        return;
                    }
                    self.choose_animation(clip.clone(), window, cx);
                    if let Some(job) = &clip.job_id {
                        self.send("jobs/get", json!({"id":job}));
                    }
                    self.refresh();
                    self.message(
                        if method == "animations/create" {
                            "Your sprite animation is queued…"
                        } else {
                            "Animation saved. Preview the motion and export when ready."
                        },
                        false,
                        cx,
                    );
                }
            }
            "jobs/create" | "jobs/get" => {
                self.is_submitting = false;
                if let Ok(job) = serde_json::from_value::<Job>(v) {
                    if self.project_id().as_deref() != Some(&job.project_id) {
                        return;
                    }
                    if let Some(id) = job.asset_ids.first() {
                        self.selected = Some(id.clone());
                    }
                    if let Some(error) = &job.error {
                        self.message(&error.message, true, cx);
                    }
                    self.job = Some(job);
                    if let Some(job) = &self.job
                        && job.request.animation.is_some()
                    {
                        self.animation_id = Some(job.id.clone());
                        self.tab = Tab::Animate;
                        self.send("animations/list", json!({"projectId":job.project_id}));
                    }
                }
            }
            "assistant/message" | "assistant/get" => {
                self.guide_submitting = false;
                if let Ok(session) = serde_json::from_value::<AssistantSession>(v) {
                    if method == "assistant/message" {
                        set(&self.guide_input, "", window, cx);
                    }
                    if let Some(project) = &session.project_id {
                        self.send("projects/get", json!({"id":project}));
                    }
                    if let Some(job) = session.generated_job_ids.last() {
                        self.send("jobs/get", json!({"id":job}));
                    }
                    if session.status != AssistantStatus::Thinking {
                        self.guide_stream.clear();
                    }
                    if let Some(error) = &session.error {
                        self.message(&error.message, true, cx);
                    }
                    self.guide = Some(session);
                    self.guide_scroll.scroll_to_bottom();
                }
            }
            "assistant/list" => {
                if let Ok(page) = serde_json::from_value::<Page<AssistantSession>>(v)
                    && self.guide.is_none()
                {
                    self.guide = page.data.into_iter().next();
                }
            }
            "jobs/list" => {
                if let Ok(page) = serde_json::from_value::<Page<Job>>(v) {
                    self.job = page.data.into_iter().next();
                }
            }
            _ => {}
        }
    }

    fn save_style(&mut self, cx: &mut Context<Self>) {
        let style = StyleGuide {
            preset_id: (!self.preset.is_empty()).then(|| self.preset.clone()),
            name: value(&self.style_name, cx),
            description: value(&self.direction, cx),
            palette: value(&self.palette, cx)
                .split([',', ' ', '\n'])
                .filter(|v| !v.is_empty())
                .map(String::from)
                .collect(),
            perspective: value(&self.perspective, cx),
            lighting: value(&self.lighting, cx),
            reference_asset_ids: self
                .project
                .as_ref()
                .map(|p| p.style.reference_asset_ids.clone())
                .unwrap_or_default(),
        };
        if let Some(id) = self.project_id() {
            self.send(
                "projects/style/update",
                json!({"projectId":id,"style":style}),
            );
        } else {
            self.send(
                "projects/create",
                json!({"name":value(&self.project_name,cx),"style":style}),
            );
        }
        self.message("Saving your style…", false, cx);
    }
    fn generate(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.project_id() else {
            self.tab = Tab::Style;
            self.message(
                "Save your game's style before generating the first asset.",
                true,
                cx,
            );
            return;
        };
        if self.busy() {
            return;
        }
        let brief = value(&self.brief, cx);
        if brief.trim().is_empty() {
            self.message("Describe the asset you want to create.", true, cx);
            return;
        }
        let input = GenerateInput {
            project_id: project,
            idempotency_key: uuid::Uuid::new_v4().to_string(),
            prompt: brief,
            kind: self.kind,
            character_id: self.character.clone(),
            reference_asset_ids: self.references.clone(),
            width: self.width,
            height: self.height,
            transparent_background: self.transparent,
            animation: None,
        };
        self.is_submitting = true;
        self.send("jobs/create", json!(input));
        self.message("Your asset is queued…", false, cx);
    }
    fn busy(&self) -> bool {
        self.is_submitting || self.job.as_ref().is_some_and(|j| !j.status.is_terminal())
    }
    fn import(&mut self, sheet: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project_id() else {
            self.message(
                "Save a project first, then import its reference images.",
                true,
                cx,
            );
            return;
        };
        let kind = if sheet {
            AssetKind::SpriteSheet
        } else {
            AssetKind::Character
        };
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import a PNG, JPEG or WebP reference".into()),
        });
        cx.spawn_in(window, async move |entity, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.first()
            {
                let path = path.clone();
                let _ = entity.update_in(cx, |studio, _, _| {
                    let name = path.file_stem().unwrap_or_default().to_string_lossy();
                    studio.send(
                        "assets/import",
                        json!({"projectId":project,"path":path,"name":name,"kind":kind}),
                    );
                });
            }
        })
        .detach();
    }
    fn export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(asset) = self
            .assets
            .iter()
            .find(|a| Some(&a.id) == self.selected.as_ref())
            .cloned()
        else {
            return;
        };
        let filename = format!(
            "{}-{}.png",
            asset.name.replace(|c: char| !c.is_alphanumeric(), "-"),
            &asset.id[..8]
        );
        let directory = forge_core::default_export_dir();
        let picker = cx.prompt_for_new_path(&directory, Some(&filename));
        cx.spawn_in(window, async move |entity, cx| {
            if let Ok(Ok(Some(path))) = picker.await {
                let _ = entity.update_in(cx, |studio, _, _| {
                    studio.send("assets/export", json!({"id":asset.id,"path":path}))
                });
            }
        })
        .detach();
    }
    fn reference_controls(&self, cx: &mut Context<Self>) -> Div {
        let mut row = div().flex().items_center().gap_1().flex_wrap().child(
            Button::new("add-reference")
                .label("+ Image reference")
                .xsmall()
                .ghost()
                .disabled(self.project.is_none() || self.references.len() >= 8)
                .on_click(cx.listener(|this, _, window, cx| this.import(false, window, cx))),
        );
        for (index, id) in self.references.iter().enumerate() {
            let id = id.clone();
            let asset = self.assets.iter().find(|a| a.id == id);
            let mut chip = div()
                .flex()
                .items_center()
                .gap_1()
                .rounded_md()
                .bg(rgb(0xe7ece2));
            if let Some(asset) = asset {
                chip = chip.child(
                    img(PathBuf::from(&asset.path))
                        .w(px(28.))
                        .h(px(28.))
                        .object_fit(ObjectFit::Contain),
                );
            }
            row = row.child(
                chip.child(
                    Button::new(SharedString::from(format!("remove-reference-{id}")))
                        .label(format!(
                            "{} ×",
                            asset
                                .map(|a| a.name.clone())
                                .unwrap_or_else(|| format!("Reference {}", index + 1))
                        ))
                        .xsmall()
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.references.retain(|r| r != &id);
                            cx.notify();
                        })),
                ),
            );
        }
        row
    }
    fn style_reference(&mut self, cx: &mut Context<Self>) {
        let (Some(project), Some(selected)) = (&self.project, &self.selected) else {
            return;
        };
        let mut style = project.style.clone();
        if style.reference_asset_ids.contains(selected) {
            style.reference_asset_ids.retain(|id| id != selected);
        } else {
            style.reference_asset_ids.push(selected.clone());
        }
        self.send(
            "projects/style/update",
            json!({"projectId":project.id,"style":style}),
        );
        self.message("Updating style reference…", false, cx);
    }
    fn character_reference(&mut self, cx: &mut Context<Self>) {
        let (Some(character), Some(selected)) = (
            self.characters
                .iter()
                .find(|c| Some(&c.id) == self.character.as_ref()),
            &self.selected,
        ) else {
            self.message("Choose a saved character first.", true, cx);
            return;
        };
        let mut references = character.reference_asset_ids.clone();
        if references.contains(selected) {
            references.retain(|id| id != selected);
        } else {
            references.push(selected.clone());
        }
        self.send(
            "characters/references/update",
            json!({"id":character.id,"referenceAssetIds":references}),
        );
    }

    fn guide_busy(&self) -> bool {
        self.guide_submitting
            || self
                .guide
                .as_ref()
                .is_some_and(|g| g.status == AssistantStatus::Thinking)
    }

    fn sync_grid_inputs(&self, window: &mut Window, cx: &mut Context<Self>) {
        set(
            &self.frame_width_input,
            self.animation_config.frame_width.to_string(),
            window,
            cx,
        );
        set(
            &self.frame_height_input,
            self.animation_config.frame_height.to_string(),
            window,
            cx,
        );
        set(
            &self.margin_input,
            self.animation_config.margin.to_string(),
            window,
            cx,
        );
        set(
            &self.spacing_input,
            self.animation_config.spacing.to_string(),
            window,
            cx,
        );
    }
    fn reset_motion(&mut self, motion: Motion, window: &mut Window, cx: &mut Context<Self>) {
        let style = self
            .project
            .as_ref()
            .map(|p| p.style.clone())
            .unwrap_or_else(|| self.presets[0].style.clone());
        self.animation_config = forge_core::animation::defaults(&style, motion);
        self.sync_grid_inputs(window, cx);
        self.animation_advanced = false;
    }
    fn choose_animation(&mut self, clip: Animation, window: &mut Window, cx: &mut Context<Self>) {
        self.animation_id = Some(clip.id.clone());
        self.animation_config = clip.config.clone();
        self.character = clip.character_id.clone().or_else(|| self.character.clone());
        self.selected = clip
            .source_asset_id
            .clone()
            .or_else(|| self.selected.clone());
        self.animation_atlas = None;
        if let Some(source) = &clip.source_asset_id {
            self.send("assets/get", json!({"id":source}));
        }
        self.sync_grid_inputs(window, cx);
        self.animation_advanced = clip.config.margin != 0
            || clip.config.spacing != 0
            || clip.config.frame_width != clip.config.frame_height;
        self.frame_index = 0;
        self.play_started = Instant::now();
        self.playing = true;
        self.show_atlas = false;
        if let Some(existing) = self.animations.iter_mut().find(|a| a.id == clip.id) {
            *existing = clip;
        } else {
            self.animations.insert(0, clip);
        }
        cx.notify();
    }
    fn read_animation_config(&self, cx: &App) -> Result<AnimationConfig> {
        let mut config = self.animation_config.clone();
        if self.animation_advanced {
            let parse = |field: &Entity<InputState>| {
                value(field, cx).parse::<u32>().map_err(|_| {
                    forge_core::error::ApiError::validation(
                        "Grid dimensions, margin and spacing must be whole numbers.",
                    )
                })
            };
            config.frame_width = parse(&self.frame_width_input)?;
            config.frame_height = parse(&self.frame_height_input)?;
            config.margin = parse(&self.margin_input)?;
            config.spacing = parse(&self.spacing_input)?;
        }
        config.validate()?;
        Ok(config)
    }
    fn generate_animation(&mut self, cx: &mut Context<Self>) {
        let (Some(project), Some(character)) = (self.project_id(), self.character.clone()) else {
            self.message(
                "Save a style and choose a character before generating an animation.",
                true,
                cx,
            );
            return;
        };
        if self.busy() {
            return;
        }
        match self.read_animation_config(cx) {
            Err(e) => self.message(e.message, true, cx),
            Ok(config) => {
                if config.margin != 0 || config.spacing != 0 {
                    self.message("Generation uses a grid without margin or spacing. Use those settings for imported sheets.",true,cx);
                    return;
                }
                self.send(
                    "animations/create",
                    json!(CreateAnimation {
                        project_id: project,
                        character_id: character,
                        idempotency_key: uuid::Uuid::new_v4().to_string(),
                        config,
                        prompt: value(&self.animation_brief, cx),
                        reference_asset_ids: self.references.clone()
                    }),
                );
                self.is_submitting = true;
                self.message("Your sprite animation is queued…", false, cx);
            }
        }
    }
    fn setup_animation(&mut self, cx: &mut Context<Self>) {
        let (Some(project), Some(asset)) = (self.project_id(), self.selected.clone()) else {
            return;
        };
        match self.read_animation_config(cx) {
            Err(e) => self.message(e.message, true, cx),
            Ok(config) => self.send(
                "animations/setup",
                json!(SetupAnimation {
                    project_id: project,
                    asset_id: asset,
                    character_id: self.character.clone(),
                    idempotency_key: uuid::Uuid::new_v4().to_string(),
                    config
                }),
            ),
        }
    }
    fn fit_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(asset) = self
            .assets
            .iter()
            .find(|a| Some(&a.id) == self.selected.as_ref())
        {
            self.animation_config.frame_width = asset.width / self.animation_config.columns;
            self.animation_config.frame_height = asset.height / self.animation_config.rows();
            self.animation_config.margin = 0;
            self.animation_config.spacing = 0;
            self.animation_advanced = true;
            self.show_atlas = true;
            self.sync_grid_inputs(window, cx);
            self.message("Grid fitted to the selected sheet. Check cell size and padding, then extract frames.",false,cx);
        }
    }
    fn export_animation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(clip) = self
            .animations
            .iter()
            .find(|a| Some(&a.id) == self.animation_id.as_ref())
            .cloned()
        else {
            return;
        };
        let filename = format!(
            "{}-{}.zip",
            clip.config
                .name
                .replace(|c: char| !c.is_alphanumeric(), "-"),
            &clip.id[..8]
        );
        let picker = cx.prompt_for_new_path(&forge_core::default_export_dir(), Some(&filename));
        cx.spawn_in(window, async move |entity, cx| {
            if let Ok(Ok(Some(path))) = picker.await {
                let _ = entity.update_in(cx, |studio, _, _| {
                    studio.send("animations/export", json!({"id":clip.id,"path":path}))
                });
            }
        })
        .detach();
    }
    fn animation_panel(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let clip = self
            .animations
            .iter()
            .find(|a| Some(&a.id) == self.animation_id.as_ref());
        let selected = self
            .assets
            .iter()
            .find(|a| Some(&a.id) == self.selected.as_ref())
            .or_else(|| {
                self.animation_atlas
                    .as_ref()
                    .filter(|a| Some(&a.id) == self.selected.as_ref())
            });
        let preview = if self.show_atlas {
            selected.map(|a| a.path.clone())
        } else {
            clip.and_then(|a| {
                a.frames
                    .get(self.frame_index.min(a.frames.len().saturating_sub(1)))
            })
            .map(|f| f.path.clone())
        };
        let mut motions = div().flex().gap_1().flex_wrap();
        for motion in [
            Motion::Idle,
            Motion::Walk,
            Motion::Run,
            Motion::Jump,
            Motion::Attack,
            Motion::Custom,
        ] {
            motions = motions.child(
                Button::new(forge_core::animation::motion_label(motion))
                    .label(forge_core::animation::motion_label(motion))
                    .small()
                    .ghost()
                    .when(self.animation_config.motion == motion, |b| {
                        b.bg(rgb(0xe7ece2))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.reset_motion(motion, window, cx);
                        this.animation_id = None;
                        cx.notify();
                    })),
            );
        }
        let mut cast = div().flex().gap_1().flex_wrap();
        for c in &self.characters {
            let id = c.id.clone();
            cast = cast.child(
                Button::new(SharedString::from(format!("anim-char-{id}")))
                    .label(c.name.clone())
                    .small()
                    .ghost()
                    .when(self.character.as_ref() == Some(&id), |b| {
                        b.bg(rgb(0xe7ece2))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.character = Some(id.clone());
                        cx.notify();
                    })),
            );
        }
        let mut sizes = div().flex().items_center().gap_1().child(label("CELL"));
        for size in [32, 64, 128, 256, 512] {
            sizes = sizes.child(
                Button::new(SharedString::from(format!("cell-{size}")))
                    .label(format!("{size}"))
                    .xsmall()
                    .ghost()
                    .when(
                        self.animation_config.frame_width == size
                            && self.animation_config.frame_height == size,
                        |b| b.bg(rgb(0xe7ece2)),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.animation_config.frame_width = size;
                        this.animation_config.frame_height = size;
                        this.sync_grid_inputs(window, cx);
                        cx.notify();
                    })),
            );
        }
        let mut controls = div().flex().gap_3().items_center().flex_wrap();
        for (key, title, current, minimum, maximum) in [
            ("frames", "FRAMES", self.animation_config.frame_count, 2, 16),
            ("columns", "COLUMNS", self.animation_config.columns, 1, 8),
            ("fps", "FPS", self.animation_config.fps, 1, 60),
        ] {
            if key != "fps" && !self.animation_advanced {
                continue;
            }
            let update = move |this: &mut Self, delta: i32| {
                let next = (current as i32 + delta).clamp(minimum, maximum) as u32;
                match key {
                    "frames" => {
                        this.animation_config.frame_count = next;
                        this.animation_config.columns = this.animation_config.columns.min(next);
                    }
                    "columns" => {
                        this.animation_config.columns = next.min(this.animation_config.frame_count)
                    }
                    _ => this.animation_config.fps = next,
                }
            };
            controls = controls.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(label(title))
                    .child(
                        Button::new(SharedString::from(format!("{key}-less")))
                            .label("−")
                            .xsmall()
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                update(this, -1);
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .w(px(22.))
                            .text_center()
                            .text_sm()
                            .child(current.to_string()),
                    )
                    .child(
                        Button::new(SharedString::from(format!("{key}-more")))
                            .label("+")
                            .xsmall()
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                update(this, 1);
                                cx.notify();
                            })),
                    ),
            );
        }
        controls = controls.child(
            Button::new("anim-loop")
                .label(if self.animation_config.is_looping {
                    "✓ Loop"
                } else {
                    "Play once"
                })
                .small()
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.animation_config.is_looping = !this.animation_config.is_looping;
                    cx.notify();
                })),
        );
        let mut clips = div()
            .id("clips")
            .flex()
            .gap_2()
            .overflow_x_scroll()
            .flex_shrink_0();
        for a in &self.animations {
            let a = a.clone();
            clips = clips.child(
                Button::new(SharedString::from(format!("clip-{}", a.id)))
                    .label(format!(
                        "{} · {}f{}",
                        a.config.name,
                        a.config.frame_count,
                        if a.status == JobStatus::Succeeded {
                            ""
                        } else {
                            " · pending"
                        }
                    ))
                    .small()
                    .ghost()
                    .when(self.animation_id.as_ref() == Some(&a.id), |b| {
                        b.bg(rgb(0xe7ece2))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_animation(a.clone(), window, cx)
                    })),
            );
        }
        let mut viewer = div()
            .flex_1()
            .min_h(px(180.))
            .rounded_lg()
            .bg(rgb(0xe9eae1))
            .border_1()
            .border_color(rgb(LINE))
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center();
        if let Some(path) = preview {
            viewer = viewer.child(
                img(PathBuf::from(path))
                    .size_full()
                    .object_fit(ObjectFit::Contain),
            );
        } else {
            viewer=viewer.child(div().flex().flex_col().gap_2().items_center().p_5().child(div().font_family("Lora").text_size(px(30.)).child("Give your character a little life."))
            .child(div().text_color(rgb(MUTED)).text_center().max_w(px(390.)).child("Choose a motion and generate a clip, or import a sprite sheet and set up its frames.")));
        }
        let ready = clip.is_some_and(|a| a.status == JobStatus::Succeeded);
        let playback = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("play")
                            .label(if self.playing { "Pause" } else { "Play →" })
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
                                if let Some(clip) = this
                                    .animations
                                    .iter()
                                    .find(|a| Some(&a.id) == this.animation_id.as_ref())
                                    && !clip.frames.is_empty()
                                {
                                    this.playing = false;
                                    this.frame_index = (this.frame_index + 1) % clip.frames.len();
                                }
                                cx.notify();
                            })),
                    )
                    .child(label(
                        clip.map(|a| {
                            format!(
                                "{} / {} · {} FPS",
                                self.frame_index + 1,
                                a.config.frame_count,
                                a.config.fps
                            )
                        })
                        .unwrap_or_else(|| "ROW MAJOR / TRANSPARENT PNG".into()),
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        Button::new("atlas-view")
                            .label(if self.show_atlas { "Playback" } else { "Atlas" })
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_atlas = !this.show_atlas;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("export-animation")
                            .label("Export clip ↗")
                            .small()
                            .primary()
                            .disabled(!ready)
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.export_animation(window, cx)
                                }),
                            ),
                    ),
            );
        let mut settings=div().flex().flex_col().gap_2().p_4().bg(rgb(0xffffff)).rounded_lg().border_1().border_color(rgb(LINE))
            .child(motions).child(cast).child(controls)
            .child(label(format!("{} frames · {} columns · {} × {} px", self.animation_config.frame_count, self.animation_config.columns, self.animation_config.frame_width, self.animation_config.frame_height)))
            .when(self.animation_advanced, |d| d.child(sizes))
            .child(div().flex().items_end().gap_2().child(Input::new(&self.animation_brief).h(px(58.)).flex_1())
                .child(Button::new("generate-animation").label(if self.busy(){"Rendering…"}else{"Generate clip →"}).primary().disabled(self.busy()||self.character.is_none()||!self.account.as_ref().is_some_and(|a|a.can_generate_images))
                    .on_click(cx.listener(|this,_,_,cx|this.generate_animation(cx)))))
            .child(self.reference_controls(cx))
            .child(div().flex().gap_2().flex_wrap()
                .child(Button::new("grid-options").label(if self.animation_advanced {"Hide clip settings"}else{"Clip settings"}).xsmall().ghost().on_click(cx.listener(|this,_,_,cx|{this.animation_advanced= !this.animation_advanced;cx.notify();})))
                .child(Button::new("import-sheet").label("+ Import sheet").xsmall().ghost().disabled(self.project.is_none()).on_click(cx.listener(|this,_,window,cx|this.import(true,window,cx))))
                .child(Button::new("fit-sheet").label("Fit selected sheet").xsmall().ghost().disabled(selected.is_none()).on_click(cx.listener(|this,_,window,cx|this.fit_sheet(window,cx))))
                .child(Button::new("extract-frames").label("Extract frames").xsmall().ghost().disabled(selected.is_none()||self.busy()).on_click(cx.listener(|this,_,_,cx|this.setup_animation(cx))))
                .child(Button::new("save-timing").label("Save timing").xsmall().ghost().disabled(!ready).on_click(cx.listener(|this,_,_,_|{
                    if let Some(id)=&this.animation_id {this.send("animations/timing/update",json!({"id":id,"fps":this.animation_config.fps,"isLooping":this.animation_config.is_looping}));}}))));
        if self.animation_advanced {
            settings = settings.child(
                div()
                    .flex()
                    .gap_3()
                    .child(section("CELL WIDTH", &self.frame_width_input, "").flex_1())
                    .child(section("CELL HEIGHT", &self.frame_height_input, "").flex_1())
                    .child(section("MARGIN", &self.margin_input, "").flex_1())
                    .child(section("SPACING", &self.spacing_input, "").flex_1()),
            );
        }
        let mut sheets = div()
            .id("sheet-assets")
            .flex()
            .gap_1()
            .overflow_x_scroll()
            .flex_shrink_0();
        for a in &self.assets {
            if a.kind == AssetKind::SpriteSheet {
                let id = a.id.clone();
                sheets = sheets.child(
                    Button::new(SharedString::from(format!("sheet-{id}")))
                        .label(format!("{} · {}×{}", a.name, a.width, a.height))
                        .xsmall()
                        .ghost()
                        .when(self.selected.as_ref() == Some(&id), |b| b.bg(rgb(0xe7ece2)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selected = Some(id.clone());
                            this.show_atlas = true;
                            cx.notify();
                        })),
                );
            }
        }
        div().id("animation-workspace").flex_1().min_h_0().p_5().flex().flex_col().gap_3().overflow_y_scroll()
            .child(div().flex().justify_between().items_center().child(label("SPRITE WORKSHOP"))
                .child(Button::new("ask-animation").label("Let Forge animate →").small().ghost().disabled(self.guide_busy()||self.characters.is_empty()).on_click(cx.listener(|this,_,_,cx|{let name=this.characters.iter().find(|c|Some(&c.id)==this.character.as_ref()).map(|c|c.name.as_str()).unwrap_or("the first saved character");this.ask_guide(format!("Generate a six-frame walk cycle for {name}, keeping the saved style and identity."),true,cx)}))))
            .child(clips).child(viewer).child(playback).child(settings).child(sheets)
            .when(self.busy(),|d|d.child(Button::new("cancel-animation").label("Cancel rendering").small().on_click(cx.listener(|this,_,_,_|{if let Some(job)=&this.job {this.send("jobs/cancel",json!({"id":job.id}));}}))))
    }
    fn ask_guide(&mut self, message: String, allow_generation: bool, cx: &mut Context<Self>) {
        if self.guide_busy() || message.trim().is_empty() {
            return;
        }
        self.guide_stream.clear();
        self.guide_actions.clear();
        self.guide_submitting = true;
        self.send("assistant/message",json!({"requestId":uuid::Uuid::new_v4().to_string(),"sessionId":self.guide.as_ref().map(|g|&g.id),"projectId":self.project_id(),"message":message,"allowGeneration":allow_generation}));
        cx.notify();
    }
    fn apply_preset(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.presets.iter().find(|p| p.id == id).cloned() {
            self.preset = p.id;
            set(&self.style_name, p.style.name, window, cx);
            set(&self.direction, p.style.description, window, cx);
            set(&self.palette, p.style.palette.join(", "), window, cx);
            set(&self.perspective, p.style.perspective, window, cx);
            set(&self.lighting, p.style.lighting, window, cx);
            if self.kind == AssetKind::Scene {
                self.width = p.scene_width;
                self.height = p.scene_height;
            } else {
                self.width = p.character_size;
                self.height = p.character_size;
            }
        }
    }
    fn create_panel(&self, cx: &mut Context<Self>) -> Div {
        let mut kinds = div().flex().gap_1();
        for (kind, name) in [
            (AssetKind::Character, "Character"),
            (AssetKind::Scene, "Scene"),
            (AssetKind::Prop, "Prop"),
            (AssetKind::SpriteSheet, "Sprite sheet"),
        ] {
            kinds = kinds.child(
                Button::new(name)
                    .label(name)
                    .small()
                    .ghost()
                    .when(self.kind == kind, |b| b.bg(rgb(0xe5ebe1)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if kind == AssetKind::SpriteSheet {
                            this.tab = Tab::Animate;
                            cx.notify();
                            return;
                        }
                        this.kind = kind;
                        this.transparent = kind != AssetKind::Scene;
                        (this.width, this.height) = this
                            .project
                            .as_ref()
                            .map(|p| forge_core::presets::output_size(&p.style, kind))
                            .unwrap_or_else(|| {
                                forge_core::presets::output_size(&StyleGuide::default(), kind)
                            });
                        cx.notify();
                    })),
            );
        }
        let mut chars = div().flex().flex_wrap().gap_1();
        for c in &self.characters {
            let id = c.id.clone();
            chars = chars.child(
                Button::new(SharedString::from(format!("pick-{}", c.id)))
                    .label(c.name.clone())
                    .xsmall()
                    .when(self.character.as_ref() == Some(&c.id), |b| b.primary())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.character = Some(id.clone());
                        cx.notify();
                    })),
            );
        }
        chars = chars.child(
            Button::new("no-character")
                .label("No identity")
                .xsmall()
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.character = None;
                    cx.notify();
                })),
        );
        div()
            .p_4()
            .bg(rgb(0xffffff))
            .rounded_lg()
            .border_1()
            .border_color(rgb(LINE))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(kinds)
                    .child(label(format!(
                        "{} × {} · {}",
                        self.width,
                        self.height,
                        if self.transparent {
                            "transparent"
                        } else {
                            "opaque"
                        }
                    ))),
            )
            .when(!self.characters.is_empty(), |d| d.child(chars))
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap_3()
                    .child(Input::new(&self.brief).h(px(76.)).flex_1())
                    .child(
                        Button::new("generate")
                            .label(if self.busy() {
                                "Rendering…"
                            } else {
                                "Generate →"
                            })
                            .primary()
                            .h(px(38.))
                            .disabled(
                                self.busy()
                                    || self.project.is_none()
                                    || !self
                                        .account
                                        .as_ref()
                                        .is_some_and(|a| a.can_generate_images),
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.generate(cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_xs().text_color(rgb(MUTED)).child(format!(
                        "Saved style + character identity + {} extra references",
                        self.references.len()
                    )))
                    .child(
                        Button::new("output-settings")
                            .label(if self.advanced {
                                "Hide settings"
                            } else {
                                "Output settings"
                            })
                            .xsmall()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.advanced = !this.advanced;
                                cx.notify();
                            })),
                    ),
            )
            .child(self.reference_controls(cx))
            .when(self.advanced, |d| {
                d.child(
                    div()
                        .flex()
                        .gap_2()
                        .children(
                            [
                                (256, 256, "256²"),
                                (512, 512, "512²"),
                                (1024, 1024, "1024²"),
                                (1536, 1024, "Landscape"),
                            ]
                            .into_iter()
                            .map(|(w, h, name)| {
                                Button::new(name)
                                    .label(name)
                                    .xsmall()
                                    .when((self.width, self.height) == (w, h), |b| b.primary())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.width = w;
                                        this.height = h;
                                        cx.notify();
                                    }))
                            }),
                        )
                        .child(
                            Button::new("background")
                                .label(if self.transparent {
                                    "✓ Transparent"
                                } else {
                                    "Opaque"
                                })
                                .xsmall()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.transparent = !this.transparent;
                                    cx.notify();
                                })),
                        ),
                )
            })
    }
    fn style_panel(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut cards = div().flex().flex_wrap().gap_3().max_w(px(760.));
        for (index, p) in self.presets.iter().enumerate() {
            let id = p.id.clone();
            let active = self.preset == id;
            let colors = p
                .style
                .palette
                .iter()
                .filter_map(|c| u32::from_str_radix(c.trim_start_matches('#'), 16).ok())
                .collect::<Vec<_>>();
            let art = div()
                .h(px(93.))
                .w_full()
                .rounded_md()
                .bg(rgb(colors[2]))
                .overflow_hidden()
                .child(
                    img(self.previews[index].clone())
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                );
            cards = cards.child(
                div()
                    .id(SharedString::from(format!("preset-{id}")))
                    .w(px(208.))
                    .p_2()
                    .rounded_lg()
                    .border_2()
                    .border_color(rgb(if active { GREEN } else { LINE }))
                    .bg(rgb(0xffffff))
                    .cursor_pointer()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(art)
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(p.title.clone()),
                            )
                            .when(active, |d| d.child(div().text_color(rgb(GREEN)).child("✓"))),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(p.subtitle.clone()),
                    )
                    .child(
                        div().flex().gap_1().children(
                            colors
                                .into_iter()
                                .map(|color| div().size(px(12.)).rounded_full().bg(rgb(color))),
                        ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.apply_preset(&id, window, cx);
                        cx.notify();
                    })),
            );
        }
        div().id("style-page").flex_1().min_h_0().overflow_y_scroll().flex().flex_col().gap_4().p_6()
            .child(label("01 / ART DIRECTION"))
            .child(div().font_family("Lora").text_size(px(32.)).child("Every world has a language."))
            .child(div().text_color(rgb(MUTED)).child("Start with a visual direction. Forge will carry it through your cast and your scenery."))
            .when(self.project.is_none(),|d|d.child(Input::new(&self.project_name).w(px(300.))))
            .child(cards)
            .child(div().flex().items_center().gap_3().child(Button::new("save-style").label(if self.project.is_none(){"Start with this style →"}else{"Apply this direction →"}).primary().disabled(self.guide_busy()).on_click(cx.listener(|this,_,_,cx|this.save_style(cx))))
                .child(Button::new("customize").label(if self.advanced{"Hide details"}else{"Customize details"}).ghost().on_click(cx.listener(|this,_,_,cx|{this.advanced= !this.advanced;cx.notify();}))))
            .when(self.advanced,|d|d.child(div().flex().flex_col().gap_3().max_w(px(620.)).child(section("STYLE NAME",&self.style_name,"")).child(section("ART DIRECTION",&self.direction,"")).child(section("PALETTE",&self.palette,"Hex colors, separated by commas")).child(section("PERSPECTIVE",&self.perspective,"")).child(section("LIGHTING",&self.lighting,""))))
            .when(self.project.is_some(),|d|d.child(div().p_4().bg(rgb(0xe9ede4)).rounded_md().flex().flex_col().gap_2().child(label("YOUR SAVED DIRECTION")).child(self.project.as_ref().unwrap().style.description.clone()).child(div().text_xs().text_color(rgb(MUTED)).child(format!("{} pinned image references · Used by every new generation",self.project.as_ref().unwrap().style.reference_asset_ids.len())))))
    }
    fn characters_panel(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut cast = div().flex().flex_wrap().gap_3();
        for c in &self.characters {
            let id = c.id.clone();
            cast = cast.child(
                div()
                    .w(px(260.))
                    .p_4()
                    .bg(rgb(0xffffff))
                    .border_1()
                    .border_color(rgb(LINE))
                    .rounded_lg()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .font_family("Lora")
                            .text_size(px(23.))
                            .child(c.name.clone()),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child(c.description.clone()),
                    )
                    .child(label(format!(
                        "{} VISUAL REFERENCES",
                        c.reference_asset_ids.len()
                    )))
                    .child(
                        Button::new(SharedString::from(format!("use-{}", c.id)))
                            .label("Create with this character →")
                            .small()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.character = Some(id.clone());
                                this.tab = Tab::Create;
                                cx.notify();
                            })),
                    ),
            );
        }
        div().id("cast-page").flex_1().min_h_0().overflow_y_scroll().p_6().flex().flex_col().gap_4().child(label("02 / YOUR CAST")).child(div().font_family("Lora").text_size(px(32.)).child("Familiar faces. New adventures."))
            .child(div().text_color(rgb(MUTED)).child("Save the traits that make a character recognizable. Pin its best image to keep future poses consistent."))
            .child(cast)
            .child(Button::new("suggest-cast").label("Ask Forge to develop the cast →").primary().disabled(self.guide_busy()||self.project.is_none()).on_click(cx.listener(|this,_,_,cx|this.ask_guide("Help develop this game's cast. Add one fitting new character with a distinctive identity. Do not generate an image yet.".into(),false,cx))))
            .child(div().max_w(px(560.)).flex().flex_col().gap_3().child(label("ADD YOUR OWN CHARACTER")).child(Input::new(&self.character_name)).child(Input::new(&self.identity).h(px(85.))).child(Button::new("save-character").label("Save character").disabled(self.project.is_none()).on_click(cx.listener(|this,_,_,cx|{if let Some(project)=this.project_id(){this.send("characters/create",json!({"projectId":project,"name":value(&this.character_name,cx),"description":value(&this.identity,cx)}));}}))))
    }
    fn guide_panel(&self, cx: &mut Context<Self>) -> Div {
        let logged_in = self.account.as_ref().is_some_and(|a| a.is_logged_in);
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
            conversation=conversation.child(div().font_family("Lora").text_size(px(27.)).child("Let's build your world."))
                .child(div().text_sm().line_height(px(23.)).text_color(rgb(MUTED)).child("Tell me about your game. I'll shape its style, develop the cast, and help you make consistent assets."))
                .child(label("A PLACE TO START"));
            let suggestions = if self.project.is_some() {
                vec![
                    (
                        "Develop the cast",
                        "Develop this game's cast using its saved art direction. Add one fitting new character if needed; do not generate an image yet.",
                        false,
                    ),
                    (
                        "Make a first character",
                        "Generate the first character's idle pose using this project's saved style and identity. If the cast is empty, save one fitting character first. Make one transparent sprite.",
                        true,
                    ),
                    (
                        "Plan the next asset",
                        "Recommend one useful next asset for this game, based on what is already saved. Do not generate or change anything yet.",
                        false,
                    ),
                ]
            } else {
                vec![
                    (
                        "Cozy forest RPG",
                        "Set up a cozy forest RPG called Woodland, with a warm storybook style and a forest scout character. Save the style and character; do not generate images yet.",
                        false,
                    ),
                    (
                        "Pixel platformer",
                        "Set up a pixel-art platformer called Moonhop and create one memorable explorer character. Save the style and character; do not generate images yet.",
                        false,
                    ),
                    (
                        "Hand-drawn adventure",
                        "Set up a hand-drawn seaside adventure called Tidebook with a sketchbook style and a young courier character. Save the style and character; do not generate images yet.",
                        false,
                    ),
                ]
            };
            for (title, prompt, permitted) in suggestions {
                conversation = conversation.child(
                    Button::new(title)
                        .label(format!("{title} ↗"))
                        .w_full()
                        .disabled(!logged_in || self.guide_busy())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.ask_guide(prompt.into(), permitted, cx)
                        })),
                );
            }
        }
        if let Some(guide) = &self.guide {
            for (i, message) in guide.messages.iter().enumerate() {
                let user = message.role == "USER";
                conversation = conversation.child(
                    div()
                        .id(SharedString::from(format!("msg-{i}")))
                        .flex()
                        .flex_col()
                        .gap_2()
                        .p_3()
                        .rounded_lg()
                        .when(user, |d| d.bg(rgb(0xe7ece2)))
                        .child(label(if user { "YOU" } else { "FORGE" }))
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
                            "Shaping your next step…".to_string()
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
        div()
            .w(px(334.))
            .flex_shrink_0()
            .h_full()
            .border_l_1()
            .border_color(rgb(LINE))
            .bg(rgb(0xf1f2e9))
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(62.))
                    .flex_shrink_0()
                    .px_5()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .size(px(27.))
                                    .rounded_full()
                                    .bg(rgb(GREEN))
                                    .text_color(rgb(0xffffff))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child("✦"),
                            )
                            .child(div().font_weight(FontWeight::SEMIBOLD).child("Forge")),
                    )
                    .child(label("YOUR ART DIRECTOR")),
            )
            .child(conversation)
            .child(
                div()
                    .p_4()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        Input::new(&self.guide_input)
                            .h(px(88.))
                            .disabled(self.guide_busy()),
                    )
                    .child(
                        Button::new("guide-send")
                            .label(if self.guide_busy() {
                                "Working…"
                            } else {
                                "Send to Forge →"
                            })
                            .primary()
                            .w_full()
                            .disabled(!logged_in || self.guide_busy())
                            .on_click(cx.listener(|this, _, _, cx| {
                                let message = value(&this.guide_input, cx);
                                this.ask_guide(message, true, cx);
                            })),
                    )
                    .when(self.guide_busy(), |d| {
                        d.child(
                            Button::new("guide-stop")
                                .label("Stop guide")
                                .small()
                                .w_full()
                                .on_click(cx.listener(|this, _, _, _| {
                                    if let Some(g) = &this.guide {
                                        this.send("assistant/cancel", json!({"id":g.id}));
                                    }
                                })),
                        )
                    })
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(MUTED))
                            .child(if logged_in {
                                "Forge can update this project and render one asset when you ask."
                            } else {
                                "Connect Codex above to start your guided session."
                            }),
                    ),
            )
    }
}

impl Render for Studio {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self
            .assets
            .iter()
            .find(|a| Some(&a.id) == self.selected.as_ref())
            .cloned();
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
        let mut nav = div().flex().gap_1();
        for (tab, name) in [
            (Tab::Create, "Canvas"),
            (Tab::Style, "Art direction"),
            (Tab::Characters, "Cast"),
            (Tab::Animate, "Animate"),
        ] {
            nav = nav.child(
                Button::new(name)
                    .label(name)
                    .ghost()
                    .when(self.tab == tab, |b| b.bg(rgb(0xe7ece2)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tab = tab;
                        this.advanced = false;
                        cx.notify();
                    })),
            );
        }
        let header = div()
            .flex_shrink_0()
            .h(px(67.))
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
            .child(nav)
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        Button::new("account")
                            .label(account_label)
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.account.as_ref().is_some_and(|a| a.is_logged_in) {
                                    this.send("account/read", json!({}));
                                    this.message(
                                        "Connected to your Codex subscription.",
                                        false,
                                        cx,
                                    );
                                } else if let Some(login) = &this.login {
                                    let _ = open::that(&login.auth_url);
                                } else {
                                    this.send("account/login/start", json!({}));
                                }
                            })),
                    )
                    .child(
                        Button::new("account-refresh")
                            .label("↻")
                            .small()
                            .ghost()
                            .on_click(
                                cx.listener(|this, _, _, _| this.send("account/read", json!({}))),
                            ),
                    ),
            );
        let mut projects = div()
            .id("project-picker")
            .flex()
            .gap_1()
            .items_center()
            .overflow_x_scroll()
            .min_w_0()
            .flex_1();
        for project in &self.projects {
            let p = project.clone();
            projects = projects.child(
                Button::new(SharedString::from(p.id.clone()))
                    .label(p.name.clone())
                    .small()
                    .ghost()
                    .when(self.project.as_ref().is_some_and(|x| x.id == p.id), |b| {
                        b.bg(rgb(0xe5ebe1))
                    })
                    .disabled(self.guide_busy())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.guide = None;
                        this.choose_project(p.clone(), window, cx);
                        this.tab = Tab::Create;
                    })),
            );
        }
        projects = projects.child(
            Button::new("new-project")
                .label("+ New game")
                .small()
                .ghost()
                .disabled(self.guide_busy())
                .on_click(cx.listener(|this, _, window, cx| {
                    this.project = None;
                    this.guide = None;
                    this.assets.clear();
                    this.characters.clear();
                    this.character = None;
                    this.selected = None;
                    this.references.clear();
                    this.job = None;
                    this.guide_stream.clear();
                    this.guide_actions.clear();
                    this.asset_page = 1;
                    this.asset_total = 0;
                    this.tab = Tab::Style;
                    this.advanced = false;
                    set(&this.project_name, "My game", window, cx);
                    this.apply_preset("woodland", window, cx);
                    this.message(
                        "Pick a style, or let Forge help shape your game.",
                        false,
                        cx,
                    );
                })),
        );
        let projectbar = div()
            .px_6()
            .h(px(51.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_4()
            .border_b_1()
            .border_color(rgb(LINE))
            .child(projects)
            .when(self.project.is_some(), |d| {
                d.child(label(format!(
                    "{} · {} CHARACTERS",
                    self.project.as_ref().unwrap().style.name,
                    self.characters.len()
                )))
            });
        let mut canvas = div()
            .relative()
            .flex_1()
            .min_h(px(160.))
            .rounded_lg()
            .bg(rgb(0xe9eae1))
            .border_1()
            .border_color(rgb(LINE))
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center();
        if let Some(asset) = &selected {
            canvas = canvas.child(
                img(PathBuf::from(&asset.path))
                    .size_full()
                    .object_fit(ObjectFit::Contain),
            );
        } else {
            canvas=canvas.child(div().flex().flex_col().items_center().gap_3().p_5().child(div().text_color(rgb(GREEN)).text_size(px(34.)).child("✦")).child(div().font_family("Lora").text_size(px(30.)).child(if self.busy(){"Bringing your world to life…"}else{"The first piece of your world."})).child(div().text_color(rgb(MUTED)).text_center().max_w(px(400.)).child(if self.project.is_some(){"Describe an asset below, or ask Forge to create your first character."}else{"Choose an art direction or tell Forge about your game."})));
        }
        let mut canvas_tools = div().flex().items_center().justify_between().gap_2().child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div().truncate().font_weight(FontWeight::SEMIBOLD).child(
                        selected
                            .as_ref()
                            .map(|a| a.name.clone())
                            .unwrap_or_else(|| "Your canvas".into()),
                    ),
                )
                .child(label(
                    selected
                        .as_ref()
                        .map(|a| {
                            format!(
                                "{} × {} · PNG · {}",
                                a.width,
                                a.height,
                                if a.has_alpha { "ALPHA" } else { "OPAQUE" }
                            )
                        })
                        .unwrap_or_else(|| "CREATE / REVIEW / KEEP WHAT WORKS".into()),
                )),
        );
        if selected.is_some() {
            let pinned = self.project.as_ref().is_some_and(|p| {
                self.selected
                    .as_ref()
                    .is_some_and(|id| p.style.reference_asset_ids.contains(id))
            });
            canvas_tools = canvas_tools.child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        Button::new("style-ref")
                            .label(if pinned { "✓ Style ref" } else { "Style ref" })
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| this.style_reference(cx))),
                    )
                    .child(
                        Button::new("character-ref")
                            .label("Character ref")
                            .small()
                            .disabled(self.character.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.character_reference(cx))),
                    )
                    .child(
                        Button::new("export")
                            .label("Export PNG ↗")
                            .small()
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| this.export(window, cx))),
                    ),
            );
        }
        let mut library = div()
            .id("library")
            .flex()
            .gap_2()
            .overflow_x_scroll()
            .h(px(116.))
            .flex_shrink_0();
        for asset in &self.assets {
            let id = asset.id.clone();
            let active = self.selected.as_ref() == Some(&id);
            let reference = self.references.contains(&id);
            library = library.child(
                div()
                    .id(SharedString::from(asset.id.clone()))
                    .w(px(91.))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .id(SharedString::from(format!("thumb-{}", asset.id)))
                            .h(px(79.))
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
                                this.selected = Some(id.clone());
                                cx.notify();
                            })),
                    )
                    .child({
                        let id = asset.id.clone();
                        Button::new(SharedString::from(format!("ref-{id}")))
                            .label(if reference {
                                "✓ Reference"
                            } else {
                                "+ Reference"
                            })
                            .xsmall()
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.references.contains(&id) {
                                    this.references.retain(|r| r != &id);
                                } else {
                                    this.references.push(id.clone());
                                }
                                cx.notify();
                            }))
                    }),
            );
        }
        let library_header = div()
            .flex()
            .items_center()
            .justify_between()
            .child(label(format!("YOUR ASSETS / {}", self.asset_total)))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        Button::new("prev-page")
                            .label("←")
                            .xsmall()
                            .ghost()
                            .disabled(self.asset_page == 1)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.asset_page = this.asset_page.saturating_sub(1).max(1);
                                this.refresh();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("next-page")
                            .label("→")
                            .xsmall()
                            .ghost()
                            .disabled(self.asset_page * 24 >= self.asset_total)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.asset_page += 1;
                                this.refresh();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("import")
                            .label("+ Import reference")
                            .xsmall()
                            .ghost()
                            .disabled(self.project.is_none())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.import(false, window, cx)),
                            ),
                    ),
            );
        let create = div()
            .id("create-workspace")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_5()
            .flex()
            .flex_col()
            .gap_3()
            .child(canvas_tools)
            .child(canvas)
            .child(self.create_panel(cx))
            .child(library_header)
            .when(!self.assets.is_empty(), |d| d.child(library))
            .when(self.busy(), |d| {
                d.child(
                    Button::new("cancel-render")
                        .label("Cancel rendering")
                        .small()
                        .on_click(cx.listener(|this, _, _, _| {
                            if let Some(job) = &this.job {
                                this.send("jobs/cancel", json!({"id":job.id}));
                            }
                        })),
                )
            });
        let workspace = match self.tab {
            Tab::Create => create.into_any_element(),
            Tab::Style => self.style_panel(cx).into_any_element(),
            Tab::Characters => self.characters_panel(cx).into_any_element(),
            Tab::Animate => self.animation_panel(cx).into_any_element(),
        };
        let login_banner = div()
            .px_6()
            .py_2()
            .bg(rgb(0xe4ebdf))
            .flex()
            .items_center()
            .justify_between()
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
            .h(px(35.))
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
            .child(self.status.clone());
        div()
            .size_full()
            .font_family("IBM Plex Sans")
            .text_color(rgb(INK))
            .bg(rgb(PAPER))
            .flex()
            .flex_col()
            .child(header)
            .when(self.login.is_some(), |d| d.child(login_banner))
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
