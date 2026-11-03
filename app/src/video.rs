//! Video through the ffmpeg and ffprobe programs: reading clips, playing them and
//! writing the graded result. Colour stays at 16 bits a channel on both sides.

use crate::color::Source;
use crate::lut::LutBank;
use crate::pipeline::{self, Grade, Picture};
use crate::shared::Lru;
use rayon::prelude::*;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, OnceLock};

pub const MAX_SIDE: usize = 3840;
/// Memory each open clip may spend on decoded frames for the paused picture.
const CACHE_BYTES: usize = 320 * 1024 * 1024;
const MIN_CACHED: usize = 16;
/// Frames to read past the one asked for, while nothing else is wanted.
const READ_AHEAD: u32 = 24;
/// How far before a backward step to start reading.
const BACK_WINDOW: u32 = 16;
/// Reading on is quicker than starting ffmpeg again, up to this many frames.
const READ_ON_LIMIT: u32 = 48;
/// Seconds of clip below which its film strip reads every frame.
const STRIP_FULL_READ: f64 = 60.0;
pub const EXTENSIONS: [&str; 7] = ["mp4", "mov", "m4v", "mkv", "avi", "webm", "mts"];

pub const SOUND_EXTENSIONS: [&str; 7] = ["mp3", "wav", "m4a", "aac", "flac", "ogg", "opus"];

pub fn is_sound(p: &Path) -> bool {
    p.extension().is_some_and(|x| SOUND_EXTENSIONS.iter().any(|e| x.eq_ignore_ascii_case(e)))
}

pub fn is_video(p: &Path) -> bool {
    p.extension().is_some_and(|x| EXTENSIONS.iter().any(|e| x.eq_ignore_ascii_case(e)))
}


/// The first folder holding the program, or just its name for the system to look up.
fn search(folders: &[PathBuf], name: &str) -> PathBuf {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    folders.iter().map(|d| d.join(&file)).find(|p| p.is_file()).unwrap_or_else(|| PathBuf::from(name))
}

/// LOOKROOM_FFMPEG first, then beside the app, then an ffmpeg folder beside it, then the PATH.
fn locate(name: &str) -> PathBuf {
    let mut folders: Vec<PathBuf> = std::env::var_os("LOOKROOM_FFMPEG").map(PathBuf::from).into_iter().collect();
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        folders.push(dir.join("ffmpeg"));
        folders.push(dir);
    }
    search(&folders, name)
}

/// Whether video and HEIC can work at all. Asked once, since it starts a program.
pub fn available() -> bool {
    static FOUND: OnceLock<bool> = OnceLock::new();
    *FOUND.get_or_init(|| {
        let works = |n: &str| tool(n).arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
        works("ffmpeg") && works("ffprobe")
    })
}

/// What to say when ffmpeg or ffprobe can't be started.
pub fn cannot_start(e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound { format!("ffmpeg was not found. {}", crate::platform::ffmpeg_hint()) } else { format!("ffmpeg could not be started: {e}") }
}

pub fn tool(name: &str) -> Command {
    let mut c = Command::new(locate(name));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Info {
    pub path: PathBuf,
    pub w: usize,
    pub h: usize,
    pub fps: f64,
    pub frames: u32,
    pub has_audio: bool,
    pub bits: u32,
    pub codec: String,
    /// Whether the clip says which colour matrix it uses.
    pub tagged: bool,
    /// The HDR format, for clips that declare one.
    pub hdr: Option<Source>,
}

impl Info {
    /// HDR clips need converting to look right, so a look that hasn't chosen an
    /// input gets the one the clip declares.
    pub fn suited(&self, g: &Grade) -> Grade {
        let mut g = g.clone();
        if let (Some(s), Source::Rec709) = (self.hdr, g.cst.source) {
            g.cst.source = s;
        }
        g
    }

    /// Untagged clips follow the broadcast convention: BT.709 from 720 lines up.
    fn matrix(&self) -> &'static str {
        if self.tagged {
            "auto"
        } else if self.w.min(self.h) >= 720 {
            "bt709"
        } else {
            "bt601"
        }
    }

    pub fn seconds(&self, frame: u32) -> f64 {
        frame as f64 / self.fps.max(1.0)
    }

    pub fn size_within(&self, side: usize) -> (usize, usize) {
        let long = self.w.max(self.h);
        if long <= side {
            return (self.w & !1, self.h & !1);
        }
        let s = side as f64 / long as f64;
        (((self.w as f64 * s).round() as usize) & !1, ((self.h as f64 * s).round() as usize) & !1)
    }
}

