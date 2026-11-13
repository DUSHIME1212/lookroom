//! The running project: bringing media in, keeping the editor on the clip under the
//! playhead, and playing the timeline in step with its sound.

use super::{App, CLIP_PREVIEW, Clip, PREVIEW_MAX, Sheet};
use crate::audio;
use crate::export;
use crate::player::{self, Player, Scene};
use crate::timeline::{Body, Item, Kind as MediaKind, Media, ROW_MUSIC};
use crate::video::{self, Feed};
use eframe::egui::Context;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// Clips kept decoded and ready behind the one on screen.
const KEPT_FEEDS: usize = 2;
/// Finished frames to have in hand before the clock starts.
const PREROLL: u32 = 4;
/// How long to wait for sound before playing without it. A project with many sound items
/// (each a separate ffmpeg input) takes several seconds for the mix to start, and missing
/// this wait leaves the whole run silent, so it is generous.
const SOUND_WAIT: Duration = Duration::from_millis(10_000);

/// A run of the timeline in progress.
pub(super) struct Run {
    pub player: Player,
    /// Kept so the sound stops with the run.
    pub _feed: Option<audio::Feed>,
    pub from: u32,
    pub total: u32,
    pub began: Option<Instant>,
    pub asked: Instant,
    pub by_sound: bool,
    pub held: Option<player::Shown>,
    pub grade_at_start: crate::pipeline::Grade,
    /// The sound it plays, so a change to it (a mute, a volume) is heard straight away.
    pub sound_at_start: Vec<crate::timeline::Part>,
    /// What asked for playback, for the console line.
    pub why: &'static str,
}

impl App {
    pub(super) fn scene(&mut self) -> Arc<Scene> {
        self.sync_active();
        Arc::new(Scene { project: self.project.clone(), bank: self.bank.clone(), stills: self.stills.clone() })
    }

    /// The editor works on `self.grade`; the clip takes it back from there.
    pub(super) fn sync_active(&mut self) {
        let Some(id) = self.active else { return };
        // Read first: a write would take a private copy of every clip for nothing.
        let Some(i) = self.project.clips.iter().position(|c| c.id == id) else { return };
        if *self.project.clips[i].grade != self.grade {
            self.project.clips[i].grade = self.grade.clone().into();
        }
    }

    /// Reads a file into the bin.
    pub(super) fn add_media(&mut self, path: &Path) -> Result<u32, String> {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let id = self.project.new_id();
        let path_kept = Some(path.to_path_buf());
        let m = if video::is_sound(path) {
            Media { id, path: path_kept, name, kind: MediaKind::Audio, seconds: video::sound_seconds(path)?, info: None, has_audio: true, size: (0, 0) }
        } else if video::is_video(path) {
            let i = video::probe(path)?;
            Media { id, path: path_kept, name, kind: MediaKind::Video, seconds: i.seconds(i.frames), has_audio: i.has_audio, size: (i.w, i.h), info: Some(i) }
        } else {
            let o = export::open_photo(path)?;
            let size = (o.picture.w, o.picture.h);
            self.stills.insert(id, Arc::new(o.picture));
            self.exifs.insert(id, o.exif);
            Media { id, path: path_kept, name, kind: MediaKind::Photo, seconds: 0.0, info: None, has_audio: false, size }
        };
        self.project.media.push(m);
        self.want_strip(id);
        Ok(id)
    }

