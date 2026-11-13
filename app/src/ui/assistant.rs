//! The Assistant page: a chat with Claude that edits the open project through tools,
//! such as adding titles, restyling captions, grading clips, changing the shape and
//! applying the brand kit. Every change it makes is one Ctrl Z away.

use super::App;
use super::edit::RATIOS;
use super::kit::*;
use crate::assistant::{self, calls, said};
use crate::brand::{hex, parse_hex};
use crate::textanim::{Anim, Backing, Phase, Repeat, Sway};
use crate::timeline::{Body, Fx, Item, Picked, ROW_FX, ROW_FX2, Wipe};
use eframe::egui::{self, Color32, Key, Rect, Sense, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::sync::mpsc::{self, Receiver};

/// Tool rounds one message may take before the assistant is asked to stop.
const MOST_ROUNDS: u32 = 24;
const MOST_FONTS: usize = 30;
/// Items described to the assistant; a longer project is summarised past this.
const MOST_ITEMS: usize = 400;
const CHAT_H: f32 = 360.0;
const SUGGESTIONS: [&str; 5] = [
    "Make the captions bold and yellow with an outline",
    "Add a title that says Summer Sale at the start",
    "Give every clip a warm film look",
    "Make it vertical for Reels and fill the frame",
    "Apply our brand kit",
];

const SYSTEM: &str = "You are the assistant inside Lookroom, a video editor and colour grading app. You change the person's open project with the tools you have.

Times are seconds from the start of the timeline. Positions are fractions of the picture and place an item's centre: x 0 is the left edge and 1 the right, y 0 the top and 1 the bottom. Captions sit near y 0.86. Colours are hex, like #FFD21A. Text sizes are points on a 1080-high picture.

Look at the project with get_project before changing anything you haven't seen in this conversation. When the project uses a brand kit, use its fonts and colours unless asked otherwise, and write any words in its voice. Use find_fonts before naming a font that isn't in the brand kit.

Editing: the main track holds the A-roll, the shots whose sound carries the video. B-roll goes over it with add_cutaway, silent, so the A-roll's sound runs on underneath. Build a sequence with add_clip from the media bin, trim with split and remove_range, and shape each join with set_cut: J and L cuts let sound lead or trail the picture, smash cuts jolt, jump cuts skip time inside one shot. Cross-cutting is cutaways from two scenes in turn. You can't see the footage, so choose shots by their names and lengths, and say so when a choice is a guess.

Do what is asked, then say in one or two plain sentences what changed. Ask a question only when the request can't be done without the answer. You can't see the video's pictures or hear its sound; if a request depends on them, say so. The person can undo any change with Ctrl+Z. Reply in the language the person writes in.";

#[derive(Clone)]
enum Line {
    You(String),
    Claude(String),
    Did(String),
    Problem(String),
}

pub struct AssistantState {
    history: Vec<Value>,
    lines: Vec<Line>,
    input: String,
    waiting: Option<Receiver<Result<Value, String>>>,
    rounds: u32,
    key_input: String,
    tools: Option<Value>,
}

impl Default for AssistantState {
    fn default() -> Self {
        Self { history: Vec::new(), lines: Vec::new(), input: String::new(), waiting: None, rounds: 0, key_input: String::new(), tools: None }
    }
}

impl App {
    pub(super) fn assistant_page(&mut self, ui: &mut Ui) {
        ui.label(egui::RichText::new("Assistant").font(bold(15.0)).color(Color32::WHITE));
        let key = assistant::key(&self.settings.assistant_key);
        if key.is_none() {
            return self.key_form(ui);
        }
        note(ui, "Tell it what to change: titles, captions, colour, shape, effects, the brand kit. It edits this project; Ctrl+Z undoes anything.");
        let w = ui.available_width();
        if self.edit.assistant.lines.is_empty() {
            for s in SUGGESTIONS {
                let (r, _) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
                if action(ui, r, s, false, 12.0).clicked() {
                    self.edit.assistant.input = s.to_string();
                }
            }
        } else {
            self.chat_lines(ui);
        }
        let busy = self.edit.assistant.waiting.is_some();
        if busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(egui::RichText::new("Working…").font(regular(12.5)).color(MUTED));
                if ui.button("Stop").clicked() {
                    self.edit.assistant.waiting = None;
                    self.edit.assistant.lines.push(Line::Problem("Stopped.".into()));
                }
            });
        }
        let edit = ui.add(egui::TextEdit::multiline(&mut self.edit.assistant.input).desired_rows(3).desired_width(f32::INFINITY).hint_text("Ask for a change. Enter sends; Shift+Enter starts a new line."));
        let enter = edit.has_focus() && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);
        if enter {
            // The Enter that sent it shouldn't stay behind as a blank line.
            let t = self.edit.assistant.input.trim_end().to_string();
            self.edit.assistant.input = t;
        }
        let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        let half = Rect::from_min_size(r.min, vec2(w * 0.62, r.height()));
        let can_send = !busy && !self.edit.assistant.input.trim().is_empty();
        if (action(ui, half, "Send", can_send, 13.0).clicked() || enter) && can_send {
            let text = std::mem::take(&mut self.edit.assistant.input);
            self.ask(text.trim());
        }
        let rest = Rect::from_min_max(pos2(half.right() + 6.0, r.top()), r.max);
        if action(ui, rest, "New Chat", false, 12.5).on_hover_text("Start again; changes already made stay").clicked() && !busy {
            self.edit.assistant.history.clear();
            self.edit.assistant.lines.clear();
        }
        ui.add_space(6.0);
        let names: Vec<String> = assistant::MODELS.iter().map(|m| format!("{}: {}", m.1, m.2)).collect();
        let labels: Vec<&str> = names.iter().map(String::as_str).collect();
        let chosen = assistant::model(&self.settings.assistant_model);
        let mut at = assistant::MODELS.iter().position(|m| m.0 == chosen).unwrap_or(0);
        let (r, _) = ui.allocate_exact_size(vec2(w, 52.0), Sense::hover());
        if dropdown(ui, r, Some("Claude model"), &labels, &mut at) {
            self.settings.assistant_model = assistant::MODELS[at].0.to_string();
            crate::store::save_settings(&self.settings);
        }
        note(ui, "Runs through your Anthropic API key. Your words and a description of this project are sent to Anthropic; the video itself is not.");
        if ui.add(egui::Button::new(egui::RichText::new("Change API key").font(regular(11.5)).color(MUTED)).frame(false)).clicked() {
            self.settings.assistant_key.clear();
            crate::store::save_settings(&self.settings);
        }
    }

    fn key_form(&mut self, ui: &mut Ui) {
        note(ui, "The assistant runs on Claude through your own Anthropic API key. Make one at console.anthropic.com, then paste it here. It is kept on this computer.");
        ui.add(egui::TextEdit::singleline(&mut self.edit.assistant.key_input).password(true).hint_text("sk-ant-…").desired_width(f32::INFINITY));
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 36.0), Sense::hover());
        let ready = !self.edit.assistant.key_input.trim().is_empty();
        if action(ui, r, "Save Key", ready, 13.0).clicked() && ready {
            self.settings.assistant_key = std::mem::take(&mut self.edit.assistant.key_input).trim().to_string();
            crate::store::save_settings(&self.settings);
        }
        note(ui, "A key in the ANTHROPIC_API_KEY variable is used too.");
    }

    fn chat_lines(&mut self, ui: &mut Ui) {
        let lines = self.edit.assistant.lines.clone();
        egui::ScrollArea::vertical().id_salt("assistant-chat").max_height(CHAT_H).auto_shrink([false, true]).stick_to_bottom(true).show(ui, |ui| {
            for l in &lines {
                match l {
                    Line::You(t) => {
                        egui::Frame::new().fill(RAISED).corner_radius(4).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(egui::RichText::new(t).font(regular(12.5)).color(Color32::WHITE));
                        });
                    }
                    Line::Claude(t) => {
                        ui.label(egui::RichText::new(t).font(regular(12.5)).color(TEXT_2));
                    }
                    Line::Did(t) => {
                        ui.label(egui::RichText::new(t).font(regular(11.5)).color(ACCENT));
                    }
                    Line::Problem(t) => {
                        ui.label(egui::RichText::new(t).font(regular(12.0)).color(ERROR));
                    }
                }
                ui.add_space(4.0);
            }
        });
    }

    /// The person's words go out with the whole conversation so far.
    fn ask(&mut self, text: &str) {
        assistant::push_user(&mut self.edit.assistant.history, text);
        self.edit.assistant.lines.push(Line::You(text.to_string()));
        self.edit.assistant.rounds = 0;
        self.send_turn();
    }

    fn send_turn(&mut self) {
        let Some(key) = assistant::key(&self.settings.assistant_key) else { return };
        let tools = self.edit.assistant.tools.get_or_insert_with(|| tools(&self.builtins.iter().map(|b| b.name.clone()).chain(self.user.iter().map(|u| u.name.clone())).collect::<Vec<_>>())).clone();
        let model = assistant::model(&self.settings.assistant_model);
        let body = assistant::body(model, SYSTEM, &tools, &self.edit.assistant.history);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(assistant::send(&key, &body));
        });
        self.edit.assistant.waiting = Some(rx);
    }

    /// Takes a finished reply: shows its words, runs its tools and, when it asked for
    /// them, sends the results back for the next step.
    pub(super) fn poll_assistant(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.edit.assistant.waiting else { return };
        let got = match rx.try_recv() {
            Ok(g) => g,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err("The assistant stopped unexpectedly.".into()),
        };
        self.edit.assistant.waiting = None;
        let reply = match got {
            Ok(r) => r,
            Err(e) => {
                self.edit.assistant.lines.push(Line::Problem(e));
                return;
            }
        };
        // Exactly as it came, so its thinking stays valid for the next request.
        self.edit.assistant.history.push(json!({ "role": "assistant", "content": reply["content"].clone() }));
        for t in said(&reply) {
            self.edit.assistant.lines.push(Line::Claude(t));
        }
        match reply["stop_reason"].as_str().unwrap_or("") {
            "tool_use" => {
                let mut results = Vec::new();
                for (id, name, input) in calls(&reply) {
                    let (content, error, did) = match self.run_tool(&name, &input) {
                        Ok((content, did)) => (content, false, did),
                        Err(e) => (e.clone(), true, Some(format!("Couldn't {}: {e}", name.replace('_', " ")))),
                    };
                    if let Some(d) = did {
                        self.edit.assistant.lines.push(if error { Line::Problem(d) } else { Line::Did(d) });
                    }
                    results.push(assistant::result(&id, &content, error));
                }
                self.edit.assistant.history.push(json!({ "role": "user", "content": results }));
                self.edit.assistant.rounds += 1;
                if self.edit.assistant.rounds >= MOST_ROUNDS {
                    self.edit.assistant.lines.push(Line::Problem("That took many steps, so it stopped here. Say \"go on\" to continue.".into()));
                } else {
                    self.send_turn();
                }
            }
            "refusal" => self.edit.assistant.lines.push(Line::Problem("The assistant declined that request.".into())),
            "max_tokens" => self.edit.assistant.lines.push(Line::Problem("The answer ran too long and was cut off. Ask for less at once.".into())),
            _ => {}
        }
        ctx.request_repaint();
    }

    /// Runs one tool: what goes back to Claude, and a line for the chat when it changed
    /// something.
    fn run_tool(&mut self, name: &str, input: &Value) -> Result<(String, Option<String>), String> {
        let num = |k: &str| input.get(k).and_then(Value::as_f64);
        let text = |k: &str| input.get(k).and_then(Value::as_str);
        match name {
            "get_project" => Ok((self.describe().to_string(), None)),
            "find_fonts" => {
                let q = text("query").unwrap_or("").to_lowercase();
                let found: Vec<String> = crate::overlay::font_families().into_iter().filter(|f| f.to_lowercase().contains(&q)).take(MOST_FONTS).collect();
                Ok((json!({ "fonts": found }).to_string(), None))
            }
            "add_text" => {
                let words = text("text").filter(|t| !t.trim().is_empty()).ok_or("text is empty")?.to_string();
                let at = self.project.frames(num("start").unwrap_or(self.project.seconds(self.head)).max(0.0));
                let len = self.project.frames(num("duration").unwrap_or(crate::timeline::ITEM_SECONDS).max(0.1)).max(2);
                let (mut colour, mut motion) = ([1.0; 3], crate::textanim::Motion::default());
                self.brand_new_text(&mut colour, &mut motion);
                let mut body = Body::Text { text: words.clone(), size: 96.0, colour, x: 0.5, y: 0.5, motion };
                change(&mut body, input)?;
                let id = self.project.new_id();
                self.project.items.push(Item { id, row: ROW_FX2, at, len, body });
                self.select_only(id);
                Ok((json!({ "id": id }).to_string(), Some(format!("Added \"{}\"", short_text(&words)))))
            }
            "edit_items" => {
                let ids = self.targets(input)?;
                let shift = num("shift").map(|s| (s * self.project.fps).round() as i64);
                let (start, length) = (num("start").map(|s| self.project.frames(s.max(0.0))), num("duration").map(|s| self.project.frames(s.max(0.05)).max(2)));
                let mut n = 0;
                for t in self.project.items.iter_mut().filter(|t| ids.contains(&t.id)) {
                    change(&mut t.body, input)?;
                    if let Some(d) = shift {
                        t.at = (t.at as i64 + d).max(0) as u32;
                    }
                    if let Some(s) = start {
                        t.at = s;
                    }
                    if let Some(l) = length {
                        t.len = l;
                    }
                    n += 1;
                }
                Ok((json!({ "changed": n }).to_string(), Some(format!("Changed {}", count(n, "item")))))
            }
            "delete_items" => {
                let ids = self.targets(input)?;
                let before = self.project.items.len();
                self.project.items.retain(|t| !ids.contains(&t.id));
                let n = before - self.project.items.len();
                self.picked = None;
                Ok((json!({ "deleted": n }).to_string(), Some(format!("Removed {}", count(n, "item")))))
            }
            "select_items" => {
                let ids = self.targets(input)?;
                let n = ids.len();
                self.select_these(ids);
                Ok((json!({ "selected": n }).to_string(), Some(format!("Selected {}", count(n, "item")))))
            }
            "add_effect" => {
                let kind = pick(text("effect").unwrap_or(""), &Fx::ALL, Fx::name).ok_or("no effect by that name")?;
                let at = self.project.frames(num("start").unwrap_or(self.project.seconds(self.head)).max(0.0));
                let len = self.project.frames(num("duration").unwrap_or(crate::timeline::ITEM_SECONDS).max(0.1)).max(2);
                let amount = num("amount").unwrap_or(1.0).clamp(0.0, 1.0) as f32;
                let row = if matches!(kind, Fx::BlockFlashes | Fx::Strobe) { ROW_FX } else { ROW_FX2 };
                let id = self.project.new_id();
                self.project.items.push(Item { id, row, at, len, body: Body::Effect { kind, amount } });
                Ok((json!({ "id": id }).to_string(), Some(format!("Added {}", kind.name()))))
            }
            "set_look" => {
                let want = text("look").unwrap_or("").to_lowercase();
                let grade = self.builtins.iter().find(|b| b.name.to_lowercase() == want).map(|b| b.grade.clone()).or_else(|| self.user.iter().find(|u| u.name.to_lowercase() == want).map(|u| u.grade.clone())).ok_or("no look by that name")?;
                let ids = self.clip_targets(input);
                for id in &ids {
                    self.change_grade(*id, |g| {
                        // Framing, masks and the camera format belong to the picture, not the look.
                        let mut new = grade.clone();
                        new.frame_ratio = g.frame_ratio;
                        new.crop = g.crop.clone();
                        new.cst = g.cst;
                        new.layers = std::mem::take(&mut g.layers);
                        *g = new;
                    });
                }
                Ok((json!({ "clips": ids.len() }).to_string(), Some(format!("Gave {} the {} look", count(ids.len(), "clip"), text("look").unwrap_or("")))))
            }
            "adjust_colour" => {
                let ids = self.clip_targets(input);
                let get = |k: &str, lo: f64, hi: f64| num(k).map(|v| v.clamp(lo, hi) as f32);
                let (exposure, contrast, saturation, temp, tint) = (get("exposure", -3.0, 3.0), get("contrast", 0.3, 2.0), get("saturation", 0.0, 2.0), get("temperature", -3000.0, 3000.0), get("tint", -100.0, 100.0));
                for id in &ids {
                    self.change_grade(*id, |g| {
                        if let Some(v) = exposure {
                            g.exposure = v;
                        }
                        if let Some(v) = contrast {
                            g.contrast = v;
                        }
                        if let Some(v) = saturation {
                            g.saturation = v;
                        }
                        if let Some(v) = temp {
                            g.temp = v;
                        }
                        if let Some(v) = tint {
                            g.tint = v;
                        }
                    });
                }
                Ok((json!({ "clips": ids.len() }).to_string(), Some(format!("Adjusted the colour of {}", count(ids.len(), "clip")))))
            }
            "add_transition" => {
                let id = input.get("clip_id").and_then(Value::as_u64).ok_or("clip_id is needed")? as u32;
                let i = self.project.clips.iter().position(|c| c.id == id).ok_or("no clip with that id")?;
                if i == 0 {
                    return Err("the first clip has nothing before it; name the clip the transition leads into".into());
                }
                let kind = pick(text("transition").unwrap_or("Cross Dissolve"), &Wipe::ALL, Wipe::name).ok_or("no transition by that name")?;
                let len = self.project.frames(num("seconds").unwrap_or(1.0).clamp(0.1, 3.0)).max(2);
                if !self.project.set_transition(i, kind, len) {
                    return Err("the clips on each side are too short for that transition".into());
                }
                Ok(("done".into(), Some(format!("Added {} before clip {}", kind.name(), i + 1))))
            }
            "set_aspect_ratio" => {
                let want = text("ratio").unwrap_or("");
                let (label, size) = RATIOS.iter().find(|r| r.0 == want).copied().ok_or("unknown ratio")?;
                self.project.size = size;
                if let Some(f) = input.get("fill").and_then(Value::as_bool) {
                    self.project.fill = f;
                }
                let how = if self.project.fill { "filled" } else { "fitted with bars" };
                Ok((json!({ "size": size }).to_string(), Some(format!("Changed the shape to {label}, {how}"))))
            }
            "apply_brand" => {
                let want = text("brand").map(str::to_lowercase);
                let b = self.edit.brand.brands.iter().find(|b| want.as_ref().map_or(b.id == self.project.brand, |w| b.name.to_lowercase() == *w)).or_else(|| (self.edit.brand.brands.len() == 1).then(|| &self.edit.brand.brands[0])).map(|b| b.id.clone());
                let id = b.ok_or("name one of the brand kits listed in get_project")?;
                let m = self.apply_brand(&id)?;
                Ok((m.clone(), Some(m)))
            }
            "generate_captions" => {
                let m = self.listen_for(input.get("whole").and_then(Value::as_bool).unwrap_or(true))?;
                Ok((m.clone(), Some(m)))
            }
            "seek" => {
                let f = self.project.frames(num("seconds").unwrap_or(0.0).max(0.0));
                self.seek_head(f);
                Ok(("done".into(), None))
            }
            "add_clip" => {
                let media = input.get("media_id").and_then(Value::as_u64).ok_or("media_id is needed")? as u32;
                let m = self.project.media_of(media).cloned().ok_or("no media with that id")?;
                if m.kind == crate::timeline::Kind::Audio {
                    return Err("that is sound; clips on the main track are pictures".into());
                }
                let slot = match input.get("position").and_then(Value::as_str) {
                    Some("start") => 0,
                    Some("playhead") => self.project.slot_at(self.head),
                    _ => match input.get("after_clip_id").and_then(Value::as_u64) {
                        Some(id) => self.project.clips.iter().position(|c| c.id == id as u32).map_or(self.project.clips.len(), |i| i + 1),
                        None => self.project.clips.len(),
                    },
                };
                self.sync_active();
                let i = self.project.insert_clip(media, slot).ok_or("that couldn't go on the timeline")?;
                let from = num("source_start").unwrap_or(0.0).clamp(0.0, (m.seconds - 0.1).max(0.0));
                let id = self.project.clips[i].id;
                if from > 0.0 {
                    let d = (from * self.project.fps).round() as i64;
                    self.project.trim_clip_start(id, d);
                }
                if let Some(d) = num("duration") {
                    let want = self.project.frames(d.max(0.1)) as i64;
                    let now = self.project.clips[i].len as i64;
                    self.project.trim_clip_end(id, want - now);
                }
                self.needs_activate = true;
                Ok((json!({ "clip_id": id, "index": i }).to_string(), Some(format!("Placed {} on the timeline", m.name))))
            }
            "split" => {
                self.sync_active();
                let f = self.project.frames(num("seconds").ok_or("seconds is needed")?.max(0.0));
                let i = self.project.split_clip(f).ok_or("there's no clip there, or the cut is too near an end")?;
                self.needs_activate = true;
                Ok((json!({ "new_clip_id": self.project.clips[i].id }).to_string(), Some("Split a clip".into())))
            }
            "remove_range" => {
                let (a, b) = (num("start").ok_or("start is needed")?, num("end").ok_or("end is needed")?);
                if b <= a {
                    return Err("end must come after start".into());
                }
                self.sync_active();
                let (fa, fb) = (self.project.frames(a.max(0.0)), self.project.frames(b));
                // The later cut first, so the earlier one's place doesn't move.
                self.project.split_clip(fb);
                self.project.split_clip(fa);
                let starts = self.project.starts();
                let gone: Vec<u32> = self.project.clips.iter().zip(&starts).filter(|(c, s)| **s + 1 >= fa && **s + c.len <= fb + 1).map(|(c, _)| c.id).collect();
                for id in &gone {
                    self.project.delete(Picked::Clip(*id));
                }
                self.needs_activate = true;
                Ok((json!({ "removed_clips": gone.len() }).to_string(), Some(format!("Cut out {:.1} seconds", b - a))))
            }
            "set_cut" => {
                let id = input.get("clip_id").and_then(Value::as_u64).ok_or("clip_id is needed")? as u32;
                let i = self.project.clips.iter().position(|c| c.id == id).ok_or("no clip with that id")?;
                if i == 0 {
                    return Err("the first clip has no cut before it; name the clip after the cut".into());
                }
                let secs = num("seconds").unwrap_or(0.5).clamp(0.0, 3.0);
                let kind = text("kind").unwrap_or("hard");
                match kind {
                    "hard" => {
                        self.project.clips[i].transition = None;
                        self.project.clips[i].lead = 0.0;
                        self.project.clips[i - 1].tail = 0.0;
                    }
                    "j" => self.project.clips[i].lead = secs,
                    "l" => self.project.clips[i - 1].tail = secs,
                    "smash" => self.smash_cut(i),
                    other => return Err(format!("no cut called {other}")),
                }
                Ok(("done".into(), Some(format!("Made a {} cut before clip {}", kind.to_uppercase(), i + 1))))
            }
            "jump_cut" => {
                self.seek_head(self.project.frames(num("at").ok_or("at is needed")?.max(0.0)));
                self.edit.cuts.punch = input.get("punch_in").and_then(Value::as_bool).unwrap_or(true);
                if let Some(s) = num("skip") {
                    self.edit.cuts.jump = s.clamp(0.05, 5.0) as f32;
                }
                let before = self.project.clips.len();
                self.jump_cut();
                if self.project.clips.len() == before {
                    return Err("couldn't cut there; it must be inside a clip, away from its ends".into());
                }
                Ok(("done".into(), Some("Made a jump cut".into())))
            }
            "add_cutaway" => {
                let media = input.get("media_id").and_then(Value::as_u64).ok_or("media_id is needed")? as u32;
                let at = self.project.frames(num("start").unwrap_or(self.project.seconds(self.head)).max(0.0));
                let id = self.add_cutaway(media, at, num("duration").unwrap_or(2.0).max(0.2), num("source_start").unwrap_or(0.0))?;
                Ok((json!({ "item_id": id }).to_string(), Some("Added a B-roll cutaway".into())))
            }
            "set_speed" => {
                let id = input.get("clip_id").and_then(Value::as_u64).ok_or("clip_id is needed")? as u32;
                self.project.clips.iter().find(|c| c.id == id).ok_or("no clip with that id")?;
                match text("ramp") {
                    Some("none") | None => {
                        self.project.set_ramp(id, Vec::new());
                        if let Some(s) = num("speed") {
                            self.project.set_speed(id, s);
                        }
                    }
                    Some(r) => {
                        let (_, pts) = crate::timeline::RAMPS.iter().find(|x| x.0.eq_ignore_ascii_case(r)).ok_or("no ramp by that name")?;
                        self.project.set_ramp(id, pts.iter().map(|(at, speed)| crate::timeline::RampPoint { at: *at, speed: *speed }).collect());
                    }
                }
                self.needs_activate = true;
                Ok(("done".into(), Some("Changed a clip's speed".into())))
            }
            "set_punch_in" => {
                let id = input.get("clip_id").and_then(Value::as_u64).ok_or("clip_id is needed")? as u32;
                let c = self.project.clips.iter_mut().find(|c| c.id == id).ok_or("no clip with that id")?;
                c.zoom = num("zoom").unwrap_or(1.15).clamp(1.0, 2.0) as f32;
                Ok(("done".into(), Some("Punched in".into())))
            }
            "add_template" => {
                let list = super::templates::templates();
                let want = text("template").unwrap_or("");
                let t = list.iter().find(|t| t.name.eq_ignore_ascii_case(want)).ok_or("no template by that name")?;
                let words: Option<Vec<String>> = input.get("words").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect());
                let at = self.project.frames(num("start").unwrap_or(self.project.seconds(self.head)).max(0.0));
                let centre = (num("x").unwrap_or(0.5).clamp(0.0, 1.0) as f32, num("y").unwrap_or(0.5).clamp(0.0, 1.0) as f32);
                let n = self.place_template(t, words.as_deref(), at, centre);
                Ok((json!({ "items": self.selection(), "parts": n }).to_string(), Some(format!("Added the {} template", t.name))))
            }
            _ => Err(format!("there is no tool called {name}")),
        }
    }

    /// Which items a call means: by id, or every caption, text, the selection or all.
    fn targets(&self, input: &Value) -> Result<Vec<u32>, String> {
        let items = &self.project.items;
        let ids: Vec<u32> = match input.get("target").and_then(Value::as_str).unwrap_or("ids") {
            "captions" => items.iter().filter(|t| super::captions::is_caption(t)).map(|t| t.id).collect(),
            "all_text" => items.iter().filter(|t| matches!(t.body, Body::Text { .. })).map(|t| t.id).collect(),
            "selected" => self.selection(),
            "all" => items.iter().map(|t| t.id).collect(),
            _ => input.get("ids").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64).map(|v| v as u32).filter(|id| items.iter().any(|t| t.id == *id)).collect(),
        };
        if ids.is_empty() { Err("nothing matched; check the ids with get_project".into()) } else { Ok(ids) }
    }

    /// Clips by id, or every clip when none are named.
    fn clip_targets(&self, input: &Value) -> Vec<u32> {
        let named: Vec<u32> = input.get("clip_ids").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64).map(|v| v as u32).filter(|id| self.project.clips.iter().any(|c| c.id == *id)).collect();
        if named.is_empty() { self.project.clips.iter().map(|c| c.id).collect() } else { named }
    }

    /// Changes a clip's grade. The clip open in the editor keeps its grade in the editor,
    /// so that one is changed there.
    pub(super) fn change_grade(&mut self, id: u32, f: impl Fn(&mut crate::pipeline::Grade)) {
        if self.active == Some(id) {
            f(&mut self.grade);
        }
        if let Some(c) = self.project.clips.iter_mut().find(|c| c.id == id) {
            let mut g = (*c.grade).clone();
            f(&mut g);
            c.grade = g.into();
        }
    }

    /// The project as the assistant reads it.
    fn describe(&self) -> Value {
        let p = &self.project;
        let secs = |f: u32| (p.seconds(f) * 100.0).round() / 100.0;
        let index = self.timeline_index();
        let clips: Vec<Value> = p
            .clips
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let name = p.media_of(c.media).map(|m| m.name.clone()).unwrap_or_default();
                json!({
                    "id": c.id, "media_id": c.media, "name": name, "start": secs(index.starts[i]), "duration": secs(c.len),
                    "source_start": (c.start * 100.0).round() / 100.0, "speed": c.speed, "ramped": c.ramp.len() >= 2,
                    "punch_in": c.zoom, "j_cut": c.lead, "l_cut": c.tail, "transition_in": c.transition.map(|t| t.kind.name()),
                })
            })
            .collect();
        let media: Vec<Value> = p
            .media
            .iter()
            .map(|m| json!({ "id": m.id, "name": m.name, "kind": format!("{:?}", m.kind).to_lowercase(), "seconds": (m.seconds * 100.0).round() / 100.0, "has_sound": m.has_audio, "width": m.size.0, "height": m.size.1 }))
            .collect();
        let items: Vec<Value> = p.items.iter().take(MOST_ITEMS).map(|t| describe_item(t, &secs)).collect();
        let brand = self.project_brand().map(|b| {
            json!({ "name": b.name, "colours": b.colours.iter().map(|c| hex(*c)).collect::<Vec<_>>(), "title_colour": hex(b.title_colour), "heading_font": b.heading_font, "body_font": b.body_font, "caption_font": b.captions.font, "voice": b.voice, "has_logo": !b.logo.is_empty() })
        });
        let ratio = RATIOS.iter().find(|r| r.1 == p.size).map(|r| r.0);
        json!({
            "picture": { "width": p.size.0, "height": p.size.1, "ratio": ratio, "fill": p.fill },
            "fps": p.fps,
            "duration": secs(p.total()),
            "playhead": secs(self.head),
            "clips": clips,
            "media_bin": media,
            "items": items,
            "items_not_listed": p.items.len().saturating_sub(MOST_ITEMS),
            "selected": self.selection(),
            "brand_kit": brand,
            "brand_kits_available": self.edit.brand.brands.iter().map(|b| b.name.clone()).collect::<Vec<_>>(),
            "picked_clip": match self.picked { Some(Picked::Clip(id)) => Some(id), _ => None },
        })
    }
}

