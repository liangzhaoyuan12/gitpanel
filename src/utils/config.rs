use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::i18n::Language;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub date_format: DateFormat,
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: u32,
    #[serde(default = "default_commit_files_limit")]
    pub commit_files_limit: u32,
    #[serde(default = "default_sidebar_items_limit")]
    pub sidebar_items_limit: u32,
    #[serde(default)]
    pub recent_workspaces: Vec<String>,
    /// Command line used by "Open in <editor>". `None` means unconfigured.
    /// Stored as a command rather than an editor id so a custom command and a
    /// detected one are the same thing to everything downstream.
    #[serde(default)]
    pub external_editor: Option<String>,
    /// UI language preference.
    #[serde(default)]
    pub language: Language,
    /// Config schema version — allows future migrations.
    #[serde(default)]
    pub version: u32,
}

fn default_refresh_interval() -> u32 {
    30
}

fn default_commit_files_limit() -> u32 {
    10
}

fn default_sidebar_items_limit() -> u32 {
    5
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            date_format: DateFormat::European,
            refresh_interval_secs: default_refresh_interval(),
            commit_files_limit: default_commit_files_limit(),
            sidebar_items_limit: default_sidebar_items_limit(),
            recent_workspaces: Vec::new(),
            external_editor: None,
            language: Language::default(),
            version: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum DateFormat {
    #[default]
    European,
    Iso,
    American,
}

impl DateFormat {
    pub fn label(&self) -> &'static str {
        match self {
            DateFormat::European => "dd.MM.yyyy",
            DateFormat::Iso => "yyyy-MM-dd",
            DateFormat::American => "MM/dd/yyyy",
        }
    }

    pub fn format_date(&self, dt: &chrono::DateTime<chrono::Utc>) -> String {
        match self {
            DateFormat::European => dt.format("%d.%m.%Y").to_string(),
            DateFormat::Iso => dt.format("%Y-%m-%d").to_string(),
            DateFormat::American => dt.format("%m/%d/%Y").to_string(),
        }
    }

    pub fn format_datetime(&self, dt: &chrono::DateTime<chrono::Utc>) -> String {
        match self {
            DateFormat::European => dt.format("%d.%m.%Y %H:%M:%S UTC").to_string(),
            DateFormat::Iso => dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            DateFormat::American => dt.format("%m/%d/%Y %H:%M:%S UTC").to_string(),
        }
    }

    pub fn index(&self) -> u32 {
        match self {
            DateFormat::European => 0,
            DateFormat::Iso => 1,
            DateFormat::American => 2,
        }
    }

    pub fn from_index(i: u32) -> Self {
        match i {
            1 => DateFormat::Iso,
            2 => DateFormat::American,
            _ => DateFormat::European,
        }
    }
}

fn config_path() -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("io.github.liangzhaoyuan12");
    dir.join("config.json")
}

impl AppConfig {
    pub fn load() -> Self {
        let path = config_path();
        if let Ok(data) = fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn add_recent_workspace(&mut self, path: &str) {
        self.recent_workspaces.retain(|p| p != path);
        self.recent_workspaces.insert(0, path.to_string());
        self.recent_workspaces.truncate(10);
        self.save();
    }

    pub fn save(&self) -> bool {
        let path = config_path();
        if let Some(dir) = path.parent() {
            if fs::create_dir_all(dir).is_err() {
                return false;
            }
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => fs::write(&path, json).is_ok(),
            Err(_) => false,
        }
    }
}
