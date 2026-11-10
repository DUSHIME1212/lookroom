//! Auto Captions, the plugin that comes with the app. The timeline's own sound, mixed the
//! way playback mixes it, goes through Whisper (OpenAI's speech model, run by whisper.cpp
//! inside ffmpeg's whisper filter) and comes back as timed lines of text. Nothing leaves
//! the computer: the only download is the model, once.

use crate::store;
use crate::timeline::{Part, mix_graph};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub struct Model {
    pub name: &'static str,
    file: &'static str,
    pub megabytes: u32,
    pub note: &'static str,
}

/// whisper.cpp's conversions of OpenAI's Whisper models (MIT). Each understands about 100
/// languages; larger ones hear more accurately and run slower.
pub const MODELS: [Model; 3] = [
    Model { name: "Tiny", file: "ggml-tiny.bin", megabytes: 75, note: "Fastest; fine for clear speech" },
    Model { name: "Base", file: "ggml-base.bin", megabytes: 142, note: "The balance most people want" },
    Model { name: "Small", file: "ggml-small.bin", megabytes: 466, note: "Most accurate, several times slower" },
];
const MODEL_HOST: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";
/// Languages offered by name; Whisper also finds the language by itself.
pub const LANGUAGES: [(&str, &str); 12] = [
    ("Find it", "auto"),
    ("English", "en"),
    ("French", "fr"),
    ("Spanish", "es"),
    ("Portuguese", "pt"),
    ("German", "de"),
    ("Italian", "it"),
    ("Swahili", "sw"),
    ("Arabic", "ar"),
    ("Chinese", "zh"),
    ("Japanese", "ja"),
    ("Hindi", "hi"),
];
/// Whisper hears at this rate.
const RATE: u32 = 16_000;

pub fn models_dir() -> PathBuf {
    store::data_dir().join("Models")
}

pub fn model_path(m: &Model) -> PathBuf {
    models_dir().join(m.file)
}

pub fn have(m: &Model) -> bool {
    model_path(m).is_file()
}

/// Whether this ffmpeg was built with Whisper. Asked once.
pub fn engine_ready() -> bool {
    static FOUND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FOUND.get_or_init(|| {
        crate::video::tool("ffmpeg").args(["-hide_banner", "-filters"]).stderr(Stdio::null()).output().is_ok_and(|o| {
            String::from_utf8_lossy(&o.stdout).lines().any(|l| l.split_whitespace().nth(1) == Some("whisper"))
        })
    })
}

/// Fetches a model with the system's curl, into a part file renamed only once complete.
/// `got` counts the bytes so far, for a progress bar.
pub fn download(m: &Model, got: Arc<AtomicU32>, stop: Arc<AtomicBool>) -> Result<PathBuf, String> {
    std::fs::create_dir_all(models_dir()).map_err(|e| e.to_string())?;
    let done = model_path(m);
    let part = done.with_extension("part");
    let mut c = std::process::Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let mut child = c
        .args(["-sSfL", "--proto", "=https", "-o"])
        .arg(&part)
        .arg(format!("{MODEL_HOST}{}", m.file))
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl could not start: {e}"))?;
    loop {
        if stop.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&part);
            return Err("Stopped.".into());
        }
        got.store((std::fs::metadata(&part).map_or(0, |d| d.len()) / 1024) as u32, Ordering::Relaxed);
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                let _ = std::fs::remove_file(&part);
                return Err("The model could not be downloaded. Check the connection and try again.".into());
            }
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    std::fs::rename(&part, &done).map_err(|e| e.to_string())?;
    Ok(done)
}

/// One caption: when it shows, in seconds on the timeline, and what it says.
#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

fn srt_time(s: &str) -> Option<f64> {
    let (hms, ms) = s.trim().split_once([',', '.'])?;
    let mut parts = hms.split(':').map(|p| p.trim().parse::<f64>());
    let (h, m, sec) = (parts.next()?.ok()?, parts.next()?.ok()?, parts.next()?.ok()?);
    Some(h * 3600.0 + m * 60.0 + sec + ms.trim().parse::<f64>().ok()? / 1000.0)
}

/// Reads SubRip text: a number, a time range, then the words, blocks apart by a blank line.
pub fn parse_srt(text: &str) -> Vec<Cue> {
    let text = text.replace("\r\n", "\n");
    let mut out = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        let Some(at) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let Some((a, b)) = lines[at].split_once("-->") else { continue };
        let words = lines[at + 1..].join(" ");
        let words = words.trim();
        // Whisper marks what isn't speech, like [Music]; those are not captions.
        let marked = (words.starts_with('[') && words.ends_with(']')) || (words.starts_with('(') && words.ends_with(')'));
        if let (Some(start), Some(end)) = (srt_time(a), srt_time(b))
            && !words.is_empty()
            && !marked
            && end > start
        {
            out.push(Cue { start, end, text: words.to_string() });
        }
    }
    out
}

