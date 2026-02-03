use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub database: DatabaseConfig,

    #[serde(default)]
    pub llm: LlmConfig,

    #[serde(default)]
    pub scanner: ScannerConfig,

    #[serde(default)]
    pub preview: PreviewConfig,

    #[serde(default)]
    pub trash: TrashConfig,

    #[serde(default)]
    pub thumbnails: ThumbnailConfig,

    #[serde(default)]
    pub schedule: ScheduleConfig,

    #[serde(default)]
    pub library: LibraryConfig,

    #[serde(default)]
    pub keybindings: KeyBindings,

    #[serde(default)]
    pub view: ViewConfig,
}

/// View filter settings (persisted across sessions)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ViewConfig {
    /// Show hidden files/directories (starting with .)
    #[serde(default)]
    pub show_hidden: bool,

    /// Show all files, not just supported image formats
    #[serde(default)]
    pub show_all_files: bool,
}

/// Database backend type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    #[default]
    Sqlite,
    Postgresql,
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database backend type (sqlite or postgresql)
    #[serde(default)]
    pub backend: DatabaseType,

    /// SQLite database path (used when backend = sqlite)
    #[serde(default = "default_db_path")]
    pub sqlite_path: PathBuf,

    /// PostgreSQL connection string (used when backend = postgresql)
    /// Example: "postgresql://user:password@localhost:5432/clepho"
    #[serde(default)]
    pub postgresql_url: Option<String>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            backend: DatabaseType::default(),
            sqlite_path: default_db_path(),
            postgresql_url: None,
        }
    }
}

impl Config {
    /// Get the database path for backwards compatibility
    /// This returns the SQLite path, which is the default.
    pub fn db_path(&self) -> &PathBuf {
        &self.database.sqlite_path
    }
}

/// Action that can be triggered by a keybinding
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    // Navigation
    MoveDown,
    MoveUp,
    GoParent,
    EnterSelected,
    GoToBottom,
    PageDown,
    PageUp,
    ScrollPreviewDown,
    ScrollPreviewUp,
    GoHome,

    // Selection
    ToggleSelection,
    EnterVisualMode,

    // Actions
    Scan,
    FindDuplicates,
    DescribeWithLlm,
    BatchLlm,
    DetectFaces,
    ClusterFaces,
    ClipEmbedding,
    ViewTasks,
    ViewTrash,
    MoveFiles,
    RenameFiles,
    ExportDatabase,
    SemanticSearch,
    ManagePeople,
    EditDescription,
    ViewChanges,
    OpenSchedule,
    OpenGallery,
    OpenTags,
    OpenSlideshow,
    CentraliseFiles,
    RotateCW,
    RotateCCW,
    YankFiles,
    PasteFiles,
    DeleteFiles,
    ShowHelp,
    Quit,
    // View filters
    ToggleHidden,
    ToggleShowAllFiles,
    OpenExternal,
}

/// A keybinding specification in config
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum KeySpec {
    /// Simple key like "j", "k", "Enter"
    Simple(String),
    /// Key with modifiers like "Ctrl+d"
    WithModifiers(String),
}

impl KeySpec {
    /// Parse a key specification string into KeyCode and KeyModifiers
    pub fn parse(&self) -> Option<(KeyCode, KeyModifiers)> {
        let s = match self {
            KeySpec::Simple(s) => s,
            KeySpec::WithModifiers(s) => s,
        };

        let parts: Vec<&str> = s.split('+').collect();
        let mut modifiers = KeyModifiers::empty();
        let key_part = parts.last()?;

        // Parse modifiers
        for part in &parts[..parts.len().saturating_sub(1)] {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
                "alt" => modifiers |= KeyModifiers::ALT,
                "shift" => modifiers |= KeyModifiers::SHIFT,
                _ => {}
            }
        }