fn ratio(s: &str) -> Option<f64> {
    let (a, b) = s.split_once('/')?;
    let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
    (b != 0.0 && a != 0.0).then_some(a / b)
}

pub fn probe(path: &Path) -> Result<Info, String> {
    let out = tool("ffprobe")
        .args(["-v", "error", "-print_format", "json", "-show_streams", "-show_format"])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| cannot_start(&e))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    parse_probe(path, &doc)
}

fn parse_probe(path: &Path, doc: &serde_json::Value) -> Result<Info, String> {
    let streams = doc["streams"].as_array().ok_or("No streams in that file.")?;
    let v = streams
        .iter()
        .find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1)
        .ok_or("That file has no video.")?;
    let mut w = v["width"].as_u64().unwrap_or(0) as usize;
    let mut h = v["height"].as_u64().unwrap_or(0) as usize;
    if w == 0 || h == 0 {
        return Err("The video's size could not be read.".into());
    }
    // Phones store upright clips sideways with a rotation tag.
    let turned = v["side_data_list"].as_array().is_some_and(|l| {
        l.iter().any(|d| d["rotation"].as_f64().is_some_and(|r| (r.abs() % 180.0 - 90.0).abs() < 1.0))
    }) || v["tags"]["rotate"].as_str().and_then(|r| r.parse::<f64>().ok()).is_some_and(|r| (r.abs() % 180.0 - 90.0).abs() < 1.0);
    if turned {
        std::mem::swap(&mut w, &mut h);
    }
    let fps = v["avg_frame_rate"]
        .as_str()
        .and_then(ratio)
        .or_else(|| v["r_frame_rate"].as_str().and_then(ratio))
        .unwrap_or(24.0);
    let number = |x: &serde_json::Value| x.as_str().and_then(|s| s.parse::<f64>().ok());
    let seconds = number(&v["duration"]).or_else(|| number(&doc["format"]["duration"])).unwrap_or(0.0);
    let frames = number(&v["nb_frames"]).map(|n| n as u32).unwrap_or((seconds * fps).round() as u32).max(1);
    let bits = v["bits_per_raw_sample"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| v["pix_fmt"].as_str().map(|f| if f.contains("10") || f.contains("12") { 10 } else { 8 }))
        .unwrap_or(8);
    Ok(Info {
        path: path.to_path_buf(),
        w,
        h,
        fps,
        frames,
        has_audio: streams.iter().any(|s| s["codec_type"] == "audio"),
        bits,
        codec: v["codec_name"].as_str().unwrap_or("video").to_string(),
        tagged: v["color_space"].as_str().is_some_and(|c| c != "unknown"),
        hdr: match v["color_transfer"].as_str() {
            Some("arib-std-b67") => Some(Source::Hlg),
            Some("smpte2084") => Some(Source::Pq),
            _ => None,
        },
    })
}

