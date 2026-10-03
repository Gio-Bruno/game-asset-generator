//! Public v1 contract shared by the CLI, stdio API and native desktop client.
use crate::error::{ApiError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StyleGuide {
    #[serde(default)]
    pub preset_id: Option<String>,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub palette: Vec<String>,
    #[serde(default)]
    pub perspective: String,
    #[serde(default)]
    pub lighting: String,
    #[serde(default)]
    pub reference_asset_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub style: StyleGuide,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateProject {
    pub name: String,
    pub style: StyleGuide,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Character {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub description: String,
    /// Existing character records and clients remain characters when omitted.
    #[serde(default)]
    pub kind: SubjectKind,
    pub reference_asset_ids: Vec<String>,
}

/// Reusable identities share reference and generation behavior in the catalog.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SubjectKind {
    #[default]
    Character,
    Structure,
    Prop,
}
impl SubjectKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Character => "Character",
            Self::Structure => "Structure",
            Self::Prop => "Prop",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCharacter {
    pub project_id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub kind: SubjectKind,
    #[serde(default)]
    pub reference_asset_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateCharacter {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub kind: Option<SubjectKind>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssetKind {
    #[default]
    Character,
    Scene,
    Prop,
    SpriteSheet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenerateInput {
    pub project_id: String,
    pub idempotency_key: String,
    pub prompt: String,
    #[serde(default)]
    pub kind: AssetKind,
    #[serde(default)]
    pub character_id: Option<String>,
    #[serde(default)]
    pub reference_asset_ids: Vec<String>,
    #[serde(default = "default_size")]
    pub width: u32,
    #[serde(default = "default_size")]
    pub height: u32,
    #[serde(default)]
    pub transparent_background: bool,
    /// Omitted for static assets to preserve existing idempotency hashes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<AnimationConfig>,
}
fn default_size() -> u32 {
    1024
}

impl GenerateInput {
    pub fn validate(&self) -> Result<()> {
        nonempty("prompt", &self.prompt, 8000)?;
        nonempty("idempotencyKey", &self.idempotency_key, 128)?;
        for dimension in [self.width, self.height] {
            let minimum = if self.animation.is_some() { 16 } else { 64 };
            if !(minimum..=4096).contains(&dimension) {
                return Err(ApiError::validation(
                    "Image dimensions must be between 64 and 4096 pixels.",
                ));
            }
        }
        if self.reference_asset_ids.len() > 8 {
            return Err(ApiError::validation("Use at most 8 reference images."));
        }
        if let Some(config) = &self.animation {
            config.validate()?;
            if self.kind != AssetKind::SpriteSheet
                || self.character_id.is_none()
                || !self.transparent_background
            {
                return Err(ApiError::validation(
                    "Animations require a saved character, SPRITE_SHEET kind and a transparent background.",
                ));
            }
            if config.margin != 0
                || config.spacing != 0
                || config.atlas_size() != (self.width, self.height)
            {
                return Err(ApiError::validation(
                    "Generated animations require an unpadded grid matching the requested atlas dimensions.",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Motion {
    #[default]
    Idle,
    Walk,
    Run,
    Jump,
    Attack,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnimationConfig {
    pub name: String,
    #[serde(default)]
    pub motion: Motion,
    #[serde(default = "six_frames")]
    pub frame_count: u32,
    #[serde(default = "three_columns")]
    pub columns: u32,
    #[serde(default = "cell_size")]
    pub frame_width: u32,
    #[serde(default = "cell_size")]
    pub frame_height: u32,
    #[serde(default = "eight_fps")]
    pub fps: u32,
    #[serde(default = "looping")]
    pub is_looping: bool,
    #[serde(default)]
    pub margin: u32,
    #[serde(default)]
    pub spacing: u32,
}
fn six_frames() -> u32 {
    6
}
fn three_columns() -> u32 {
    3
}
fn cell_size() -> u32 {
    256
}
fn eight_fps() -> u32 {
    8
}
fn looping() -> bool {
    true
}
impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            name: "Idle".into(),
            motion: Motion::Idle,
            frame_count: 6,
            columns: 3,
            frame_width: 256,
            frame_height: 256,
            fps: 8,
            is_looping: true,
            margin: 0,
            spacing: 0,
        }
    }
}
impl AnimationConfig {
    pub fn rows(&self) -> u32 {
        self.frame_count.div_ceil(self.columns.max(1))
    }
    pub fn atlas_size(&self) -> (u32, u32) {
        let rows = self.rows();
        (
            2 * self.margin
                + self.columns * self.frame_width
                + self.columns.saturating_sub(1) * self.spacing,
            2 * self.margin + rows * self.frame_height + rows.saturating_sub(1) * self.spacing,
        )
    }
    pub fn validate(&self) -> Result<()> {
        nonempty("name", &self.name, 120)?;
        if !(2..=16).contains(&self.frame_count)
            || !(1..=8).contains(&self.columns)
            || self.columns > self.frame_count
            || !(16..=1024).contains(&self.frame_width)
            || !(16..=1024).contains(&self.frame_height)
            || !(1..=60).contains(&self.fps)
            || self.margin > 128
            || self.spacing > 64
        {
            return Err(ApiError::validation(
                "Use 2–16 frames, 1–8 columns, 16–1024 pixel cells, 1–60 FPS, margin ≤128 and spacing ≤64.",
            ));
        }
        let (width, height) = self.atlas_size();
        if width > 4096 || height > 4096 {
            return Err(ApiError::validation(
                "The complete atlas must fit within 4096 × 4096 pixels.",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateAnimation {
    pub project_id: String,
    pub character_id: String,
    pub idempotency_key: String,
    pub config: AnimationConfig,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub reference_asset_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetupAnimation {
    pub project_id: String,
    pub asset_id: String,
    #[serde(default)]
    pub character_id: Option<String>,
    pub idempotency_key: String,
    pub config: AnimationConfig,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationFrame {
    pub index: u32,
    pub path: String,
    pub rect: FrameRect,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Animation {
    pub id: String,
    pub project_id: String,
    pub character_id: Option<String>,
    pub job_id: Option<String>,
    pub source_asset_id: Option<String>,
    pub config: AnimationConfig,
    pub status: JobStatus,
    pub frames: Vec<AnimationFrame>,
    pub preview_path: Option<String>,
    pub error: Option<ApiError>,
    pub created_at: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateAnimationTiming {
    pub id: String,
    pub fps: u32,
    pub is_looping: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Unknown,
}
impl JobStatus {
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Queued | Self::Running)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub project_id: String,
    pub status: JobStatus,
    pub request: GenerateInput,
    pub style_snapshot: StyleGuide,
    pub character_snapshot: Option<Character>,
    pub reference_asset_ids: Vec<String>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub asset_ids: Vec<String>,
    pub error: Option<ApiError>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub project_id: String,
    pub job_id: Option<String>,
    pub character_id: Option<String>,
    pub kind: AssetKind,
    pub name: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub data: Vec<T>,
    pub pagination: Pagination,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    pub page: usize,
    pub page_size: usize,
    pub total_items: usize,
    pub total_pages: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListInput {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default = "first_page")]
    pub page: usize,
    #[serde(default = "page_size")]
    pub page_size: usize,
}
fn first_page() -> usize {
    1
}
fn page_size() -> usize {
    50
}
impl Default for ListInput {
    fn default() -> Self {
        Self {
            project_id: None,
            page: 1,
            page_size: 50,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    pub is_logged_in: bool,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub can_generate_images: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Login {
    pub login_id: String,
    pub auth_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub kind: String,
    pub job_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StylePreset {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub style: StyleGuide,
    pub character_size: u32,
    pub scene_width: u32,
    pub scene_height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssistantInput {
    pub request_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    pub message: String,
    #[serde(default)]
    pub allow_generation: bool,
    /// Visual context for this message only. Empty input preserves legacy replay hashes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference_asset_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssistantStatus {
    Ready,
    Thinking,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference_asset_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSession {
    pub id: String,
    pub project_id: Option<String>,
    pub status: AssistantStatus,
    pub messages: Vec<ChatMessage>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub allow_generation: bool,
    #[serde(default)]
    pub reference_asset_ids: Vec<String>,
    pub generated_job_ids: Vec<String>,
    #[serde(default)]
    pub turn_job_count: u32,
    pub error: Option<ApiError>,
    pub created_at: u64,
}

pub fn nonempty(name: &str, value: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > max {
        return Err(ApiError::validation(format!(
            "{name} must contain 1–{max} characters."
        )));
    }
    Ok(())
}

pub fn validate_style(style: &StyleGuide) -> Result<()> {
    if let Some(id) = &style.preset_id {
        crate::presets::get(id)?;
    }
    nonempty("style.name", &style.name, 120)?;
    nonempty("style.description", &style.description, 8000)?;
    if style.palette.len() > 16
        || style.palette.iter().any(|v| {
            v.len() != 7 || !v.starts_with('#') || !v[1..].chars().all(|c| c.is_ascii_hexdigit())
        })
    {
        return Err(ApiError::validation(
            "Palette must contain at most 16 colors in #RRGGBB format.",
        ));
    }
    if style.reference_asset_ids.len() > 8 {
        return Err(ApiError::validation("Use at most 8 style references."));
    }
    Ok(())
}
