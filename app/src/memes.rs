//! Meme templates, from Imgflip's public list of its 100 most used. Nothing is bundled:
//! the list and each picture are fetched only when someone browses them, and kept in
//! %APPDATA%\Lookroom\Memes so they work offline afterwards. The pictures belong to
//! their owners. Fetching uses the system's curl, as video uses ffmpeg.

use crate::store;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const LIST_URL: &str = "https://api.imgflip.com/get_memes";
/// Pictures are only ever fetched from Imgflip's own image host.
const IMAGE_HOST: &str = "https://i.imgflip.com/";
const TIMEOUT_SECONDS: &str = "20";

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    /// How many captions the template is made for.
    #[serde(default, rename = "box_count")]
    pub boxes: u32,
}

#[derive(Deserialize)]
struct Reply {
    success: bool,
    data: Option<Data>,
}

#[derive(Deserialize)]
struct Data {
    memes: Vec<Template>,
}

pub fn dir() -> PathBuf {
    store::data_dir().join("Memes")
}

fn curl() -> Command {
    let mut c = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c.args(["-sSfL", "--proto", "=https", "--max-time", TIMEOUT_SECONDS]);
    c.stdin(Stdio::null());
    c
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let out = curl().arg(url).output().map_err(|e| format!("curl could not start: {e}"))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        return Err(why.trim().lines().last().unwrap_or("The download failed.").to_string());
    }
    Ok(out.stdout)
}

/// Keeps only templates that are safe to save and that the app can open.
fn usable(list: Vec<Template>) -> Vec<Template> {
    list.into_iter()
        .filter(|t| !t.id.is_empty() && t.id.chars().all(|c| c.is_ascii_alphanumeric()))
        .filter(|t| t.url.starts_with(IMAGE_HOST) && extension(&t.url).is_some())
        .collect()
}

fn parse(bytes: &[u8]) -> Result<Vec<Template>, String> {
    let r: Reply = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    match (r.success, r.data) {
        (true, Some(d)) => Ok(usable(d.memes)),
        _ => Err("Imgflip didn't send a list.".into()),
    }
}

/// The current list, or the last one fetched when there is no connection.
pub fn list() -> Result<Vec<Template>, String> {
    let cached = dir().join("memes.json");
    match fetch(LIST_URL).and_then(|b| parse(&b).map(|l| (b, l))) {
        Ok((bytes, list)) => {
            let _ = std::fs::create_dir_all(dir());
            let _ = std::fs::write(&cached, bytes);
            Ok(list)
        }
        Err(e) => std::fs::read(&cached).ok().and_then(|b| parse(&b).ok()).ok_or(e),
    }
}

fn extension(url: &str) -> Option<&'static str> {
    let lower = url.to_lowercase();
    [".jpg", ".jpeg", ".png"].into_iter().find(|x| lower.ends_with(x)).map(|x| &x[1..])
}

pub fn image_path(t: &Template) -> Option<PathBuf> {
    Some(dir().join(format!("{}.{}", t.id, extension(&t.url)?)))
}

/// The template's picture on disk, fetching it the first time.
pub fn image(t: &Template) -> Result<PathBuf, String> {
    let path = image_path(t).ok_or("That template isn't a picture the app can open.")?;
    if path.exists() {
        return Ok(path);
    }
    if !t.url.starts_with(IMAGE_HOST) {
        return Err("That template's picture isn't on Imgflip.".into());
    }
    let bytes = fetch(&t.url)?;
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let partial = path.with_extension("part");
    std::fs::write(&partial, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&partial, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"success":true,"data":{"memes":[
        {"id":"181913649","name":"Drake Hotline Bling","url":"https://i.imgflip.com/30b1gx.jpg","width":1200,"height":1200,"box_count":2,"captions":1},
        {"id":"../evil","name":"Bad","url":"https://i.imgflip.com/x.jpg","box_count":2},
        {"id":"1","name":"Elsewhere","url":"https://example.com/x.jpg","box_count":2},
        {"id":"2","name":"Moving","url":"https://i.imgflip.com/y.gif","box_count":2}
    ]}}"#;

    #[test]
    fn only_safe_openable_templates_are_kept() {
        let l = parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!((l[0].name.as_str(), l[0].boxes), ("Drake Hotline Bling", 2));
        assert!(image_path(&l[0]).unwrap().ends_with("181913649.jpg"));
    }

    #[test]
    fn a_failed_reply_is_an_error() {
        assert!(parse(br#"{"success":false,"error_message":"no"}"#).is_err());
        assert!(parse(b"not json").is_err());
    }
}
