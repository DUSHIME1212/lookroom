//! Keyboard shortcuts. One table drives both the keys and the help sheet, so the two
//! can't disagree. A hash map from the table finds a key's action in one step.

use super::App;
use super::edit::Rail;
use super::sheets::Sheet;
use crate::timeline::Picked;
use eframe::egui::{Context, Event, Key, Modifiers};
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Act {
    PlayPause,
    BackFrame,
    ForwardFrame,
    BackSecond,
    ForwardSecond,
    BackFive,
    ForwardFive,
    PreviousCut,
    NextCut,
    ToStart,
    ToEnd,
    Split,
    Delete,
    Duplicate,
    TrimStart,
    TrimEnd,
    DetachAudio,
    MuteClip,
    Undo,
    Redo,
    Deselect,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    Compare,
    Scopes,
    FullScreen,
    Export,
    Save,
    OpenProject,
    Import,
    AddText,
    Page(Rail),
    Help,
    AddKey,
    RemoveKey,
    PreviousKey,
    NextKey,
    Faster,
    Slower,
    SpeedPanel,
    PreviewFx,
    Snap,
    SelectAll,
}

pub struct Binding {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub keys: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    pub act: Act,
}

const fn bind(key: Key, ctrl: bool, shift: bool, keys: &'static str, group: &'static str, label: &'static str, act: Act) -> Binding {
    Binding { key, ctrl, shift, keys, group, label, act }
}

const PLAY: &str = "Playback";
const EDIT: &str = "Editing";
const VIEW: &str = "View";
const FILE: &str = "Project";
const PAGE: &str = "Pages";
const MOTION: &str = "Motion";

