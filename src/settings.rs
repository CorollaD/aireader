//! User preferences, recent files and reading positions, persisted as JSON.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::model::Location;

const MAX_RECENT: usize = 30;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    Light,
    Dark,
    Sepia,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Light, Theme::Dark, Theme::Sepia];

    pub fn label(self) -> &'static str {
        match self {
            Theme::Light => "明亮",
            Theme::Dark => "夜间",
            Theme::Sepia => "护眼",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageLayout {
    /// All pages in one vertical scroll.
    #[default]
    Continuous,
    /// One page at a time, fitted to the window.
    Single,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recent {
    pub path: PathBuf,
    pub title: String,
    pub opened: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    pub font_size: f32,
    pub line_spacing: f32,
    pub max_width: f32,
    pub page_layout: PageLayout,
    pub show_toc: bool,
    pub recent: Vec<Recent>,
    /// Canonical file path → last reading position.
    pub positions: HashMap<String, Location>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::Light,
            font_size: 19.0,
            line_spacing: 1.7,
            max_width: 760.0,
            page_layout: PageLayout::Continuous,
            show_toc: true,
            recent: Vec::new(),
            positions: HashMap::new(),
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn book_key(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_owned())
        .to_string_lossy()
        .into_owned()
}

impl Settings {
    pub fn file() -> Option<PathBuf> {
        directories::ProjectDirs::from("com", "aireader", "AiReader")
            .map(|d| d.config_dir().join("settings.json"))
    }

    pub fn load() -> Self {
        Self::file()
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(file) = Self::file() else { return };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            // Write then rename so a crash never leaves a truncated file.
            let tmp = file.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(tmp, file);
            }
        }
    }

    /// Cheap value that changes whenever a display preference changes.
    pub fn prefs_fingerprint(&self) -> (Theme, u32, u32, u32, PageLayout, bool) {
        (
            self.theme,
            self.font_size.to_bits(),
            self.line_spacing.to_bits(),
            self.max_width.to_bits(),
            self.page_layout,
            self.show_toc,
        )
    }

    pub fn touch_recent(&mut self, path: &Path, title: &str) {
        let key = book_key(path);
        self.recent.retain(|r| book_key(&r.path) != key);
        self.recent.insert(
            0,
            Recent {
                path: PathBuf::from(&key),
                title: title.to_owned(),
                opened: now(),
            },
        );
        self.recent.truncate(MAX_RECENT);
    }

    pub fn remove_recent(&mut self, path: &Path) {
        self.recent.retain(|r| r.path != path);
    }
}