fn describe_item(t: &Item, secs: &dyn Fn(u32) -> f64) -> Value {
    let mut v = json!({ "id": t.id, "start": secs(t.at), "end": secs(t.end()) });
    let o = v.as_object_mut().expect("an object");
    let mut put = |k: &str, x: Value| {
        o.insert(k.into(), x);
    };
    match &t.body {
        Body::Text { text, size, colour, x, y, motion } => {
            put("kind", json!(if motion.group == super::captions::GROUP { "caption" } else { "text" }));
            put("text", json!(text));
            put("size", json!(size.round()));
            put("colour", json!(hex(*colour)));
            put("x", json!(x));
            put("y", json!(y));
            put("font", json!(if motion.font.is_empty() { "app default" } else { motion.font.as_str() }));
            put("weight", json!(motion.weight));
            put("outline", json!(motion.outline));
            put("animation_in", json!(motion.enter.kind.name()));
            put("animation_out", json!(motion.leave.kind.name()));
            put("gradient", json!(motion.gradient.map(hex)));
            put("highlight", json!(hex(motion.highlight)));
            put("backing", json!(motion.backing.name()));
            put("backing_colour", json!(hex(motion.backing_colour)));
        }
        Body::Sticker { x, y, .. } => {
            put("kind", json!("sticker"));
            put("x", json!(x));
            put("y", json!(y));
        }
        Body::Graphic { name, x, y, motion, .. } => {
            put("kind", json!(if motion.group == super::brand::LOGO_GROUP { "brand logo" } else { "design layer" }));
            put("name", json!(name));
            put("x", json!(x));
            put("y", json!(y));
        }
        Body::Overlay { x, y, .. } => {
            put("kind", json!("video over the picture"));
            put("x", json!(x));
            put("y", json!(y));
        }
        Body::Effect { kind, amount } => {
            put("kind", json!("effect"));
            put("effect", json!(kind.name()));
            put("amount", json!(amount));
        }
        Body::Audio { name, volume, .. } => {
            put("kind", json!("sound"));
            put("name", json!(name));
            put("volume", json!(volume));
        }
    }
    v
}