        // Parse key
        let key = match key_part.to_lowercase().as_str() {
            "enter" | "return" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "space" => KeyCode::Char(' '),
            "tab" => KeyCode::Tab,
            "backspace" | "bs" => KeyCode::Backspace,
            "delete" | "del" => KeyCode::Delete,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" | "pgup" => KeyCode::PageUp,
            "pagedown" | "pgdn" => KeyCode::PageDown,
            "f1" => KeyCode::F(1),
            "f2" => KeyCode::F(2),
            "f3" => KeyCode::F(3),
            "f4" => KeyCode::F(4),
            "f5" => KeyCode::F(5),
            "f6" => KeyCode::F(6),
            "f7" => KeyCode::F(7),
            "f8" => KeyCode::F(8),
            "f9" => KeyCode::F(9),
            "f10" => KeyCode::F(10),
            "f11" => KeyCode::F(11),
            "f12" => KeyCode::F(12),
            s if s.len() == 1 => {
                // Use original character to preserve case
                let original_c = key_part.chars().next()?;
                // Uppercase letters: crossterm reports KeyCode::Char('G') WITH KeyModifiers::SHIFT
                // so we need to add SHIFT modifier to match the lookup
                if original_c.is_ascii_uppercase() && !modifiers.contains(KeyModifiers::SHIFT) {
                    modifiers |= KeyModifiers::SHIFT;
                }
                KeyCode::Char(original_c)
            }
            _ => return None,
        };

        Some((key, modifiers))
    }
}

/// Keybinding configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindings {
    // Navigation
    #[serde(default = "default_move_down")]
    pub move_down: Vec<KeySpec>,
    #[serde(default = "default_move_up")]
    pub move_up: Vec<KeySpec>,
    #[serde(default = "default_go_parent")]
    pub go_parent: Vec<KeySpec>,
    #[serde(default = "default_enter_selected")]
    pub enter_selected: Vec<KeySpec>,
    #[serde(default = "default_go_to_bottom")]
    pub go_to_bottom: Vec<KeySpec>,
    #[serde(default = "default_page_down")]
    pub page_down: Vec<KeySpec>,
    #[serde(default = "default_page_up")]
    pub page_up: Vec<KeySpec>,
    #[serde(default = "default_scroll_preview_down")]
    pub scroll_preview_down: Vec<KeySpec>,
    #[serde(default = "default_scroll_preview_up")]
    pub scroll_preview_up: Vec<KeySpec>,
    #[serde(default = "default_go_home")]
    pub go_home: Vec<KeySpec>,

    // Selection
    #[serde(default = "default_toggle_selection")]
    pub toggle_selection: Vec<KeySpec>,
    #[serde(default = "default_enter_visual_mode")]
    pub enter_visual_mode: Vec<KeySpec>,

    // Actions
    #[serde(default = "default_scan")]
    pub scan: Vec<KeySpec>,
    #[serde(default = "default_find_duplicates")]
    pub find_duplicates: Vec<KeySpec>,
    #[serde(default = "default_describe_with_llm")]
    pub describe_with_llm: Vec<KeySpec>,
    #[serde(default = "default_batch_llm")]
    pub batch_llm: Vec<KeySpec>,
    #[serde(default = "default_detect_faces")]
    pub detect_faces: Vec<KeySpec>,
    #[serde(default = "default_cluster_faces")]
    pub cluster_faces: Vec<KeySpec>,
    #[serde(default = "default_clip_embedding")]
    pub clip_embedding: Vec<KeySpec>,
    #[serde(default = "default_view_tasks")]
    pub view_tasks: Vec<KeySpec>,
    #[serde(default = "default_view_trash")]
    pub view_trash: Vec<KeySpec>,
    #[serde(default = "default_move_files")]
    pub move_files: Vec<KeySpec>,
    #[serde(default = "default_rename_files")]
    pub rename_files: Vec<KeySpec>,
    #[serde(default = "default_export_database")]
    pub export_database: Vec<KeySpec>,
    #[serde(default = "default_semantic_search")]
    pub semantic_search: Vec<KeySpec>,
    #[serde(default = "default_manage_people")]
    pub manage_people: Vec<KeySpec>,
    #[serde(default = "default_edit_description")]
    pub edit_description: Vec<KeySpec>,
    #[serde(default = "default_view_changes")]
    pub view_changes: Vec<KeySpec>,
    #[serde(default = "default_open_schedule")]
    pub open_schedule: Vec<KeySpec>,
    #[serde(default = "default_open_gallery")]
    pub open_gallery: Vec<KeySpec>,
    #[serde(default = "default_open_tags")]
    pub open_tags: Vec<KeySpec>,
    #[serde(default = "default_open_slideshow")]
    pub open_slideshow: Vec<KeySpec>,
    #[serde(default = "default_centralise_files")]
    pub centralise_files: Vec<KeySpec>,
    #[serde(default = "default_rotate_cw")]
    pub rotate_cw: Vec<KeySpec>,
    #[serde(default = "default_rotate_ccw")]
    pub rotate_ccw: Vec<KeySpec>,
    #[serde(default = "default_yank_files")]
    pub yank_files: Vec<KeySpec>,
    #[serde(default = "default_paste_files")]
    pub paste_files: Vec<KeySpec>,
    #[serde(default = "default_delete_files")]
    pub delete_files: Vec<KeySpec>,
    #[serde(default = "default_show_help")]
    pub show_help: Vec<KeySpec>,
    #[serde(default = "default_quit")]
    pub quit: Vec<KeySpec>,
    #[serde(default = "default_toggle_hidden")]
    pub toggle_hidden: Vec<KeySpec>,
    #[serde(default = "default_toggle_show_all_files")]
    pub toggle_show_all_files: Vec<KeySpec>,
    #[serde(default = "default_open_external")]
    pub open_external: Vec<KeySpec>,
}

