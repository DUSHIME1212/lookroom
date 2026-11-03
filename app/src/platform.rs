//! What differs between Windows, macOS and Linux: where the app keeps its files, how a
//! folder is shown, which faces the interface and text use, and how to get ffmpeg.

use std::path::{Path, PathBuf};

/// The app's own folder: %APPDATA%\Lookroom, ~/Library/Application Support/Lookroom, or
/// $XDG_DATA_HOME/lookroom (~/.local/share/lookroom).
pub fn data_dir() -> PathBuf {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).map(|d| d.join("Lookroom"))
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library").join("Application Support").join("Lookroom"))
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| home().map(|h| h.join(".local").join("share"))).map(|d| d.join("lookroom"))
    };
    base.unwrap_or_else(|| PathBuf::from("Lookroom"))
}

/// Opens a folder in the system's file browser, making it first if need be.
pub fn reveal(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(dir).spawn();
}

/// How to get ffmpeg on this system, said the way its people would do it.
pub fn ffmpeg_hint() -> &'static str {
    if cfg!(windows) {
        "Install it with winget install Gyan.FFmpeg, or put ffmpeg.exe and ffprobe.exe next to Lookroom."
    } else if cfg!(target_os = "macos") {
        "Install it with brew install ffmpeg, or put ffmpeg and ffprobe next to Lookroom."
    } else {
        "Install it with your package manager (sudo apt install ffmpeg, sudo dnf install ffmpeg), or put ffmpeg and ffprobe next to Lookroom."
    }
}

/// Families for the interface and for text on pictures, the first one installed winning.
pub const SANS: &[&str] = if cfg!(windows) {
    &["Segoe UI", "Arial"]
} else if cfg!(target_os = "macos") {
    &["SF Pro Text", "SF Pro", ".AppleSystemUIFont", "Helvetica Neue", "Helvetica", "Arial"]
} else {
    &["Inter", "Noto Sans", "Cantarell", "Ubuntu", "DejaVu Sans", "Liberation Sans"]
};

/// Font files that are always where Windows keeps them, read without searching every
/// installed font, which would slow the start.
pub const WINDOWS_UI: [(&str, u16); 3] = [("C:\\Windows\\Fonts\\segoeui.ttf", 400), ("C:\\Windows\\Fonts\\seguisb.ttf", 600), ("C:\\Windows\\Fonts\\segoeuib.ttf", 700)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_folder_is_named_for_the_app() {
        let d = data_dir().to_string_lossy().to_lowercase();
        assert!(d.ends_with("lookroom"), "{d}");
    }

    #[test]
    fn every_system_has_a_way_to_get_ffmpeg() {
        assert!(ffmpeg_hint().contains("ffmpeg") && ffmpeg_hint().contains("next to Lookroom"));
        assert!(!SANS.is_empty());
    }
}