/// How long a sound file runs, in seconds.
pub fn sound_seconds(path: &Path) -> Result<f64, String> {
    let out = tool("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| cannot_start(&e))?;
    String::from_utf8_lossy(&out.stdout).trim().parse().map_err(|_| "That sound file could not be read.".to_string())
}

/// A few small pictures spread through a clip, for the timeline's film strip. A long
/// clip reads key frames only, so it takes moments; a short one may hold just one key
/// frame, so it is read in full.
pub fn strip(info: &Info, count: usize, w: usize, h: usize) -> Result<Vec<Vec<u8>>, String> {
    let seconds = info.seconds(info.frames).max(0.1);
    let mut c = tool("ffmpeg");
    c.args(["-v", "error", "-nostdin"]);
    if seconds > STRIP_FULL_READ {
        c.args(["-skip_frame", "nokey"]);
    }
    let out = c
        .arg("-i")
        .arg(&info.path)
        .args(["-map", "0:v:0", "-vf"])
        .arg(format!("fps={:.6},scale={w}:{h}:flags=fast_bilinear", count as f64 / seconds))
        .args(["-frames:v"])
        .arg(count.to_string())
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| cannot_start(&e))?;
    Ok(out.stdout.chunks_exact(w * h * 3).map(<[u8]>::to_vec).collect())
}

/// A running decode of a clip, one frame at a time.
pub struct Reader {
    child: Child,
    pub w: usize,
    pub h: usize,
    buf: Vec<u8>,
    deep: bool,
}

