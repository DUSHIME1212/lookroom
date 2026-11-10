//! Keeping work safe. A panic writes a crash report; the project is saved in the
//! background every minute while it changes; and after a crash the next start offers that
//! save back. Each running window leaves a marker named for its process, removed when it
//! closes properly, so a marker whose process is gone means that window crashed.

use super::App;
use super::kit::*;
use crate::store;
use crate::timeline::Project;
use eframe::egui::{self, Color32, Context};
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::path::Path;
use std::time::{Duration, Instant};

const AUTOSAVE_EVERY: Duration = Duration::from_secs(60);

fn dir() -> PathBuf {
    store::data_dir().join("Autosave")
}

fn marker(pid: u32) -> PathBuf {
    dir().join(format!("running-{pid}"))
}

fn autosave_file(pid: u32) -> PathBuf {
    dir().join(format!("autosave-{pid}.lookroom"))
}

/// Writes a report of any panic, with where it happened, before the app closes.
pub fn install_panic_hook() {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let folder = store::data_dir().join("Crashes");
        let _ = std::fs::create_dir_all(&folder);
        let when = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let report = format!(
            "Lookroom {} stopped.\n\n{info}\n\nThread: {}\n\n{}",
            env!("CARGO_PKG_VERSION"),
            std::thread::current().name().unwrap_or("unnamed"),
            std::backtrace::Backtrace::force_capture()
        );
        let _ = std::fs::write(folder.join(format!("crash-{when}.txt")), report);
        before(info);
    }));
}

/// Whether a process is still running.
fn alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .creation_flags(0x0800_0000)
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(&format!("\"{pid}\"")))
    }
    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        std::process::Command::new("kill").args(["-0", &pid.to_string()]).status().is_ok_and(|s| s.success())
    }
}

pub struct Safety {
    pid: u32,
    saved: Option<Project>,
    last: Instant,
    /// An autosave left by a window that crashed, waiting to be offered back.
    pub offer: Option<PathBuf>,
}

impl Safety {
    /// Marks this window as running and looks for one that crashed.
    pub fn start() -> Self {
        let pid = std::process::id();
        let _ = std::fs::create_dir_all(dir());
        let mut offer = None;
        for e in std::fs::read_dir(dir()).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Some(other) = name.strip_prefix("running-").and_then(|n| n.parse::<u32>().ok()) else { continue };
            if other == pid || alive(other) {
                continue;
            }
            let _ = std::fs::remove_file(e.path());
            let saved = autosave_file(other);
            if saved.is_file() {
                // Kept under this window's name, so a second crash can't lose it.
                let kept = dir().join(format!("recovered-{other}.lookroom"));
                if std::fs::rename(&saved, &kept).is_ok() {
                    offer = Some(kept);
                }
            }
        }
        let _ = std::fs::write(marker(pid), b"");
        Self { pid, saved: None, last: Instant::now(), offer }
    }
}

impl Drop for Safety {
    /// A proper close: no crash to report, and no autosave needed.
    fn drop(&mut self) {
        let _ = std::fs::remove_file(marker(self.pid));
        let _ = std::fs::remove_file(autosave_file(self.pid));
    }
}

impl App {
    /// Saves the project in the background once a minute while it keeps changing.
    pub(super) fn tick_autosave(&mut self) {
        let Some(s) = &mut self.safety else { return };
        if s.last.elapsed() < AUTOSAVE_EVERY {
            return;
        }
        s.last = Instant::now();
        if s.saved.as_ref() == Some(&self.project) || self.project.clips.is_empty() && self.project.items.is_empty() {
            return;
        }
        let Ok(text) = serde_json::to_string(&self.project) else { return };
        s.saved = Some(self.project.clone());
        let file = autosave_file(s.pid);
        std::thread::spawn(move || {
            // Written aside first, so a crash mid-write leaves the last good save.
            let part = file.with_extension("part");
            if std::fs::write(&part, text).is_ok() {
                let _ = std::fs::rename(&part, &file);
            }
        });
    }

    /// After a crash: offers the work saved just before it.
    pub(super) fn restore_offer(&mut self, ctx: &Context) {
        let Some(path) = self.safety.as_ref().and_then(|s| s.offer.clone()) else { return };
        let mut answer = None;
        egui::Window::new("Restore your work?").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.label(egui::RichText::new("Lookroom closed unexpectedly last time. Your project was saved within a minute of that.").font(regular(13.0)).color(Color32::WHITE));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Restore It").clicked() {
                    answer = Some(true);
                }
                if ui.button("Not Now").clicked() {
                    answer = Some(false);
                }
            });
            ui.label(egui::RichText::new(format!("Kept at {}", path.display())).font(regular(11.0)).color(MUTED));
        });
        match answer {
            Some(true) => {
                self.load_project(&path);
                self.say("Restored the autosaved project. Save it to keep it.");
                if let Some(s) = &mut self.safety {
                    s.offer = None;
                }
            }
            Some(false) => {
                if let Some(s) = &mut self.safety {
                    s.offer = None;
                }
            }
            None => {}
        }
    }
}
