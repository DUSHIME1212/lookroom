//! Keeps the person's own presets, favourites and imported tables on disk.

use crate::pipeline::Grade;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct UserPreset {
    pub id: String,
    pub name: String,
    #[serde(rename = "filters")]
    pub grade: Grade,
    #[serde(rename = "createdAt", default)]
    pub created_at: f64,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub favorites: Vec<String>,
    #[serde(default)]
    pub layout: Layout,
}

/// Panel sizes the person dragged to. None keeps the size that suits the window.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Layout {
    pub bin_w: Option<f32>,
    pub lower_h: Option<f32>,
    /// Track heights as a share of their usual height.
    pub rows: Option<f32>,
}

pub fn data_dir() -> PathBuf {
    crate::platform::data_dir()
}

fn presets_file() -> PathBuf {
    data_dir().join("UserPresets").join("userFilter.json")
}

pub fn luts_dir() -> PathBuf {
    data_dir().join("UserLUTs")
}

pub fn plugins_dir() -> PathBuf {
    data_dir().join("Plugins")
}

pub fn fonts_dir() -> PathBuf {
    data_dir().join("Fonts")
}

pub fn load_presets() -> Vec<UserPreset> {
    fs::read_to_string(presets_file())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_presets(list: &[UserPreset]) -> Result<(), String> {
    let f = presets_file();
    if let Some(p) = f.parent() {
        fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    fs::write(f, text).map_err(|e| e.to_string())
}

pub fn load_settings() -> Settings {
    fs::read_to_string(data_dir().join("settings.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_settings(s: &Settings) {
    let _ = fs::create_dir_all(data_dir());
    if let Ok(t) = serde_json::to_string_pretty(s) {
        let _ = fs::write(data_dir().join("settings.json"), t);
    }
}

pub fn new_id() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("user:{t:x}")
}

pub fn now_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