// Default keybinding functions
fn default_move_down() -> Vec<KeySpec> { vec![KeySpec::Simple("j".into()), KeySpec::Simple("Down".into())] }
fn default_move_up() -> Vec<KeySpec> { vec![KeySpec::Simple("k".into()), KeySpec::Simple("Up".into())] }
fn default_go_parent() -> Vec<KeySpec> { vec![KeySpec::Simple("h".into()), KeySpec::Simple("Left".into()), KeySpec::Simple("Backspace".into())] }
fn default_enter_selected() -> Vec<KeySpec> { vec![KeySpec::Simple("l".into()), KeySpec::Simple("Right".into()), KeySpec::Simple("Enter".into())] }
fn default_go_to_bottom() -> Vec<KeySpec> { vec![KeySpec::Simple("G".into())] }
fn default_page_down() -> Vec<KeySpec> { vec![KeySpec::WithModifiers("Ctrl+f".into())] }
fn default_page_up() -> Vec<KeySpec> { vec![KeySpec::WithModifiers("Ctrl+b".into())] }
fn default_scroll_preview_down() -> Vec<KeySpec> { vec![KeySpec::Simple("}".into())] }
fn default_scroll_preview_up() -> Vec<KeySpec> { vec![KeySpec::Simple("{".into())] }
fn default_go_home() -> Vec<KeySpec> { vec![KeySpec::Simple("~".into())] }
fn default_toggle_selection() -> Vec<KeySpec> { vec![KeySpec::Simple("Space".into())] }
// Yazi-aligned: v = visual mode (V also works)
fn default_enter_visual_mode() -> Vec<KeySpec> { vec![KeySpec::Simple("v".into()), KeySpec::Simple("V".into())] }
fn default_scan() -> Vec<KeySpec> { vec![KeySpec::Simple("s".into())] }
// Clepho-specific: u = duplicates (d is trash in yazi)
fn default_find_duplicates() -> Vec<KeySpec> { vec![KeySpec::Simple("u".into())] }
// Clepho-specific: i = describe with LLM (info)
fn default_describe_with_llm() -> Vec<KeySpec> { vec![KeySpec::Simple("i".into())] }
fn default_batch_llm() -> Vec<KeySpec> { vec![KeySpec::Simple("I".into())] }
fn default_detect_faces() -> Vec<KeySpec> { vec![KeySpec::Simple("F".into())] }
fn default_cluster_faces() -> Vec<KeySpec> { vec![KeySpec::Simple("C".into())] }
fn default_clip_embedding() -> Vec<KeySpec> { vec![KeySpec::Simple("E".into())] }
fn default_view_tasks() -> Vec<KeySpec> { vec![KeySpec::Simple("T".into())] }
// Clepho-specific: X = view trash (t is tabs in yazi, we don't have tabs)
fn default_view_trash() -> Vec<KeySpec> { vec![KeySpec::Simple("X".into())] }
fn default_move_files() -> Vec<KeySpec> { vec![KeySpec::Simple("m".into())] }
// Yazi-aligned: r = rename (lowercase)
fn default_rename_files() -> Vec<KeySpec> { vec![KeySpec::Simple("r".into())] }
fn default_export_database() -> Vec<KeySpec> { vec![KeySpec::Simple("O".into())] }
fn default_semantic_search() -> Vec<KeySpec> { vec![KeySpec::Simple("/".into())] }
// Clepho-specific: P = manage people (p is paste in yazi)
fn default_manage_people() -> Vec<KeySpec> { vec![KeySpec::Simple("P".into())] }
fn default_edit_description() -> Vec<KeySpec> { vec![KeySpec::Simple("e".into())] }
fn default_view_changes() -> Vec<KeySpec> { vec![KeySpec::Simple("c".into())] }
fn default_open_schedule() -> Vec<KeySpec> { vec![KeySpec::Simple("@".into())] }
fn default_open_gallery() -> Vec<KeySpec> { vec![KeySpec::Simple("A".into())] }
fn default_open_tags() -> Vec<KeySpec> { vec![KeySpec::Simple("b".into())] }
// Clepho-specific: S = slideshow (v is now visual mode)
fn default_open_slideshow() -> Vec<KeySpec> { vec![KeySpec::Simple("S".into())] }
fn default_centralise_files() -> Vec<KeySpec> { vec![KeySpec::Simple("L".into())] }
fn default_rotate_cw() -> Vec<KeySpec> { vec![KeySpec::Simple("]".into())] }
fn default_rotate_ccw() -> Vec<KeySpec> { vec![KeySpec::Simple("[".into())] }
// Yazi-aligned: y = yank (copy), x = cut (we treat both as cut/move)
fn default_yank_files() -> Vec<KeySpec> { vec![KeySpec::Simple("y".into()), KeySpec::Simple("x".into())] }
// Yazi-aligned: p = paste
fn default_paste_files() -> Vec<KeySpec> { vec![KeySpec::Simple("p".into())] }
// Yazi-aligned: d = trash, D = permanent delete
fn default_delete_files() -> Vec<KeySpec> { vec![KeySpec::Simple("d".into()), KeySpec::Simple("Delete".into())] }
fn default_show_help() -> Vec<KeySpec> { vec![KeySpec::Simple("?".into())] }
fn default_quit() -> Vec<KeySpec> { vec![KeySpec::Simple("q".into())] }
// Yazi-aligned: . = toggle hidden files
fn default_toggle_hidden() -> Vec<KeySpec> { vec![KeySpec::Simple(".".into())] }
// Clepho-specific: H = show all files (not just images)
fn default_toggle_show_all_files() -> Vec<KeySpec> { vec![KeySpec::Simple("H".into())] }
fn default_open_external() -> Vec<KeySpec> { vec![KeySpec::Simple("o".into())] }

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            move_down: default_move_down(),
            move_up: default_move_up(),
            go_parent: default_go_parent(),
            enter_selected: default_enter_selected(),
            go_to_bottom: default_go_to_bottom(),
            page_down: default_page_down(),
            page_up: default_page_up(),
            scroll_preview_down: default_scroll_preview_down(),
            scroll_preview_up: default_scroll_preview_up(),
            go_home: default_go_home(),
            toggle_selection: default_toggle_selection(),
            enter_visual_mode: default_enter_visual_mode(),
            scan: default_scan(),
            find_duplicates: default_find_duplicates(),
            describe_with_llm: default_describe_with_llm(),
            batch_llm: default_batch_llm(),
            detect_faces: default_detect_faces(),
            cluster_faces: default_cluster_faces(),
            clip_embedding: default_clip_embedding(),
            view_tasks: default_view_tasks(),
            view_trash: default_view_trash(),
            move_files: default_move_files(),
            rename_files: default_rename_files(),
            export_database: default_export_database(),
            semantic_search: default_semantic_search(),
            manage_people: default_manage_people(),
            edit_description: default_edit_description(),
            view_changes: default_view_changes(),
            open_schedule: default_open_schedule(),
            open_gallery: default_open_gallery(),
            open_tags: default_open_tags(),
            open_slideshow: default_open_slideshow(),
            centralise_files: default_centralise_files(),
            rotate_cw: default_rotate_cw(),
            rotate_ccw: default_rotate_ccw(),
            yank_files: default_yank_files(),
            paste_files: default_paste_files(),
            delete_files: default_delete_files(),
            show_help: default_show_help(),
            quit: default_quit(),
            toggle_hidden: default_toggle_hidden(),
            toggle_show_all_files: default_toggle_show_all_files(),
            open_external: default_open_external(),
        }
    }
}