impl Reader {
    pub fn open(info: &Info, frame: u32, w: usize, h: usize, count: Option<u32>) -> Result<Self, String> {
        let mut c = tool("ffmpeg");
        c.args(["-v", "error", "-nostdin", "-ss"]).arg(format!("{:.6}", info.seconds(frame))).arg("-i").arg(&info.path);
        if let Some(n) = count {
            c.arg("-frames:v").arg(n.to_string());
        }
        c.args(["-map", "0:v:0", "-vf"])
            .arg(format!("scale={w}:{h}:in_color_matrix={}:flags=bicubic+accurate_rnd+full_chroma_int", info.matrix()))
            .args(["-f", "rawvideo", "-pix_fmt", "rgb48le", "pipe:1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let child = c.spawn().map_err(|e| cannot_start(&e))?;
        Ok(Self { child, w, h, buf: vec![0; w * h * 6], deep: true })
    }

    /// Reads a clip at the project's rate from a point in seconds. For playback `quick`
    /// trades a little scaling quality and depth for speed; export keeps both.
    /// `speed` above 1 runs through the source faster, keeping the project's frame rate.
    #[allow(clippy::too_many_arguments)]
    pub fn conformed(info: &Info, seconds: f64, w: usize, h: usize, fps: f64, speed: f64, quick: bool, deep: bool) -> Result<Self, String> {
        let mut c = tool("ffmpeg");
        c.args(["-v", "error", "-nostdin"]);
        let scaler = if quick { "fast_bilinear" } else { "bicubic+accurate_rnd+full_chroma_int" };
        let normal = (speed - 1.0).abs() < 1e-6;
        let pace = if normal { String::new() } else { format!("setpts=(PTS-STARTPTS)/{speed:.6},") };
        // Only a clip at some other rate or speed needs converting to the project's rate.
        let rate = if normal && (info.fps - fps).abs() < 0.01 { String::new() } else { format!("{pace}fps={fps:.6},") };
        c.arg("-ss").arg(format!("{seconds:.6}")).arg("-i").arg(&info.path);
        c.args(["-map", "0:v:0", "-vf"])
            .arg(format!("{rate}scale={w}:{h}:in_color_matrix={}:flags={scaler}", info.matrix()))
            .args(["-f", "rawvideo", "-pix_fmt", if deep { "rgb48le" } else { "rgb24" }, "pipe:1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let child = c.spawn().map_err(|e| cannot_start(&e))?;
        Ok(Self { child, w, h, buf: vec![0; w * h * if deep { 6 } else { 3 }], deep })
    }

    pub fn next(&mut self) -> Option<Picture> {
        let out = self.child.stdout.as_mut()?;
        out.read_exact(&mut self.buf).ok()?;
        let px = if self.deep {
            self.buf
                .par_chunks_exact(6)
                .map(|c| {
                    let v = |a: u8, b: u8| u16::from_le_bytes([a, b]) as f32 / 65535.0;
                    [v(c[0], c[1]), v(c[2], c[3]), v(c[4], c[5])]
                })
                .collect()
        } else {
            self.buf.par_chunks_exact(3).map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]).collect()
        };
        Some(Picture { w: self.w, h: self.h, px })
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A still that the image library can't read, such as HEIC. Left to itself ffmpeg
/// assembles a phone's tiles and applies its rotation, so the size is only known after.
pub fn still(path: &Path) -> Result<Picture, String> {
    let out = tool("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-frames:v", "1", "-f", "image2pipe", "-c:v", "png", "-compression_level", "1", "-pix_fmt", "rgb48be", "pipe:1"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| cannot_start(&e))?;
    if !out.status.success() || out.stdout.is_empty() {
        let why = String::from_utf8_lossy(&out.stderr);
        return Err(why.trim().lines().last().unwrap_or("That picture could not be read.").to_string());
    }
    let rgb = image::load_from_memory(&out.stdout).map_err(|e| e.to_string())?.into_rgb32f();
    Ok(Picture::from_rgb32f(rgb.width() as usize, rgb.height() as usize, rgb.as_raw()))
}

pub fn grab(info: &Info, frame: u32, w: usize, h: usize) -> Result<Picture, String> {
    Reader::open(info, frame, w, h, Some(1))?.next().ok_or_else(|| "That frame could not be read.".to_string())
}

/// Decodes the frames the paused picture needs, on its own thread. Everything it reads
/// goes in a cache, and it keeps reading ahead while idle, so stepping forward or going
/// back over ground already covered never has to restart ffmpeg.
pub struct Feed {
    tx: Sender<u32>,
    pub frames: Receiver<(u32, Arc<Picture>)>,
}

/// An open decoder and the frame it will deliver next.
struct Reading {
    reader: Reader,
    next: u32,
}

impl Reading {
    /// Reads one frame into the cache. False once the clip has run out.
    fn read(&mut self, cache: &mut Lru<u32, Arc<Picture>>) -> bool {
        match self.reader.next() {
            Some(p) => {
                cache.put(self.next, Arc::new(p));
                self.next += 1;
                true
            }
            None => false,
        }
    }
}

fn feed_loop(info: Info, w: usize, h: usize, rx: Receiver<u32>, out: Sender<(u32, Arc<Picture>)>) {
    let limit = (CACHE_BYTES / (w * h * size_of::<[f32; 3]>()).max(1)).max(MIN_CACHED);
    let read_ahead = READ_AHEAD.min(limit as u32 / 2);
    let mut cache: Lru<u32, Arc<Picture>> = Lru::new(limit);
    let mut reading: Option<Reading> = None;
    let mut last = 0u32;
    let mut ahead = 0;
    loop {
        // Idle time goes on reading ahead; a request always comes first.
        let asked = if ahead > 0 { rx.try_recv() } else { rx.recv().map_err(|_| TryRecvError::Disconnected) };
        let mut want = match asked {
            Ok(f) => f,
            Err(TryRecvError::Empty) => {
                let more = reading.as_mut().is_some_and(|r| r.read(&mut cache));
                ahead = if more { ahead - 1 } else { 0 };
                continue;
            }
            Err(TryRecvError::Disconnected) => return,
        };
        // Walk to the newest request, looking again after every frame: a drag along the
        // timeline keeps moving it.
        let mut end = None;
        loop {
            let mut moved = false;
            while let Ok(newer) = rx.try_recv() {
                want = newer;
                moved = true;
            }
            if cache.contains(&want) {
                break;
            }
            // Something to look at while the drag outruns the decoder.
            if moved
                && let Some(f) = reading.as_ref().and_then(|r| r.next.checked_sub(1))
                && let Some(p) = cache.get(&f)
                && out.send((f, p)).is_err()
            {
                return;
            }
            let usable = reading.as_ref().is_some_and(|r| r.next <= want && want - r.next <= READ_ON_LIMIT);
            if !usable {
                // Going backward: start earlier, so the next steps back are already here.
                let from = if want < last { want.saturating_sub(BACK_WINDOW) } else { want };
                reading = Reader::open(&info, from, w, h, None).ok().map(|reader| Reading { reader, next: from });
            }
            let Some(r) = reading.as_mut() else { break };
            if !r.read(&mut cache) {
                // Past the clip's real end, which a file's header can overstate.
                end = r.next.checked_sub(1);
                reading = None;
                break;
            }
        }
        let found = cache.get(&want).or_else(|| cache.get(&end?));
        if let Some(p) = found
            && out.send((want, p)).is_err()
        {
            return;
        }
        last = want;
        ahead = read_ahead;
    }
}

impl Feed {
    pub fn new(info: Info, w: usize, h: usize) -> Self {
        let (tx, rx) = mpsc::channel::<u32>();
        let (out, frames) = mpsc::channel();
        std::thread::spawn(move || feed_loop(info, w, h, rx, out));
        Self { tx, frames }
    }

    pub fn seek(&self, frame: u32) {
        let _ = self.tx.send(frame);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Codec {
    #[default]
    H264,
    Hevc,
    ProRes,
}

impl Codec {
    pub const ALL: [Codec; 3] = [Codec::Hevc, Codec::H264, Codec::ProRes];
    pub fn name(self) -> &'static str {
        match self {
            Codec::H264 => "H.264",
            Codec::Hevc => "HEVC",
            Codec::ProRes => "ProRes",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Export {
    pub source: Info,
    pub out: PathBuf,
    pub codec: Codec,
    /// Longest side of the picture going in; never above 4K.
    pub side: usize,
    pub ten_bit: bool,
    pub first: u32,
    pub last: u32,
}

/// Arguments for the writer. Separate so the choices can be tested without encoding.
pub fn encoder_args(e: &Export, w: usize, h: usize) -> Vec<String> {
    let mut a: Vec<String> = ["-y", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgb48le"].map(String::from).to_vec();
    a.extend(["-s".into(), format!("{w}x{h}"), "-r".into(), format!("{:.6}", e.source.fps), "-i".into(), "pipe:0".into()]);
    a.extend(["-ss".into(), format!("{:.6}", e.source.seconds(e.first))]);
    a.extend(["-i".into(), e.source.path.to_string_lossy().to_string()]);
    a.extend(["-map", "0:v:0", "-map", "1:a?", "-c:a", "copy", "-map_metadata", "1", "-shortest"].map(String::from));
    a.extend(picture_args(e.codec, e.ten_bit));
    a.push(e.out.to_string_lossy().to_string());
    a
}

/// The filter, colour tags and encoder settings for the picture.
pub fn picture_args(codec: Codec, ten_bit: bool) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    // Output options alone only reach the matrix; primaries and transfer must be set on the frames.
    let tags = "setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709";
    a.extend(["-vf".into(), format!("{tags},scale=out_color_matrix=bt709:out_range=tv:flags=accurate_rnd+full_chroma_int")]);
    a.extend(["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv"].map(String::from));
    let tail: &[&str] = match codec {
        Codec::H264 if ten_bit => &["-c:v", "libx264", "-crf", "17", "-preset", "medium", "-pix_fmt", "yuv420p10le"],
        Codec::H264 => &["-c:v", "libx264", "-crf", "17", "-preset", "medium", "-pix_fmt", "yuv420p"],
        Codec::Hevc if ten_bit => &["-c:v", "libx265", "-crf", "18", "-preset", "medium", "-pix_fmt", "yuv420p10le", "-tag:v", "hvc1", "-x265-params", "log-level=error"],
        Codec::Hevc => &["-c:v", "libx265", "-crf", "18", "-preset", "medium", "-pix_fmt", "yuv420p", "-tag:v", "hvc1", "-x265-params", "log-level=error"],
        Codec::ProRes => &["-c:v", "prores_ks", "-profile:v", "3", "-vendor", "apl0", "-pix_fmt", "yuv422p10le"],
    };
    a.extend(tail.iter().map(|s| s.to_string()));
    a
}

fn write_frame(to: &mut impl Write, p: &Picture, w: usize, h: usize, buf: &mut Vec<u8>) -> std::io::Result<()> {
    buf.clear();
    for y in 0..h {
        for c in &p.px[y * p.w..y * p.w + w] {
            for v in c {
                buf.extend_from_slice(&((v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16).to_le_bytes());
            }
        }
    }
    to.write_all(buf)
}

/// Grades every frame between `first` and `last` and writes the clip, keeping its sound.
pub fn export(e: &Export, g: &Grade, bank: &LutBank, done: &AtomicU32, cancel: &AtomicBool) -> Result<(), String> {
    let (w, h) = e.source.size_within(e.side.min(MAX_SIDE));
    let total = e.last.saturating_sub(e.first) + 1;
    let mut reader = Reader::open(&e.source, e.first, w, h, Some(total))?;
    let mut encoder: Option<Child> = None;
    let mut size = (0, 0);
    let mut buf = Vec::new();
    let mut result = Ok(());
    let mut n = 0;
    while n < total {
        if cancel.load(Ordering::Relaxed) {
            result = Err("Cancelled".to_string());
            break;
        }
        let Some(frame) = reader.next() else { break };
        let mut out = pipeline::render_at(&frame, g, bank, true, true, e.first + n);
        pipeline::apply_frame(&mut out, g.frame_ratio);
        if encoder.is_none() {
            // Crops can change the size, so the writer starts once the first frame is known.
            size = (out.w & !1, out.h & !1);
            let args = encoder_args(e, size.0, size.1);
            let child = tool("ffmpeg")
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|x| cannot_start(&x))?;
            encoder = Some(child);
        }
        let Some(pipe) = encoder.as_mut().and_then(|c| c.stdin.as_mut()) else { break };
        if let Err(x) = write_frame(pipe, &out, size.0, size.1, &mut buf) {
            result = Err(format!("The video writer stopped: {x}"));
            break;
        }
        n += 1;
        done.store(n, Ordering::Relaxed);
    }
    let Some(mut child) = encoder else { return result.and(Err("No frames could be read.".to_string())) };
    drop(child.stdin.take());
    let mut err = String::new();
    if let Some(mut s) = child.stderr.take() {
        let _ = s.read_to_string(&mut err);
    }
    let status = child.wait().map_err(|x| x.to_string())?;
    if result.is_err() {
        let _ = std::fs::remove_file(&e.out);
        return result;
    }
    if !status.success() {
        return Err(err.trim().lines().last().unwrap_or("The video writer failed.").to_string());
    }
    Ok(())
}

/// An export running on its own thread. `done` counts frames; `finished` holds the result.
pub struct Job {
    pub done: Arc<AtomicU32>,
    pub cancel: Arc<AtomicBool>,
    pub total: u32,
    pub finished: Receiver<Result<(), String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> Info {
        Info { path: "in.mov".into(), w: 3840, h: 2160, fps: 24.0, frames: 48, has_audio: true, bits: 10, codec: "hevc".into(), tagged: true, hdr: None }
    }

    fn job(codec: Codec, ten_bit: bool) -> Export {
        Export { source: info(), out: "out.mp4".into(), codec, side: 3840, ten_bit, first: 0, last: 47 }
    }

    #[test]
    fn prores_is_ten_bit_and_h264_can_be_eight() {
        let a = encoder_args(&job(Codec::ProRes, false), 1920, 1080);
        assert!(a.iter().any(|s| s == "yuv422p10le"));
        let b = encoder_args(&job(Codec::H264, false), 1920, 1080);
        assert!(b.iter().any(|s| s == "yuv420p"));
    }

    #[test]
    fn hevc_is_tagged_for_apple_players_and_sound_is_kept() {
        let a = encoder_args(&job(Codec::Hevc, true), 1920, 1080);
        assert!(a.windows(2).any(|w| w[0] == "-tag:v" && w[1] == "hvc1"));
        assert!(a.windows(2).any(|w| w[0] == "-c:a" && w[1] == "copy"));
        assert!(a.windows(2).any(|w| w[0] == "-map_metadata" && w[1] == "1"));
    }

    #[test]
    fn exports_declare_all_three_colour_tags_on_the_frames() {
        let a = encoder_args(&job(Codec::Hevc, true), 1920, 1080);
        let filter = a.iter().find(|s| s.starts_with("setparams")).expect("frames are tagged");
        for tag in ["colorspace=bt709", "color_primaries=bt709", "color_trc=bt709"] {
            assert!(filter.contains(tag), "{tag}");
        }
    }

    #[test]
    fn sizes_stay_even_and_within_the_limit() {
        assert_eq!(info().size_within(1920), (1920, 1080));
        let odd = Info { w: 1281, h: 721, ..info() };
        assert_eq!(odd.size_within(3840), (1280, 720));
    }

    #[test]
    fn hdr_clips_are_recognised_and_a_chosen_input_is_respected() {
        let doc = serde_json::json!({
            "streams": [{ "codec_type": "video", "width": 1920, "height": 1080, "r_frame_rate": "25/1", "color_transfer": "arib-std-b67" }],
            "format": { "duration": "1.0" }
        });
        let i = parse_probe(Path::new("a.mov"), &doc).unwrap();
        assert_eq!(i.hdr, Some(Source::Hlg));
        assert_eq!(i.suited(&Grade::default()).cst.source, Source::Hlg);
        let mut chosen = Grade::default();
        chosen.cst.source = Source::SLog3;
        assert_eq!(i.suited(&chosen).cst.source, Source::SLog3);
    }

    #[test]
    fn a_folder_with_the_program_wins_and_none_falls_back_to_its_name() {
        let dir = std::env::temp_dir().join(format!("lookroom-find-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join(format!("fake-tool{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(search(&[PathBuf::from("no-such-folder"), dir.clone()], "fake-tool"), file);
        assert_eq!(search(&[PathBuf::from("no-such-folder")], "fake-tool"), PathBuf::from("fake-tool"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_program_says_how_to_get_it() {
        let e = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert!(cannot_start(&e).contains("Install it with"));
    }

    #[test]
    fn untagged_clips_use_the_matrix_their_size_implies() {
        let hd = Info { tagged: false, ..info() };
        assert_eq!(hd.matrix(), "bt709");
        assert_eq!(Info { w: 720, h: 480, ..hd.clone() }.matrix(), "bt601");
        assert_eq!(info().matrix(), "auto");
    }

    #[test]
    fn upright_phone_clips_swap_their_sides() {
        let doc = serde_json::json!({
            "streams": [{
                "codec_type": "video", "codec_name": "hevc", "width": 1920, "height": 1080,
                "avg_frame_rate": "30000/1001", "nb_frames": "300", "pix_fmt": "yuv420p10le",
                "side_data_list": [{ "rotation": -90 }]
            }, { "codec_type": "audio" }],
            "format": { "duration": "10.0" }
        });
        let i = parse_probe(Path::new("a.mov"), &doc).unwrap();
        assert_eq!((i.w, i.h, i.frames, i.bits), (1080, 1920, 300, 10));
        assert!(i.has_audio && (i.fps - 29.97).abs() < 0.01);
    }

    #[test]
    fn missing_frame_counts_come_from_the_duration() {
        let doc = serde_json::json!({
            "streams": [{ "codec_type": "video", "width": 640, "height": 360, "r_frame_rate": "25/1" }],
            "format": { "duration": "4.0" }
        });
        assert_eq!(parse_probe(Path::new("a.mkv"), &doc).unwrap().frames, 100);
    }
}
