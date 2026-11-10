//! Several items selected at once. One of them is the picked item, which the panels show;
//! whatever is changed on it is copied to the rest (see `timeline::spread`). Ctrl or Shift
//! and a click adds or takes away; a plain click picks one again.

use super::App;
use super::kit::*;
use crate::timeline::{Body, Picked, spread};
use eframe::egui::{self, Color32, Rect, Sense, Ui, pos2, vec2};

impl App {
    /// Every selected item: the picked one and any chosen with it. The group only holds
    /// while the picked item is part of it, so picking anything else ends it.
    pub(super) fn selection(&self) -> Vec<u32> {
        let Some(Picked::Item(id)) = self.picked else { return Vec::new() };
        if self.edit.group.contains(&id) {
            self.edit.group.iter().copied().filter(|g| self.project.items.iter().any(|t| t.id == *g)).collect()
        } else {
            vec![id]
        }
    }

    pub(super) fn is_selected(&self, id: u32) -> bool {
        match self.picked {
            Some(Picked::Item(p)) if p == id => true,
            Some(Picked::Item(p)) => self.edit.group.contains(&p) && self.edit.group.contains(&id),
            _ => false,
        }
    }

    /// Picks one item on its own.
    pub(super) fn select_only(&mut self, id: u32) {
        self.edit.group.clear();
        self.picked = Some(Picked::Item(id));
    }

    /// Ctrl or Shift and a click: adds the item, or takes it away if it was selected.
    pub(super) fn toggle_select(&mut self, id: u32) {
        let mut all = self.selection();
        if let Some(i) = all.iter().position(|g| *g == id) {
            all.remove(i);
            // The panels follow whichever is left, last chosen first.
            self.picked = all.last().map(|g| Picked::Item(*g));
        } else {
            all.push(id);
            self.picked = Some(Picked::Item(id));
        }
        self.edit.group = all;
    }

    pub(super) fn select_these(&mut self, ids: Vec<u32>) {
        self.picked = ids.first().map(|id| Picked::Item(*id));
        self.edit.group = ids;
    }

    /// Every item on the timeline. The panel keeps showing what it showed; with nothing
    /// picked it shows the text at the playhead, or failing that the first text, so the
    /// shared settings are in view.
    pub(super) fn select_all_items(&mut self) {
        let mut ids: Vec<u32> = self.project.items.iter().map(|t| t.id).collect();
        let head = self.head;
        let is_text = |t: &&crate::timeline::Item| matches!(t.body, Body::Text { .. });
        let shown = match self.picked {
            Some(Picked::Item(id)) => Some(id),
            _ => self.project.items.iter().filter(is_text).find(|t| head >= t.at && head < t.end()).or_else(|| self.project.items.iter().find(is_text)).map(|t| t.id),
        };
        if let Some(i) = shown.and_then(|s| ids.iter().position(|g| *g == s)) {
            ids.swap(0, i);
        }
        let text = shown.is_some_and(|s| self.project.items.iter().any(|t| t.id == s && matches!(t.body, Body::Text { .. })));
        self.select_these(ids);
        if text {
            self.edit.rail = super::edit::Rail::Text;
            self.edit.captions.tab = 0;
        }
        let n = self.selection().len();
        self.say(format!("Selected {n} items"));
    }

    /// The picked item's body before a panel draws, to compare with afterwards.
    pub(super) fn picked_body(&self) -> Option<(u32, Body)> {
        let Some(Picked::Item(id)) = self.picked else { return None };
        if self.edit.group.len() < 2 {
            return None;
        }
        self.project.items.iter().find(|t| t.id == id).map(|t| (id, t.body.clone()))
    }

    /// Copies what the panel changed on the picked item to the rest of the selection.
    pub(super) fn spread_picked(&mut self, before: Option<(u32, Body)>) {
        let Some((id, before)) = before else { return };
        // A panel that picked something else meant that, not an edit.
        if self.picked != Some(Picked::Item(id)) {
            return;
        }
        let Some(after) = self.project.items.iter().find(|t| t.id == id).map(|t| t.body.clone()) else { return };
        if after == before {
            return;
        }
        let others: Vec<u32> = self.selection().into_iter().filter(|g| *g != id).collect();
        for t in self.project.items.iter_mut().filter(|t| others.contains(&t.id)) {
            spread(&before, &after, &mut t.body);
        }
    }

    /// Removes every selected item at once.
    pub(super) fn delete_selection(&mut self) -> bool {
        let all = self.selection();
        if all.len() < 2 {
            return false;
        }
        self.project.items.retain(|t| !all.contains(&t.id));
        self.edit.group.clear();
        self.picked = None;
        true
    }

    /// Moves the rest of the selection by as many frames as the dragged item moves, and
    /// says how far that is: the group moves as one, so none goes before the start.
    pub(super) fn carry_selection(&mut self, dragged: u32, moved: i64) -> i64 {
        let others: Vec<u32> = self.selection().into_iter().filter(|g| *g != dragged).collect();
        if moved == 0 || others.is_empty() {
            return moved;
        }
        let earliest = self.project.items.iter().filter(|t| others.contains(&t.id)).map(|t| t.at as i64).min().unwrap_or(0);
        let moved = moved.max(-earliest);
        for t in self.project.items.iter_mut().filter(|t| others.contains(&t.id)) {
            t.at = (t.at as i64 + moved) as u32;
        }
        moved
    }

    /// Above a panel while several are selected: how many, what's shared, and a way back
    /// to one.
    pub(super) fn group_banner(&mut self, ui: &mut Ui) {
        let n = self.selection().len();
        if n < 2 {
            return;
        }
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 58.0), Sense::hover());
        chosen_row(ui.painter(), r, 10);
        ui.painter().text(pos2(r.left() + 12.0, r.top() + 17.0), egui::Align2::LEFT_CENTER, format!("Editing {n} together"), semi(13.0), Color32::WHITE);
        ui.painter().text(pos2(r.left() + 12.0, r.top() + 39.0), egui::Align2::LEFT_CENTER, "Font, size, colour, place and animation", regular(11.5), TEXT_2);
        let one = Rect::from_min_size(pos2(r.right() - 84.0, r.center().y - 14.0), vec2(74.0, 28.0));
        if pill(ui, one, "Just This", false, 12.0).on_hover_text("Keep only the item shown below selected").clicked()
            && let Some(Picked::Item(id)) = self.picked
        {
            self.select_only(id);
        }
        ui.add_space(4.0);
    }
}