impl KeyBindings {
    /// Build a lookup map from (KeyCode, KeyModifiers) -> Action
    pub fn build_action_map(&self) -> HashMap<(KeyCode, KeyModifiers), Action> {
        let mut map = HashMap::new();

        let bindings: Vec<(&[KeySpec], Action)> = vec![
            (&self.move_down, Action::MoveDown),
            (&self.move_up, Action::MoveUp),
            (&self.go_parent, Action::GoParent),
            (&self.enter_selected, Action::EnterSelected),
            (&self.go_to_bottom, Action::GoToBottom),
            (&self.page_down, Action::PageDown),
            (&self.page_up, Action::PageUp),
            (&self.scroll_preview_down, Action::ScrollPreviewDown),
            (&self.scroll_preview_up, Action::ScrollPreviewUp),
            (&self.go_home, Action::GoHome),
            (&self.toggle_selection, Action::ToggleSelection),
            (&self.enter_visual_mode, Action::EnterVisualMode),
            (&self.scan, Action::Scan),
            (&self.find_duplicates, Action::FindDuplicates),
            (&self.describe_with_llm, Action::DescribeWithLlm),
            (&self.batch_llm, Action::BatchLlm),
            (&self.detect_faces, Action::DetectFaces),
            (&self.cluster_faces, Action::ClusterFaces),
            (&self.clip_embedding, Action::ClipEmbedding),
            (&self.view_tasks, Action::ViewTasks),
            (&self.view_trash, Action::ViewTrash),
            (&self.move_files, Action::MoveFiles),
            (&self.rename_files, Action::RenameFiles),
            (&self.export_database, Action::ExportDatabase),
            (&self.semantic_search, Action::SemanticSearch),
            (&self.manage_people, Action::ManagePeople),
            (&self.edit_description, Action::EditDescription),
            (&self.view_changes, Action::ViewChanges),
            (&self.open_schedule, Action::OpenSchedule),
            (&self.open_gallery, Action::OpenGallery),
            (&self.open_tags, Action::OpenTags),
            (&self.open_slideshow, Action::OpenSlideshow),
            (&self.centralise_files, Action::CentraliseFiles),
            (&self.rotate_cw, Action::RotateCW),
            (&self.rotate_ccw, Action::RotateCCW),
            (&self.yank_files, Action::YankFiles),
            (&self.paste_files, Action::PasteFiles),
            (&self.delete_files, Action::DeleteFiles),
            (&self.show_help, Action::ShowHelp),
            (&self.quit, Action::Quit),
            (&self.toggle_hidden, Action::ToggleHidden),
            (&self.toggle_show_all_files, Action::ToggleShowAllFiles),
            (&self.open_external, Action::OpenExternal),
        ];

        for (specs, action) in bindings {
            for spec in specs {
                if let Some((code, mods)) = spec.parse() {
                    map.insert((code, mods), action);
                }
            }
        }

        map
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashConfig {
    #[serde(default = "default_trash_path")]
    pub path: PathBuf,

    #[serde(default = "default_max_age_days")]
    pub max_age_days: u32,

    #[serde(default = "default_max_size_bytes")]
    pub max_size_bytes: u64,
}

fn default_trash_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from(".local/share"))
        .join("clepho/.trash")
}