/// Applies the settings a call names to one item. Each only touches what it applies to.
fn change(body: &mut Body, input: &Value) -> Result<(), String> {
    let num = |k: &str| input.get(k).and_then(Value::as_f64).map(|v| v as f32);
    let text = |k: &str| input.get(k).and_then(Value::as_str);
    let font = match text("font") {
        Some(f) if f.eq_ignore_ascii_case("app default") || f.is_empty() => Some(String::new()),
        Some(f) => Some(crate::overlay::font_families().into_iter().find(|x| x.eq_ignore_ascii_case(f)).ok_or_else(|| format!("no font called {f}; use find_fonts"))?),
        None => None,
    };
    let colour = match text("colour") {
        Some(c) => Some(parse_hex(c).ok_or_else(|| format!("{c} isn't a hex colour"))?),
        None => None,
    };
    let anim = |k: &str| -> Result<Option<Anim>, String> {
        match text(k) {
            Some(n) => pick(n, &Anim::ALL, Anim::name).map(Some).ok_or_else(|| format!("no animation called {n}")),
            None => Ok(None),
        }
    };
    let (enter, leave) = (anim("animation_in")?, anim("animation_out")?);
    let sway = match text("loop") {
        Some(n) => Some(pick(n, &Sway::ALL, Sway::name).ok_or_else(|| format!("no loop called {n}"))?),
        None => None,
    };
    let hex_or = |k: &str| -> Result<Option<[f32; 3]>, String> {
        match text(k) {
            Some(c) => parse_hex(c).map(Some).ok_or_else(|| format!("{c} isn't a hex colour")),
            None => Ok(None),
        }
    };
    let gradient = match text("gradient") {
        Some("none") => Some(None),
        Some(_) => Some(hex_or("gradient")?),
        None => None,
    };
    let (highlight, backing_colour) = (hex_or("highlight")?, hex_or("backing_colour")?);
    let backing = match text("backing") {
        Some(n) => Some(pick(n, &Backing::ALL, Backing::name).ok_or_else(|| format!("no background shape called {n}"))?),
        None => None,
    };
    if let Body::Text { text: words, size, colour: c, .. } = body {
        if let Some(t) = text("text") {
            *words = t.to_string();
        }
        if let Some(s) = num("size") {
            *size = s.clamp(12.0, 400.0);
        }
        if let Some(v) = colour {
            *c = v;
        }
    }
    match body {
        Body::Effect { amount, .. } => {
            if let Some(a) = num("amount") {
                *amount = a.clamp(0.0, 1.0);
            }
        }
        Body::Audio { volume, .. } | Body::Overlay { volume, .. } if num("volume").is_some() => *volume = num("volume").unwrap_or(1.0).clamp(0.0, 2.0),
        _ => {}
    }
    if let Some((x, y, m)) = body.placed_mut() {
        if let Some(v) = num("x") {
            *x = v.clamp(-0.5, 1.5);
        }
        if let Some(v) = num("y") {
            *y = v.clamp(-0.5, 1.5);
        }
        if let Some(f) = font {
            m.font = f;
        }
        if let Some(w) = num("weight") {
            m.weight = (w.clamp(100.0, 900.0) / 100.0).round() as u16 * 100;
        }
        if let Some(o) = num("outline") {
            m.outline = o.clamp(0.0, 0.2);
        }
        if let Some(o) = num("opacity") {
            m.opacity = o.clamp(0.0, 1.0);
        }
        if let Some(a) = enter {
            m.enter = Phase::preset(a);
        }
        if let Some(a) = leave {
            m.leave = Phase::preset(a);
        }
        if let Some(s) = sway {
            m.repeat = Repeat::preset(s);
        }
        if let Some(g) = gradient {
            m.gradient = g;
        }
        if let Some(h) = highlight {
            m.highlight = h;
        }
        if let Some(b) = backing {
            m.backing = b;
        }
        if let Some(c) = backing_colour {
            m.backing_colour = c;
        }
    }
    Ok(())
}