    /// Puts media on the timeline: pictures after the last clip, sound on the music track.
    pub(super) fn place(&mut self, id: u32, index: Option<usize>) {
        let Some(m) = self.project.media_of(id).cloned() else { return };
        if m.kind == MediaKind::Audio {
            let at = self.project.items.iter().filter(|t| t.row == ROW_MUSIC).map(Item::end).max().unwrap_or(0);
            let len = self.project.frames(m.seconds).max(2);
            let new = self.project.new_id();
            let body = Body::Audio { media: id, start: 0.0, volume: 1.0, name: m.name.clone(), speed: 1.0, denoise: 0.0 };
            self.project.items.push(Item { id: new, row: ROW_MUSIC, at, len, body });
            return;
        }
        // The first real picture takes over from the sample, keeping the look on it.
        let sample = (self.project.clips.len() == 1)
            .then(|| self.project.clips[0].clone())
            .filter(|c| self.project.media_of(c.media).is_some_and(|m| m.path.is_none()));
        if let Some(s) = &sample {
            self.sync_active();
            let look = self.look_only();
            self.project.clips.clear();
            self.project.media.retain(|m| m.id != s.media);
            self.active = None;
            let Some(i) = self.project.insert_clip(id, 0) else { return };
            self.project.clips[i].grade = look.into();
        } else {
            let at = index.unwrap_or(self.project.clips.len());
            if self.project.insert_clip(id, at).is_none() {
                return;
            }
        }
        let i = if sample.is_some() { 0 } else { index.unwrap_or(self.project.clips.len() - 1) };
        self.project.suit_first(id);
        self.project.suit_clip(i);
        self.head = self.project.starts()[i.min(self.project.clips.len() - 1)];
        self.needs_activate = true;
    }

    pub(super) fn import(&mut self, path: &Path, to_timeline: bool) {
        match self.add_media(path) {
            Ok(id) if to_timeline => {
                self.place(id, None);
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let hdr = self.project.clips.last().and_then(|c| self.project.media_of(c.media)).and_then(|m| m.info.as_ref()).and_then(|i| i.hdr);
                match hdr {
                    Some(h) => self.say(format!("Added {name}. {} converted to Rec.709", h.name())),
                    None => self.say(format!("Added {name}")),
                }
            }
            Ok(_) => {}
            Err(e) => self.say(format!("That file could not be opened: {e}")),
        }
    }

    /// Brings the editor onto whichever clip is under the playhead.
    pub(super) fn activate(&mut self, ctx: &Context) {
        self.sync_active();
        let Some(i) = self.project.nearest(self.head) else {
            self.active = None;
            return;
        };
        let clip = self.project.clips[i].clone();
        let at = self.project.starts()[i];
        let Some(media) = self.project.media_of(clip.media).cloned() else { return };
        if self.active != Some(clip.id) {
            self.active = Some(clip.id);
            self.grade = (*clip.grade).clone();
            self.active_layer = None;
            self.active_mask = 0;
            self.drawn = None;
            self.photo_name = media.name.clone();
            self.photo_path = media.path.clone();
            self.exif = self.exifs.get(&media.id).cloned().flatten();
        }
        match (media.kind, &media.info) {
            (MediaKind::Video, Some(info)) => {
                let fps = self.project.fps.max(1.0);
                let source = |offset: f64| (clip.source_at(offset, fps) * info.fps).round() as u32;
                let (first, last) = (source(0.0), source(clip.len.saturating_sub(1) as f64));
                let want = source(self.head.saturating_sub(at).min(clip.len.saturating_sub(1)) as f64);
                let same = self.clip.as_ref().is_some_and(|c| c.media == media.id);
                if !same {
                    // A clip left a moment ago keeps its cache, so cutting back to it is instant.
                    if let Some(old) = self.clip.take() {
                        self.spare_feeds.push((old.media, old.feed));
                    }
                    let kept = self.spare_feeds.iter().position(|f| f.0 == media.id).map(|i| self.spare_feeds.remove(i).1);
                    if self.spare_feeds.len() > KEPT_FEEDS {
                        self.spare_feeds.remove(0);
                    }
                    let (w, h) = info.size_within(CLIP_PREVIEW);
                    let feed = kept.unwrap_or_else(|| Feed::new(info.clone(), w, h));
                    self.clip = Some(Clip { info: info.clone(), feed, media: media.id, arrived: None, frame: u32::MAX, first, last });
                }
                if let Some(c) = &mut self.clip {
                    (c.first, c.last) = (first, last);
                    if c.frame != want {
                        c.frame = want;
                        c.feed.seek(want);
                    }
                }
            }
            _ => {
                let already = self.shown_media == Some(media.id) && self.clip.is_none();
                if !already && let Some(p) = self.stills.get(&media.id).cloned() {
                    self.clip = None;
                    self.preview = Arc::new(p.fit(PREVIEW_MAX));
                    self.full = p;
                    self.picture_id += 1;
                    self.shown_media = Some(media.id);
                    self.set_orig_texture(ctx);
                }
            }
        }
    }

