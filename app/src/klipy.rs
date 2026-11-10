//! Video memes from KLIPY's clip library: trending and search, with a moving preview for
//! each and the clip itself fetched only when it's put on the timeline. Both are kept in
//! %APPDATA%\Lookroom\KLIPY, so a clip used once works offline afterwards. Requests go
//! through the system's curl, as the meme templates and caption models do.
//!
//! The key goes in the request's address, so the address reaches curl through its
//! standard input, never on the command line where other programs could read it.

use crate::store;
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

const API: &str = "https://api.klipy.com/api/v1/";
/// Clips and previews are only ever fetched from KLIPY's own file host.
const FILE_HOST: &str = "https://static.klipy.com/";
pub const PER_PAGE: u32 = 24;
const TIMEOUT_SECONDS: &str = "30";

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub id: String,
    pub title: String,
    pub mp4: String,
    /// A small moving picture of the clip, for choosing.
    pub preview: String,
    pub width: u32,
    pub height: u32,
    /// The clip's file size, in bytes.
    pub size: u64,
}

pub struct Page {
    pub clips: Vec<Clip>,
    pub has_next: bool,
}

/// The key to use: the one saved in the app, or else the KLIPY_API_KEY variable.
pub fn key(saved: &str) -> Option<String> {
    let saved = saved.trim();
    if !saved.is_empty() {
        return Some(saved.to_string());
    }
    std::env::var("KLIPY_API_KEY").ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
}

fn dir() -> PathBuf {
    store::data_dir().join("KLIPY")
}

/// The clips everyone is using now. `customer` keeps one person's recent clips apart.
pub fn trending(key: &str, customer: &str, page: u32) -> Result<Page, String> {
    get(&format!("clips/trending?page={page}&per_page={PER_PAGE}&customer_id={}", encode(customer)), key)
}

pub fn search(key: &str, customer: &str, query: &str, page: u32) -> Result<Page, String> {
    get(&format!("clips/search?q={}&page={page}&per_page={PER_PAGE}&customer_id={}", encode(query.trim()), encode(customer)), key)
}

fn get(path: &str, key: &str) -> Result<Page, String> {
    let clean: String = key.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let text = curl(&format!("{API}{clean}/{path}"), None)?;
    let reply: Value = serde_json::from_str(&text).map_err(|_| "KLIPY's answer couldn't be read. Try again.".to_string())?;
    read_page(&reply)
}

fn read_page(reply: &Value) -> Result<Page, String> {
    if reply["result"] != true {
        let why = reply["errors"]["message"].as_array().and_then(|m| m.first()).and_then(Value::as_str).unwrap_or("KLIPY turned the request down.");
        return Err(if why.contains("key") { "The KLIPY key wasn't accepted. Check it in Settings.".into() } else { why.to_string() });
    }
    let data = &reply["data"];
    // Ads and anything else that isn't a clip are left out.
    let clips = data["data"].as_array().into_iter().flatten().filter(|c| c["type"] == "clip").filter_map(read_clip).collect();
    Ok(Page { clips, has_next: data["has_next"] == true })
}

fn read_clip(c: &Value) -> Option<Clip> {
    let s = |v: &Value| v.as_str().map(String::from);
    let mp4 = s(&c["file"]["mp4"]).filter(|u| u.starts_with(FILE_HOST))?;
    let preview = s(&c["file"]["webp"]).or_else(|| s(&c["file"]["gif"])).filter(|u| u.starts_with(FILE_HOST))?;
    let meta = &c["file_meta"]["mp4"];
    Some(Clip {
        id: c["id"].as_u64().map(|i| i.to_string()).or_else(|| s(&c["id"]))?,
        title: s(&c["title"]).unwrap_or_default(),
        mp4,
        preview,
        width: meta["width"].as_u64().unwrap_or(0) as u32,
        height: meta["height"].as_u64().unwrap_or(0) as u32,
        size: meta["size"].as_u64().unwrap_or(0),
    })
}

/// The clip's preview, fetched once.
pub fn preview(c: &Clip) -> Result<PathBuf, String> {
    let ext = if c.preview.ends_with(".gif") { "gif" } else { "webp" };
    fetch(&c.preview, &dir().join("Previews").join(format!("{}.{ext}", c.id)))
}

/// The clip itself, fetched once and kept under its title.
pub fn download(c: &Clip) -> Result<PathBuf, String> {
    let name: String = c.title.chars().map(|ch| if ch.is_alphanumeric() || ch == ' ' || ch == '-' { ch } else { '_' }).collect::<String>().trim().chars().take(60).collect();
    let name = if name.is_empty() { "clip".to_string() } else { name };
    fetch(&c.mp4, &dir().join("Clips").join(format!("{name} {}.mp4", c.id)))
}

fn fetch(url: &str, to: &Path) -> Result<PathBuf, String> {
    if to.is_file() {
        return Ok(to.to_path_buf());
    }
    if !url.starts_with(FILE_HOST) {
        return Err("That file isn't on KLIPY.".into());
    }
    std::fs::create_dir_all(to.parent().ok_or("No folder for the file.")?).map_err(|e| e.to_string())?;
    let part = to.with_extension("part");
    curl(url, Some(&part)).inspect_err(|_| {
        let _ = std::fs::remove_file(&part);
    })?;
    std::fs::rename(&part, to).map_err(|e| e.to_string())?;
    Ok(to.to_path_buf())
}

/// Fetches `url`, into `to` when given, else returning the text.
fn curl(url: &str, to: Option<&Path>) -> Result<String, String> {
    let mut c = std::process::Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c.args(["-sS", "-L", "--proto", "=https", "--max-time", TIMEOUT_SECONDS, "-K", "-"]);
    if let Some(p) = to {
        c.arg("-f").arg("-o").arg(p);
    }
    let mut child = c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("curl could not start: {e}"))?;
    if let Some(mut input) = child.stdin.take() {
        let _ = writeln!(input, "url = \"{}\"", url.replace(['"', '\\'], ""));
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("Couldn't reach KLIPY. Check the connection.".into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Letters, digits and a few marks stay; everything else is %-escaped for an address.
fn encode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_page_keeps_clips_and_drops_ads_and_foreign_files() {
        let reply = json!({ "result": true, "data": { "has_next": true, "data": [
            { "id": 45, "type": "clip", "title": "Surprised", "file": { "mp4": "https://static.klipy.com/a.mp4", "webp": "https://static.klipy.com/a.webp" }, "file_meta": { "mp4": { "width": 854, "height": 480, "size": 699934 } } },
            { "id": 46, "type": "ad", "file": { "mp4": "https://static.klipy.com/b.mp4", "webp": "https://static.klipy.com/b.webp" } },
            { "id": 47, "type": "clip", "file": { "mp4": "https://elsewhere.example/c.mp4", "webp": "https://static.klipy.com/c.webp" } },
        ]}});
        let p = read_page(&reply).unwrap();
        assert!(p.has_next);
        assert_eq!(p.clips.len(), 1);
        assert_eq!((p.clips[0].id.as_str(), p.clips[0].width, p.clips[0].size), ("45", 854, 699934));
    }

    #[test]
    fn a_refused_key_reads_plainly() {
        let e = read_page(&json!({ "result": false, "errors": { "message": ["The provided API key is invalid."] } })).err().unwrap();
        assert!(e.contains("Settings"), "{e}");
    }

    #[test]
    fn searches_are_escaped() {
        assert_eq!(encode("no way & yes"), "no%20way%20%26%20yes");
        assert_eq!(encode("ñ"), "%C3%B1");
    }
}