/// What to listen to and how.
pub struct Job {
    pub parts: Vec<Part>,
    /// Where on the timeline to start, and for how long, in seconds.
    pub from: f64,
    pub seconds: f64,
    pub model: PathBuf,
    pub language: String,
    /// Longest caption line, in characters; 0 lets Whisper choose.
    pub max_len: u32,
    pub translate: bool,
}

/// The ffmpeg arguments that hear the timeline from `from` and write SubRip to `out`.
/// Paths go in by name with ffmpeg run from the model's folder, which spares every
/// escaping rule a filter graph has for drive letters and spaces.
fn arguments(job: &Job, out: &str) -> Vec<String> {
    let (inputs, mix) = mix_graph(&job.parts, job.from, job.seconds, RATE, 0);
    let model = job.model.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let mut whisper = format!("whisper=model={model}:language={}:format=srt:destination={out}:queue=20", job.language);
    if job.max_len > 0 {
        whisper += &format!(":max_len={}", job.max_len);
    }
    if job.translate {
        whisper += ":translate=1";
    }
    let graph = format!("{mix};[mix]aformat=sample_fmts=flt:channel_layouts=mono:sample_rates={RATE},{whisper}[heard]");
    let mut a: Vec<String> = ["-v", "error", "-nostdin", "-progress", "pipe:1", "-nostats"].map(String::from).to_vec();
    a.extend(inputs);
    a.extend(crate::timeline::graph_args(&graph));
    a.extend(["-map".into(), "[heard]".into(), "-f".into(), "null".into(), "-".into()]);
    a
}

/// Listens to the timeline and gives its captions, timed on the timeline. `done` runs
/// 0 to 1000 as the audio is heard; `stop` ends it early.
pub fn transcribe(job: &Job, done: Arc<AtomicU32>, stop: Arc<AtomicBool>) -> Result<Vec<Cue>, String> {
    let dir = job.model.parent().ok_or("The model is missing.")?.to_path_buf();
    let out = format!("captions-{}.srt", std::process::id());
    let _ = std::fs::remove_file(dir.join(&out));
    let mut child = crate::video::tool("ffmpeg")
        .args(arguments(job, &out))
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| crate::video::cannot_start(&e))?;
    let total_us = (job.seconds * 1e6).max(1.0);
    if let Some(pipe) = child.stdout.take() {
        for line in BufReader::new(pipe).lines().map_while(Result::ok) {
            if stop.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Stopped.".into());
            }
            if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.trim().parse::<f64>().ok()) {
                done.store(((us / total_us) * 1000.0).clamp(0.0, 1000.0) as u32, Ordering::Relaxed);
            }
        }
    }
    let result = child.wait_with_output().map_err(|e| e.to_string())?;
    let written = std::fs::read_to_string(dir.join(&out));
    let _ = std::fs::remove_file(dir.join(&out));
    if !result.status.success() {
        let why = String::from_utf8_lossy(&result.stderr);
        return Err(why.trim().lines().last().unwrap_or("Whisper could not finish.").to_string());
    }
    let mut cues = parse_srt(&written.unwrap_or_default());
    for c in &mut cues {
        c.start += job.from;
        c.end += job.from;
    }
    done.store(1000, Ordering::Relaxed);
    Ok(cues)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subrip_is_read_into_timed_lines() {
        let srt = "1\r\n00:00:01,500 --> 00:00:03,250\r\nHello there\r\n\r\n2\r\n00:00:04,000 --> 00:00:06,000\r\n[Music]\r\n\r\n3\r\n00:01:02.100 --> 00:01:03,000\r\nTwo\r\nlines\r\n";
        let c = parse_srt(srt);
        assert_eq!(c.len(), 2, "{c:?}");
        assert_eq!(c[0], Cue { start: 1.5, end: 3.25, text: "Hello there".into() });
        assert!((c[1].start - 62.1).abs() < 1e-9 && c[1].text == "Two lines");
    }

    #[test]
    fn broken_blocks_are_skipped() {
        assert!(parse_srt("nonsense\n\n1\n00:00:05,000 --> 00:00:04,000\nbackwards").is_empty());
        assert!(parse_srt("").is_empty());
    }

    #[test]
    fn the_command_hears_the_mix_at_whispers_rate() {
        let job = Job {
            parts: vec![Part { path: "talk.mp4".into(), at: 0.0, start: 0.0, len: 4.0, volume: 1.0, speed: 1.0, denoise: 0.0 }],
            from: 0.0,
            seconds: 4.0,
            model: PathBuf::from("models/ggml-base.bin"),
            language: "en".into(),
            max_len: 42,
            translate: false,
        };
        let a = arguments(&job, "out.srt").join(" ");
        assert!(a.contains("whisper=model=ggml-base.bin:language=en:format=srt:destination=out.srt"), "{a}");
        assert!(a.contains(":max_len=42") && a.contains("sample_rates=16000") && a.contains("-i talk.mp4"), "{a}");
    }

    #[test]
    fn every_model_names_its_own_file() {
        assert!(MODELS.iter().all(|m| m.file.starts_with("ggml-") && m.file.ends_with(".bin") && m.megabytes > 0));
    }
}