    pub(super) fn seek_head(&mut self, frame: u32) {
        self.head = frame.min(self.project.total().saturating_sub(1));
        self.needs_activate = true;
        if self.run.is_some() {
            self.stop_play();
        }
    }

    /// Moves to a frame of the active clip's source, as masks and tracking count them.
    pub(super) fn seek(&mut self, source: u32) {
        let Some(id) = self.active else { return };
        let Some(i) = self.project.clips.iter().position(|c| c.id == id) else { return };
        let Some(c) = &self.clip else { return };
        let clip = &self.project.clips[i];
        let offset = clip.offset_of(source as f64 / c.info.fps.max(1.0), self.project.fps);
        let frame = self.project.starts()[i] + (offset.round().max(0.0) as u32).min(clip.len.saturating_sub(1));
        self.seek_head(frame);
    }

    /// Sets where the active clip begins, from a frame of its source.
    pub(super) fn set_in(&mut self, source: u32) {
        self.trim_to(source, true);
    }

    pub(super) fn set_out(&mut self, source: u32) {
        self.trim_to(source, false);
    }

    fn trim_to(&mut self, source: u32, start: bool) {
        let (Some(id), Some(c)) = (self.active, &self.clip) else { return };
        let Some(clip) = self.project.clips.iter().find(|c| c.id == id) else { return };
        let fps = self.project.fps;
        let at = source as f64 / c.info.fps.max(1.0);
        let into = clip.offset_of(at, fps);
        if start {
            self.project.trim_clip_start(id, into.round() as i64);
        } else {
            let want = into.round() as i64 + 1;
            self.project.trim_clip_end(id, want - clip.len as i64);
        }
        self.needs_activate = true;
    }

    /// `why` names what asked, for the console line when playback begins.
    pub(super) fn play_or_pause(&mut self, why: &'static str) {
        if self.run.is_some() {
            self.stop_play();
        } else {
            self.start_play(why);
        }
    }

    pub(super) fn start_play(&mut self, why: &'static str) {
        let total = self.project.total();
        if total == 0 {
            return;
        }
        let from = if self.head + 1 >= total { 0 } else { self.head };
        self.head = from;
        let scene = self.scene();
        let player = Player::start(scene.clone(), from, self.scope);
        let parts = scene.project.parts();
        let (start, left) = (scene.project.seconds(from), scene.project.seconds(total - from));
        let sound_at_start = parts.clone();
        let feed = self.audio_out.as_ref().map(|o| audio::Feed::start(o, parts, start, left));
        let by_sound = feed.is_some();
        self.run = Some(Box::new(Run { player, _feed: feed, from, total, began: None, asked: Instant::now(), by_sound, held: None, grade_at_start: self.grade.clone(), sound_at_start, why }));
    }

    pub(super) fn stop_play(&mut self) {
        self.play_until = None;
        if let Some(r) = self.run.take() {
            // Kept for the playback timer's report.
            self.play_side = r.player.side.load(Ordering::Relaxed);
        }
        self.needs_activate = true;
        self.drawn = None;
    }