pub const KEYS: &[Binding] = &[
    bind(Key::Space, false, false, "Space", PLAY, "Play or pause", Act::PlayPause),
    bind(Key::K, false, false, "K", PLAY, "Play or pause", Act::PlayPause),
    bind(Key::J, false, false, "J", PLAY, "Back 5 seconds", Act::BackFive),
    bind(Key::L, false, false, "L", PLAY, "Forward 5 seconds", Act::ForwardFive),
    bind(Key::ArrowLeft, false, false, "Left", PLAY, "Back one frame", Act::BackFrame),
    bind(Key::ArrowRight, false, false, "Right", PLAY, "Forward one frame", Act::ForwardFrame),
    bind(Key::ArrowLeft, false, true, "Shift Left", PLAY, "Back one second", Act::BackSecond),
    bind(Key::ArrowRight, false, true, "Shift Right", PLAY, "Forward one second", Act::ForwardSecond),
    bind(Key::ArrowUp, false, false, "Up", PLAY, "Previous cut", Act::PreviousCut),
    bind(Key::ArrowDown, false, false, "Down", PLAY, "Next cut", Act::NextCut),
    bind(Key::Home, false, false, "Home", PLAY, "Go to the start", Act::ToStart),
    bind(Key::End, false, false, "End", PLAY, "Go to the end", Act::ToEnd),
    bind(Key::S, false, false, "S", EDIT, "Split at the playhead", Act::Split),
    bind(Key::B, true, false, "Ctrl B", EDIT, "Split at the playhead", Act::Split),
    bind(Key::Delete, false, false, "Delete", EDIT, "Delete what's selected", Act::Delete),
    bind(Key::Backspace, false, false, "Backspace", EDIT, "Delete what's selected", Act::Delete),
    bind(Key::D, true, false, "Ctrl D", EDIT, "Duplicate", Act::Duplicate),
    bind(Key::A, true, false, "Ctrl A", EDIT, "Select every item; Ctrl or Shift and a click adds one", Act::SelectAll),
    bind(Key::Q, false, false, "Q", EDIT, "Trim clip start to the playhead", Act::TrimStart),
    bind(Key::W, false, false, "W", EDIT, "Trim clip end to the playhead", Act::TrimEnd),
    bind(Key::A, false, false, "A", EDIT, "Detach audio", Act::DetachAudio),
    bind(Key::M, false, false, "M", EDIT, "Mute the clip", Act::MuteClip),
    bind(Key::T, false, false, "T", EDIT, "Add text", Act::AddText),
    bind(Key::Z, true, false, "Ctrl Z", EDIT, "Undo", Act::Undo),
    bind(Key::Z, true, true, "Ctrl Shift Z", EDIT, "Redo", Act::Redo),
    bind(Key::Y, true, false, "Ctrl Y", EDIT, "Redo", Act::Redo),
    bind(Key::Escape, false, false, "Esc", EDIT, "Clear the selection, or leave full screen", Act::Deselect),
    bind(Key::Equals, false, false, "=", VIEW, "Zoom the timeline in", Act::ZoomIn),
    bind(Key::Plus, false, false, "+", VIEW, "Zoom the timeline in", Act::ZoomIn),
    bind(Key::Minus, false, false, "-", VIEW, "Zoom the timeline out", Act::ZoomOut),
    bind(Key::Backslash, false, false, "\\", VIEW, "Fit the timeline", Act::ZoomFit),
    bind(Key::C, false, false, "C", VIEW, "Before and after", Act::Compare),
    bind(Key::H, false, false, "H", VIEW, "Scopes", Act::Scopes),
    bind(Key::F, false, false, "F", VIEW, "Full screen", Act::FullScreen),
    bind(Key::F1, false, false, "F1", VIEW, "These shortcuts", Act::Help),
    bind(Key::P, false, false, "P", VIEW, "Preview the effect or transition", Act::PreviewFx),
    bind(Key::N, false, false, "N", VIEW, "Snapping on or off", Act::Snap),
    bind(Key::Questionmark, false, false, "?", VIEW, "These shortcuts", Act::Help),
    bind(Key::E, true, false, "Ctrl E", FILE, "Export", Act::Export),
    bind(Key::S, true, false, "Ctrl S", FILE, "Save project", Act::Save),
    bind(Key::O, true, false, "Ctrl O", FILE, "Open project", Act::OpenProject),
    bind(Key::I, true, false, "Ctrl I", FILE, "Import media", Act::Import),
    bind(Key::Num1, false, false, "1", PAGE, "Media", Act::Page(Rail::Media)),
    bind(Key::Num2, false, false, "2", PAGE, "Video", Act::Page(Rail::Video)),
    bind(Key::Num3, false, false, "3", PAGE, "Photo", Act::Page(Rail::Photo)),
    bind(Key::Num4, false, false, "4", PAGE, "Audio", Act::Page(Rail::Audio)),
    bind(Key::Num5, false, false, "5", PAGE, "Text", Act::Page(Rail::Text)),
    bind(Key::Num6, false, false, "6", PAGE, "Transitions", Act::Page(Rail::Transitions)),
    bind(Key::Num7, false, false, "7", PAGE, "Effect", Act::Page(Rail::Effect)),
    bind(Key::Num8, false, false, "8", PAGE, "Sticker", Act::Page(Rail::Sticker)),
    bind(Key::Num9, false, false, "9", PAGE, "Adjustment", Act::Page(Rail::Adjustment)),
    bind(Key::Num0, false, false, "0", PAGE, "Settings", Act::Page(Rail::Settings)),
    bind(Key::K, true, false, "Ctrl K", MOTION, "Add a keyframe here", Act::AddKey),
    bind(Key::K, true, true, "Ctrl Shift K", MOTION, "Remove the keyframe here", Act::RemoveKey),
    bind(Key::OpenBracket, false, false, "[", MOTION, "Previous keyframe", Act::PreviousKey),
    bind(Key::CloseBracket, false, false, "]", MOTION, "Next keyframe", Act::NextKey),
    bind(Key::ArrowUp, true, false, "Ctrl Up", MOTION, "Play the clip faster", Act::Faster),
    bind(Key::ArrowDown, true, false, "Ctrl Down", MOTION, "Play the clip slower", Act::Slower),
    bind(Key::R, false, false, "R", MOTION, "Clip speed", Act::SpeedPanel),
];

/// The groups, in the order the help sheet shows them.
pub const GROUPS: [&str; 6] = [PLAY, EDIT, VIEW, FILE, PAGE, MOTION];