/// A name from a list, matched without regard to case.
fn pick<T: Copy>(name: &str, all: &[T], name_of: fn(T) -> &'static str) -> Option<T> {
    all.iter().copied().find(|v| name_of(*v).eq_ignore_ascii_case(name.trim()))
}

fn count(n: usize, what: &str) -> String {
    if n == 1 { format!("1 {what}") } else { format!("{n} {what}s") }
}

fn short_text(t: &str) -> String {
    let one = t.lines().next().unwrap_or("");
    if one.chars().count() > 40 { format!("{}…", one.chars().take(40).collect::<String>()) } else { one.to_string() }
}

/// The tools, with the app's own names for looks, effects, transitions and animations.
fn tools(looks: &[String]) -> Value {
    let names = |v: Vec<&str>| json!(v);
    let anims = names(Anim::ALL.iter().map(|a| a.name()).collect());
    let loops = names(Sway::ALL.iter().map(|a| a.name()).collect());
    let target = json!({ "type": "string", "enum": ["ids", "captions", "all_text", "selected", "all"], "description": "Which items: those in ids, every caption, every text, what the person selected, or everything." });
    let ids = json!({ "type": "array", "items": { "type": "integer" }, "description": "Item ids from get_project, when target is ids." });
    let looks_text = json!({ "type": "object", "additionalProperties": false, "properties": {
        "text": { "type": "string", "description": "New words. Only for one item at a time." },
        "font": { "type": "string", "description": "A font family name from find_fonts or the brand kit, or \"app default\"." },
        "size": { "type": "number", "description": "Text size in points, 24 to 320." },
        "colour": { "type": "string", "description": "Hex colour, like #FFFFFF." },
        "weight": { "type": "number", "description": "400 regular, 600 semibold, 700 bold, 900 black." },
        "outline": { "type": "number", "description": "Dark edge round letters, 0 to 0.15 of the text height." },
        "x": { "type": "number" }, "y": { "type": "number" },
        "opacity": { "type": "number", "description": "0 to 1." },
        "animation_in": { "type": "string", "enum": anims },
        "animation_out": { "type": "string", "enum": anims },
        "loop": { "type": "string", "enum": loops },
        "gradient": { "type": "string", "description": "A second hex colour the letters blend into, top to bottom, or \"none\"." },
        "highlight": { "type": "string", "description": "Hex colour for words written between *stars* in the text; they are drawn bold too." },
        "backing": { "type": "string", "enum": Backing::ALL.iter().map(|b| b.name()).collect::<Vec<_>>(), "description": "A shape behind the text." },
        "backing_colour": { "type": "string", "description": "Hex colour of the shape behind the text." },
    }});
    let mut edit_props = looks_text["properties"].clone();
    let extra = json!({
        "target": target, "ids": ids,
        "start": { "type": "number", "description": "Move every one to start here, in seconds." },
        "shift": { "type": "number", "description": "Move every one by this many seconds, later if positive." },
        "duration": { "type": "number", "description": "New length in seconds." },
        "amount": { "type": "number", "description": "An effect's strength, 0 to 1." },
        "volume": { "type": "number", "description": "A sound's volume, 1 is as recorded." },
    });
    for (k, v) in extra.as_object().into_iter().flatten() {
        edit_props[k] = v.clone();
    }
    let mut add_props = looks_text["properties"].clone();
    add_props["start"] = json!({ "type": "number", "description": "Seconds; the playhead if left out." });
    add_props["duration"] = json!({ "type": "number", "description": "Seconds; 4 if left out." });
    let clip_ids = json!({ "type": "array", "items": { "type": "integer" }, "description": "Clip ids from get_project; leave out for every clip." });
    json!([
        { "name": "get_project", "description": "The open project: picture size and shape, clips in order, every item (text, captions, stickers, effects, design layers, the brand logo, sounds) with ids and times, the selection, the playhead, and the brand kit in use with its colours, fonts and voice.", "input_schema": { "type": "object", "properties": {}, "additionalProperties": false } },
        { "name": "find_fonts", "description": "Font families on this computer whose names contain the words given.", "input_schema": { "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"], "additionalProperties": false } },
        { "name": "add_text", "description": "Adds a title or other text over the picture. With a brand kit in use it starts in the kit's heading font and title colour.", "input_schema": { "type": "object", "properties": add_props, "required": ["text"], "additionalProperties": false } },
        { "name": "edit_items", "description": "Changes items: text, captions, stickers, design layers, effects or sounds. Only the settings given change; each item keeps its own words unless text is given.", "input_schema": { "type": "object", "properties": edit_props, "required": ["target"], "additionalProperties": false } },
        { "name": "delete_items", "description": "Removes items from the timeline.", "input_schema": { "type": "object", "properties": { "target": target, "ids": ids }, "required": ["target"], "additionalProperties": false } },
        { "name": "select_items", "description": "Selects items so the person can see and edit them together.", "input_schema": { "type": "object", "properties": { "target": target, "ids": ids }, "required": ["target"], "additionalProperties": false } },
        { "name": "add_effect", "description": "Adds an effect over a stretch of the timeline.", "input_schema": { "type": "object", "properties": { "effect": { "type": "string", "enum": names(Fx::ALL.iter().map(|f| f.name()).collect()) }, "start": { "type": "number" }, "duration": { "type": "number" }, "amount": { "type": "number", "description": "0 to 1." } }, "required": ["effect"], "additionalProperties": false } },
        { "name": "set_look", "description": "Gives clips one of the app's colour looks, replacing their colour settings. Framing and masks stay.", "input_schema": { "type": "object", "properties": { "look": { "type": "string", "enum": looks }, "clip_ids": clip_ids }, "required": ["look"], "additionalProperties": false } },
        { "name": "adjust_colour", "description": "Sets basic colour on clips. Only the values given change. exposure is stops, -3 to 3 (0 none); contrast 0.3 to 2 (1 none); saturation 0 to 2 (1 none, 0 black and white); temperature -3000 to 3000, warmer when positive (0 none); tint -100 to 100, more magenta when positive.", "input_schema": { "type": "object", "properties": { "clip_ids": clip_ids, "exposure": { "type": "number" }, "contrast": { "type": "number" }, "saturation": { "type": "number" }, "temperature": { "type": "number" }, "tint": { "type": "number" } }, "additionalProperties": false } },
        { "name": "add_transition", "description": "Puts a transition on the cut into a clip.", "input_schema": { "type": "object", "properties": { "clip_id": { "type": "integer", "description": "The clip the transition leads into; not the first clip." }, "transition": { "type": "string", "enum": names(Wipe::ALL.iter().map(|w| w.name()).collect()) }, "seconds": { "type": "number" } }, "required": ["clip_id"], "additionalProperties": false } },
        { "name": "set_aspect_ratio", "description": "Changes the picture's shape. 9:16 suits Reels, TikTok and Shorts; 1:1 and 4:5 suit feeds; 16:9 suits YouTube. With fill, clips are enlarged to fill the frame and their edges cropped; without it they fit inside with bars.", "input_schema": { "type": "object", "properties": { "ratio": { "type": "string", "enum": RATIOS.iter().map(|r| r.0).collect::<Vec<_>>() }, "fill": { "type": "boolean" } }, "required": ["ratio"], "additionalProperties": false } },
        { "name": "apply_brand", "description": "Puts a brand kit on the project: its logo as a watermark, its look on every caption and its heading font on every title. New text then follows the kit.", "input_schema": { "type": "object", "properties": { "brand": { "type": "string", "description": "A kit's name; the one in use if left out." } }, "additionalProperties": false } },
        { "name": "generate_captions", "description": "Starts Auto Captions, which listens to the sound on this computer and adds captions. It runs in the background for about as long as the video; the captions appear when it finishes.", "input_schema": { "type": "object", "properties": { "whole": { "type": "boolean", "description": "The whole timeline (default), or only the clip at the playhead." } }, "additionalProperties": false } },
        { "name": "seek", "description": "Moves the playhead, so the person sees that moment.", "input_schema": { "type": "object", "properties": { "seconds": { "type": "number" } }, "required": ["seconds"], "additionalProperties": false } },
        { "name": "add_clip", "description": "Puts a video or photo from the media bin on the main track, the A-roll. Choose the stretch of the source with source_start and duration. Place it at the end (default), the start, the playhead, or after a clip.", "input_schema": { "type": "object", "properties": {
            "media_id": { "type": "integer" }, "source_start": { "type": "number", "description": "Seconds into the source where the clip begins." }, "duration": { "type": "number", "description": "Seconds; the rest of the source if left out." },
            "position": { "type": "string", "enum": ["end", "start", "playhead"] }, "after_clip_id": { "type": "integer", "description": "Place right after this clip." } }, "required": ["media_id"], "additionalProperties": false } },
        { "name": "split", "description": "Cuts the main-track clip at a moment in two.", "input_schema": { "type": "object", "properties": { "seconds": { "type": "number" } }, "required": ["seconds"], "additionalProperties": false } },
        { "name": "remove_range", "description": "Cuts a stretch out of the main track, from start to end in timeline seconds, and closes the gap. For removing pauses, mistakes or dead air.", "input_schema": { "type": "object", "properties": { "start": { "type": "number" }, "end": { "type": "number" } }, "required": ["start", "end"], "additionalProperties": false } },
        { "name": "set_cut", "description": "How the cut into a clip plays. hard: straight cut, nothing else. j: the clip's sound starts `seconds` before its picture. l: the previous clip's sound runs on `seconds` into this one. smash: a white flash and a jolt of shake on the cut.", "input_schema": { "type": "object", "properties": { "clip_id": { "type": "integer", "description": "The clip after the cut." }, "kind": { "type": "string", "enum": ["hard", "j", "l", "smash"] }, "seconds": { "type": "number" } }, "required": ["clip_id", "kind"], "additionalProperties": false } },
        { "name": "jump_cut", "description": "Splits the shot at a moment and drops the next stretch of it, so time jumps forward; with punch_in the new part is enlarged, as in talking-head edits.", "input_schema": { "type": "object", "properties": { "at": { "type": "number", "description": "Timeline seconds." }, "skip": { "type": "number", "description": "Seconds dropped; 0.5 if left out." }, "punch_in": { "type": "boolean" } }, "required": ["at"], "additionalProperties": false } },
        { "name": "add_cutaway", "description": "B-roll: lays another video from the media bin over the main shot, full frame and silent, while the main shot's sound carries on. For cutaways and cross-cutting.", "input_schema": { "type": "object", "properties": { "media_id": { "type": "integer" }, "start": { "type": "number", "description": "Timeline seconds; the playhead if left out." }, "duration": { "type": "number" }, "source_start": { "type": "number", "description": "Seconds into the B-roll source." } }, "required": ["media_id"], "additionalProperties": false } },
        { "name": "set_speed", "description": "Sets a clip's speed, or gives it a speed ramp that speeds up and slows down within it. A ramped clip plays without its own sound.", "input_schema": { "type": "object", "properties": { "clip_id": { "type": "integer" }, "speed": { "type": "number", "description": "0.1 to 8; 1 is normal. Ignored with a ramp." }, "ramp": { "type": "string", "enum": std::iter::once("none").chain(crate::timeline::RAMPS.iter().map(|r| r.0)).collect::<Vec<_>>() } }, "required": ["clip_id"], "additionalProperties": false } },
        { "name": "set_punch_in", "description": "Enlarges a clip's picture from its centre, 1 to 2 (1.15 is a typical punch-in).", "input_schema": { "type": "object", "properties": { "clip_id": { "type": "integer" }, "zoom": { "type": "number" } }, "required": ["clip_id"], "additionalProperties": false } },
        { "name": "add_template", "description": "Adds a ready-made title layout made of text items: Marker Title (bold word on a brush stroke and a tag), Callout (framed title and a label), Two-Tone Headline (second line in colour), Big Word (huge gradient word), Pill Tags (three words in pills), Lower Third (name and role), Stacked Statement (short lines rising, last one lit), Underlined Note. words replaces each part's words in order; put *stars* round words to highlight them.", "input_schema": { "type": "object", "properties": {
            "template": { "type": "string", "enum": super::templates::templates().iter().map(|t| t.name).collect::<Vec<_>>() }, "words": { "type": "array", "items": { "type": "string" } },
            "start": { "type": "number" }, "x": { "type": "number" }, "y": { "type": "number" } }, "required": ["template"], "additionalProperties": false } },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Body {
        Body::Text { text: s.into(), size: 64.0, colour: [1.0; 3], x: 0.5, y: 0.86, motion: Default::default() }
    }

    #[test]
    fn a_change_touches_only_what_it_names() {
        let mut b = text("Hello");
        change(&mut b, &json!({ "colour": "#FFD21A", "weight": 900, "animation_in": "pop" })).unwrap();
        let Body::Text { text, size, colour, motion, .. } = b else { panic!() };
        assert_eq!((text.as_str(), size), ("Hello", 64.0));
        assert_eq!(hex(colour), "#FFD21A");
        assert_eq!((motion.weight, motion.enter.kind), (900, Anim::Pop));
    }

    #[test]
    fn bad_names_come_back_as_errors_not_guesses() {
        let mut b = text("Hi");
        assert!(change(&mut b, &json!({ "colour": "yellowish" })).is_err());
        assert!(change(&mut b, &json!({ "animation_in": "zoomy" })).is_err());
        assert_eq!(b, text("Hi"));
    }

    #[test]
    fn every_tool_has_a_schema_the_api_accepts() {
        let t = tools(&["Warm".into()]);
        for tool in t.as_array().unwrap() {
            assert!(tool["name"].is_string() && tool["description"].is_string(), "{tool}");
            assert_eq!(tool["input_schema"]["type"], "object", "{tool}");
        }
        assert!(t.as_array().unwrap().iter().any(|x| x["name"] == "set_aspect_ratio"));
    }
}
