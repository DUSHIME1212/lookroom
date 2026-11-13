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
    pub recent_projects: Vec<PathBuf>,
    #[serde(default)]
    pub favorites: Vec<String>,
    #[serde(default)]
    pub layout: Layout,
    #[serde(default)]
    pub saved_layout: Option<Layout>,
    /// The assistant's Anthropic API key, kept on this computer only.
    #[serde(default)]
    pub assistant_key: String,
    /// Which Claude model the assistant uses; empty is the default.
    #[serde(default)]
    pub assistant_model: String,
    /// The KLIPY key for video memes, kept on this computer only.
    #[serde(default)]
    pub klipy_key: String,
    /// An anonymous id for this computer, which KLIPY keeps recent clips under.
    #[serde(default)]
    pub klipy_customer: String,
}

/// Panel sizes the person dragged to. None keeps the size that suits the window.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Layout {
    #[serde(default)]
    pub inspector_right: bool,
    #[serde(default)]
    pub hide_timeline: bool,
    #[serde(default)]
    pub hide_layers: bool,
    #[serde(default)]
    pub layers_w: Option<f32>,
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

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn old_settings_keep_panel_sizes_and_default_new_layout_options() {
        let s: Settings = serde_json::from_str(r#"{"layout":{"bin_w":380,"lower_h":300,"rows":1.2}}"#).unwrap();
        assert_eq!(s.layout.bin_w, Some(380.0));
        assert_eq!(s.layout.lower_h, Some(300.0));
        assert!(!s.layout.inspector_right && !s.layout.hide_timeline && !s.layout.hide_layers);
        assert!(s.saved_layout.is_none());
    }

    #[test]
    fn saved_workspace_survives_a_settings_roundtrip() {
        let layout = Layout { inspector_right: true, hide_timeline: true, layers_w: Some(240.0), ..Default::default() };
        let s = Settings { layout: layout.clone(), saved_layout: Some(layout), ..Default::default() };
        let restored: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        let saved = restored.saved_layout.unwrap();
        assert!(saved.inspector_right && saved.hide_timeline);
        assert_eq!(saved.layers_w, Some(240.0));
    }
}