type Chord = (Key, bool, bool);

fn lookup() -> &'static HashMap<Chord, Act> {
    static MAP: OnceLock<HashMap<Chord, Act>> = OnceLock::new();
    MAP.get_or_init(|| KEYS.iter().map(|b| ((b.key, b.ctrl, b.shift), b.act)).collect())
}

fn act_for(key: Key, m: Modifiers) -> Option<Act> {
    if m.alt {
        return None;
    }
    // These two take Shift on some keyboards and not on others.
    let shift = m.shift && !matches!(key, Key::Plus | Key::Questionmark);
    lookup().get(&(key, m.command, shift)).copied()
}

/// Holding a key repeats only what's safe to repeat. Holding S shouldn't cut the clip to bits.
fn repeats(a: Act) -> bool {
    matches!(a, Act::BackFrame | Act::ForwardFrame | Act::BackSecond | Act::ForwardSecond | Act::ZoomIn | Act::ZoomOut | Act::Undo | Act::Redo)
}

impl App {
    /// Nothing fires while a text box has the keyboard, so typing stays typing.
    pub(super) fn run_keys(&mut self, ctx: &Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        // Figma's Copy as SVG puts the design's text on the clipboard.
        let pasted: Vec<String> = ctx.input(|i| i.events.iter().filter_map(|e| if let Event::Paste(s) = e { Some(s.clone()) } else { None }).collect());
        if self.sheet.is_none() {
            for s in pasted.iter().filter(|s| s.contains("<svg")) {
                self.import_design(s);
            }
        }
        let acts: Vec<Act> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    Event::Key { key, pressed: true, repeat, modifiers, .. } => act_for(*key, *modifiers).filter(|a| !repeat || repeats(*a)),
                    _ => None,
                })
                .collect()
        });
        for a in acts {
            // With a sheet open the keys belong to the sheet.
            match (&self.sheet, a) {
                (None, _) => self.perform(ctx, a),
                (Some(Sheet::Keys), Act::Help) => self.sheet = None,
                _ => {}
            }
        }
    }

    fn cut_near(&self, forward: bool) -> u32 {
        let index = self.timeline_index();
        let starts = &index.starts;
        let h = self.head;
        if forward {
            starts.get(starts.partition_point(|s| *s <= h)).copied().unwrap_or(self.project.total())
        } else {
            starts.partition_point(|s| *s < h).checked_sub(1).map_or(0, |i| starts[i])
        }
    }

    fn perform(&mut self, ctx: &Context, a: Act) {
        let second = self.project.frames(1.0);
        let five = self.project.frames(5.0);
        let h = self.head;
        match a {
            Act::PlayPause => self.play_or_pause("a key"),
            Act::BackFrame => self.seek_head(h.saturating_sub(1)),
            Act::ForwardFrame => self.seek_head(h + 1),
            Act::BackSecond => self.seek_head(h.saturating_sub(second)),
            Act::ForwardSecond => self.seek_head(h + second),
            Act::BackFive => self.seek_head(h.saturating_sub(five)),
            Act::ForwardFive => self.seek_head(h + five),
            Act::PreviousCut => self.seek_head(self.cut_near(false)),
            Act::NextCut => self.seek_head(self.cut_near(true)),
            Act::ToStart => self.seek_head(0),
            Act::ToEnd => self.seek_head(self.project.total()),
            Act::Split => self.split(),
            Act::Delete => {
                if self.delete_selection() || self.delete_clip_selection() {
                    self.needs_activate = true;
                } else if let Some(p) = self.picked.take() {
                    self.project.delete(p);
                    self.needs_activate = true;
                }
            }
            Act::Duplicate => {
                if let Some(p) = self.picked.or_else(|| self.active.map(Picked::Clip)) {
                    self.picked = self.project.duplicate(p);
                }
            }
            Act::TrimStart => self.trim_to_head(true),
            Act::TrimEnd => self.trim_to_head(false),
            Act::DetachAudio => self.detach(),
            Act::MuteClip => self.toggle_clip_mute(),
            Act::Undo => self.undo(),
            Act::Redo => self.redo(),
            Act::Deselect if self.edit.fullscreen => self.set_fullscreen(ctx, false),
            Act::Deselect => self.picked = None,
            Act::ZoomIn => self.zoom_by(1.25),
            Act::ZoomOut => self.zoom_by(0.8),
            Act::ZoomFit => self.zoom_fit(),
            Act::Compare => self.compare = !self.compare,
            Act::Scopes => self.toggle_scope(),
            Act::FullScreen => self.set_fullscreen(ctx, !self.edit.fullscreen),
            Act::Export => self.sheet = Some(Sheet::Export),
            Act::Save => self.save_project(),
            Act::OpenProject => self.open_project(),
            Act::Import => self.upload(),
            Act::AddText => self.add_title(),
            Act::Page(p) => self.edit.rail = p,
            Act::Help => self.sheet = Some(Sheet::Keys),
            Act::AddKey => self.key_here(true),
            Act::RemoveKey => self.key_here(false),
            Act::PreviousKey => self.jump_key(false),
            Act::NextKey => self.jump_key(true),
            Act::Faster => self.nudge_speed(true),
            Act::Slower => self.nudge_speed(false),
            Act::SpeedPanel => self.edit.motion.speed_open = !self.edit.motion.speed_open,
            Act::PreviewFx => self.preview_fx(),
            Act::SelectAll => self.select_all_items(),
            Act::Snap => {
                self.edit.snap = !self.edit.snap;
                self.say(if self.edit.snap { "Snapping on" } else { "Snapping off" });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_two_shortcuts_share_a_key() {
        assert_eq!(lookup().len(), KEYS.len());
    }

    #[test]
    fn every_shortcut_belongs_to_a_shown_group() {
        assert!(KEYS.iter().all(|b| GROUPS.contains(&b.group)));
    }

    #[test]
    fn the_right_action_comes_back_for_a_chord() {
        let ctrl = Modifiers { command: true, ctrl: true, ..Modifiers::NONE };
        assert_eq!(act_for(Key::S, Modifiers::NONE), Some(Act::Split));
        assert_eq!(act_for(Key::S, ctrl), Some(Act::Save));
        assert_eq!(act_for(Key::Z, ctrl), Some(Act::Undo));
        assert_eq!(act_for(Key::Z, Modifiers { shift: true, ..ctrl }), Some(Act::Redo));
    }

    #[test]
    fn plus_and_question_mark_work_with_or_without_shift() {
        let shift = Modifiers { shift: true, ..Modifiers::NONE };
        for m in [Modifiers::NONE, shift] {
            assert_eq!(act_for(Key::Plus, m), Some(Act::ZoomIn));
            assert_eq!(act_for(Key::Questionmark, m), Some(Act::Help));
        }
    }

    #[test]
    fn holding_a_key_only_repeats_safe_things() {
        assert!(repeats(Act::ForwardFrame));
        assert!(!repeats(Act::Split));
        assert!(!repeats(Act::PlayPause));
        assert!(!repeats(Act::Delete));
    }

    #[test]
    fn keyframes_and_speed_have_keys() {
        let ctrl = Modifiers { command: true, ctrl: true, ..Modifiers::NONE };
        assert_eq!(act_for(Key::K, ctrl), Some(Act::AddKey));
        assert_eq!(act_for(Key::K, Modifiers::NONE), Some(Act::PlayPause));
        assert_eq!(act_for(Key::CloseBracket, Modifiers::NONE), Some(Act::NextKey));
        assert_eq!(act_for(Key::ArrowUp, ctrl), Some(Act::Faster));
        assert_eq!(act_for(Key::ArrowUp, Modifiers::NONE), Some(Act::PreviousCut));
    }

    #[test]
    fn keys_nobody_bound_do_nothing() {
        assert_eq!(act_for(Key::G, Modifiers::NONE), None);
        assert_eq!(act_for(Key::S, Modifiers { alt: true, ..Modifiers::NONE }), None);
    }

    #[test]
    fn every_page_has_a_number_key() {
        let pages = KEYS.iter().filter(|b| matches!(b.act, Act::Page(_))).count();
        assert_eq!(pages, 10);
    }
}