    /// Puts the frame that is due on screen. Called every frame while playing.
    pub(super) fn tick_play(&mut self, ctx: &Context) {
        if self.run.is_none() {
            return;
        }
        // Changing a slider during play takes effect once the pointer lifts; so does a
        // change to the sound, such as muting a clip or setting its volume.
        let changed = self.run.as_ref().is_some_and(|r| r.grade_at_start != self.grade || r.sound_at_start != self.project.parts());
        if changed && !ctx.input(|i| i.pointer.any_down()) {
            self.stop_play();
            self.start_play("a change while playing");
            return;
        }
        let out = self.audio_out.as_ref();
        let Some(r) = self.run.as_mut() else { return };
        if r.began.is_none() {
            let frames = r.player.made.load(Ordering::Relaxed) >= PREROLL.min(r.total.saturating_sub(r.from));
            let sound = !r.by_sound || out.is_some_and(|o| o.has_sound_ready());
            let late = r.asked.elapsed() > SOUND_WAIT;
            if frames && (sound || late) {
                r.by_sound = r.by_sound && sound;
                if let Some(o) = out.filter(|_| r.by_sound) {
                    o.start_together();
                }
                r.began = Some(Instant::now());
                // For --play-test and anyone running from a console: did the sound make it?
                eprintln!("playback started after {} ms, {} (asked by {})", r.asked.elapsed().as_millis(), if r.by_sound { "with sound" } else { "WITHOUT sound" }, r.why);
            }
            ctx.request_repaint();
            return;
        }
        let elapsed = match out.filter(|_| r.by_sound) {
            Some(o) => o.seconds(),
            None => r.began.map_or(0.0, |b| b.elapsed().as_secs_f64()),
        };
        let due = r.from + (elapsed * self.project.fps) as u32;
        r.player.clock.store(due, Ordering::Relaxed);
        let mut show = None;
        loop {
            if r.held.is_none() {
                r.held = r.player.frames.try_recv().ok();
            }
            match &r.held {
                Some(s) if s.frame <= due => show = r.held.take(),
                _ => break,
            }
        }
        let finished = due + 1 >= r.total || self.play_until.is_some_and(|u| due >= u);
        self.head = due.min(self.project.total().saturating_sub(1));
        if let Some(s) = show {
            super::store_texture(ctx, &mut self.view_tex, "view", s.image);
            if let Some(img) = s.scope {
                super::store_texture(ctx, &mut self.scope_tex, "scope", img);
            }
            if let Some((_, t)) = &mut self.play_log {
                t.push(Instant::now());
            }
        }
        if finished {
            self.stop_play();
        }
        ctx.request_repaint();
    }

    pub(super) fn save_project(&mut self) {
        self.sync_active();
        let Some(path) = rfd::FileDialog::new().add_filter("Lookroom project", &["lookroom"]).set_file_name(format!("{}.lookroom", self.project.name)).save_file() else {
            return;
        };
        match serde_json::to_string_pretty(&self.project).map_err(|e| e.to_string()).and_then(|t| std::fs::write(&path, t).map_err(|e| e.to_string())) {
            Ok(()) => { self.remember_project(&path); self.say("Project saved"); },
            Err(e) => self.say(format!("The project could not be saved: {e}")),
        }
    }

    pub(super) fn open_project(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Lookroom project", &["lookroom"]).pick_file() else { return };
        self.load_project(&path);
    }

    /// Opens a saved project: from the Open dialog, or a .lookroom file given on the
    /// command line (which is how Windows opens a double-clicked project).
    pub(super) fn load_project(&mut self, path: &Path) {
        let parsed = std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<crate::timeline::Project>(&t).map_err(|e| e.to_string()));
        let project = match parsed {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{}: {e}", path.display());
                self.say(format!("That project could not be opened: {e}"));
                return;
            }
        };
        self.remember_project(path);
        self.home = false;
        self.stop_play();
        let mut missing = 0;
        for m in project.media.iter() {
            let found = m.path.as_ref().is_some_and(|p| p.exists());
            if !found && m.path.is_some() {
                missing += 1;
            }
            if let (MediaKind::Photo, Some(p)) = (m.kind, m.path.as_ref().filter(|_| found))
                && let Ok(o) = export::open_photo(p)
            {
                self.stills.insert(m.id, Arc::new(o.picture));
                self.exifs.insert(m.id, o.exif);
            }
        }
        let ids: Vec<u32> = project.media.iter().map(|m| m.id).collect();
        self.project = project;
        self.active = None;
        self.clip = None;
        self.shown_media = None;
        self.head = 0;
        self.history = vec![("Opened".to_string(), self.project.clone())];
        self.history_at = 0;
        self.needs_activate = true;
        for id in ids {
            self.want_strip(id);
        }
        if missing > 0 {
            self.say(format!("{missing} file(s) in this project could not be found"));
        }
        self.sheet = None::<Sheet>;
    }
}