fn default_max_age_days() -> u32 {
    30
}

fn default_max_size_bytes() -> u64 {
    1024 * 1024 * 1024 // 1GB
}

impl Default for TrashConfig {
    fn default() -> Self {
        Self {
            path: default_trash_path(),
            max_age_days: default_max_age_days(),
            max_size_bytes: default_max_size_bytes(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailConfig {
    #[serde(default = "default_thumb_cache_path")]
    pub path: PathBuf,

    #[serde(default = "default_thumb_cache_size")]
    pub size: u32,
}

fn default_thumb_cache_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(".cache"))
        .join("clepho/thumbnails")
}

fn default_thumb_cache_size() -> u32 {
    256
}

impl Default for ThumbnailConfig {
    fn default() -> Self {
        Self {
            path: default_thumb_cache_path(),
            size: default_thumb_cache_size(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    /// Whether to check for overdue schedules on startup.
    #[serde(default = "default_check_overdue_on_startup")]
    pub check_overdue_on_startup: bool,

    /// Default start hour for hours of operation (0-23).
    #[serde(default)]
    pub default_hours_start: Option<u8>,

    /// Default end hour for hours of operation (0-23).
    #[serde(default)]
    pub default_hours_end: Option<u8>,
}

fn default_check_overdue_on_startup() -> bool {
    true
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            check_overdue_on_startup: default_check_overdue_on_startup(),
            default_hours_start: None,
            default_hours_end: None,
        }
    }
}

/// Operation mode for centralising files
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CentraliseOperation {
    /// Copy files to library (keeps originals)
    #[default]
    Copy,
    /// Move files to library (removes originals)
    Move,
}

/// Configuration for file centralization/library management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryConfig {
    /// Root path of the managed library
    #[serde(default)]
    pub path: Option<PathBuf>,

    /// Default operation mode (copy or move)
    #[serde(default)]
    pub operation: CentraliseOperation,

    /// Maximum filename length (excluding extension)
    #[serde(default = "default_max_filename_length")]
    pub max_filename_length: usize,
}

fn default_max_filename_length() -> usize {
    100
}

impl Default for LibraryConfig {
    fn default() -> Self {
        Self {
            path: None,
            operation: CentraliseOperation::default(),
            max_filename_length: default_max_filename_length(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LlmProviderType {
    #[default]
    LmStudio,
    OpenAI,
    Anthropic,
    Ollama,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmConfig {
    #[serde(default)]
    pub provider: LlmProviderType,

    #[serde(default = "default_llm_endpoint")]
    pub endpoint: String,

    #[serde(default = "default_llm_model")]
    pub model: String,

    #[serde(default)]
    pub api_key: Option<String>,

    /// Custom prompt/context for image descriptions.
    /// This text will be prepended to the default prompt to provide context.
    /// Example: "These photos are from a 1985 family reunion in Texas."
    #[serde(default)]
    pub custom_prompt: Option<String>,

    /// Number of concurrent LLM requests for batch processing (default: 4)
    #[serde(default = "default_batch_concurrency")]
    pub batch_concurrency: usize,
}

fn default_batch_concurrency() -> usize {
    4
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerConfig {
    #[serde(default = "default_image_extensions")]
    pub image_extensions: Vec<String>,

    #[serde(default = "default_similarity_threshold")]
    pub similarity_threshold: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImageProtocol {
    #[default]
    Auto,
    Sixel,
    Kitty,
    ITerm2,
    Halfblocks,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewConfig {
    #[serde(default = "default_preview_enabled")]
    pub image_preview: bool,

    #[serde(default)]
    pub protocol: ImageProtocol,

    #[serde(default = "default_thumbnail_size")]
    pub thumbnail_size: u32,

    /// External viewer application for right-click open (e.g., "feh", "eog", "gimp")
    /// If not set, uses system default (xdg-open on Linux, open on macOS)
    #[serde(default)]
    pub external_viewer: Option<String>,
}

fn default_preview_enabled() -> bool {
    true
}

fn default_thumbnail_size() -> u32 {
    1024
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            image_preview: default_preview_enabled(),
            protocol: ImageProtocol::default(),
            thumbnail_size: default_thumbnail_size(),
            external_viewer: None,
        }
    }
}

fn default_db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("clepho")
        .join("clepho.db")
}

fn default_llm_endpoint() -> String {
    "http://127.0.0.1:1234/v1".to_string()
}

fn default_llm_model() -> String {
    "gemma-3-4b".to_string()
}

fn default_image_extensions() -> Vec<String> {
    vec![
        "jpg".to_string(),
        "jpeg".to_string(),
        "png".to_string(),
        "gif".to_string(),
        "webp".to_string(),
        "heic".to_string(),
        "heif".to_string(),
        "raw".to_string(),
        "cr2".to_string(),
        "nef".to_string(),
        "arw".to_string(),
    ]
}

fn default_similarity_threshold() -> u32 {
    50 // Hamming distance threshold for perceptual hash similarity (~20% of 256 bits)
       // Higher values catch more edited versions (borders, contrast) but may have false positives
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            image_extensions: default_image_extensions(),
            similarity_threshold: default_similarity_threshold(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database: DatabaseConfig::default(),
            llm: LlmConfig::default(),
            scanner: ScannerConfig::default(),
            preview: PreviewConfig::default(),
            trash: TrashConfig::default(),
            thumbnails: ThumbnailConfig::default(),
            schedule: ScheduleConfig::default(),
            library: LibraryConfig::default(),
            keybindings: KeyBindings::default(),
            view: ViewConfig::default(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            // Create default config
            let config = Config::default();
            config.save()?;
            Ok(config)
        }
    }

    pub fn load_from(path: &std::path::Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path();

        // Ensure parent directory exists
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;

        Ok(())
    }

    fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    /// Get the clepho configuration directory.
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("clepho")
    }
}
