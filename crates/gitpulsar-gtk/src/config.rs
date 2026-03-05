use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub date_format: DateFormat,
    pub refresh_interval_secs: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            date_format: DateFormat::European,
            refresh_interval_secs: 15,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum DateFormat {
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
        .join("dev.gitpulsar");
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

    pub fn save(&self) {
        let path = config_path();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(&path, json);
        }
    }
}
