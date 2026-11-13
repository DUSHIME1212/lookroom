use super::*;

impl App {
    pub(super) fn remember_project(&mut self, path: &Path) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.settings.recent_projects.retain(|p| p != &path);
        self.settings.recent_projects.insert(0, path);
        self.settings.recent_projects.truncate(12);
        store::save_settings(&self.settings);
    }

    pub(super) fn home_page(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some((message, _)) = &self.toast { ui.label(message); }
            ui.add_space(24.0);
            ui.heading(egui::RichText::new("LOOKROOM").size(18.0).color(super::kit::FOCUS));
            ui.add_space(16.0);
            ui.heading(egui::RichText::new("Your next story starts here.").size(36.0));
            ui.label("Create a video, start with a template, or pick up where you left off.");
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                if ui.button("+ New project").clicked() { self.start_home_project("Untitled", (1920, 1080)); }
                if ui.button("Open project…").clicked() { self.open_project(); }
                if ui.button("Continue editing").clicked() { self.home = false; }
            });
            ui.add_space(28.0);
            ui.heading("Video templates");
            ui.label("Ready-to-edit formats with animated titles. Add your own footage in the editor.");
            let choices = [
                ("Daily vlog", "A day worth remembering", (1920,1080)),
                ("Travel diary", "Places. People. Moments.", (1920,1080)),
                ("Product launch", "Meet your new favourite", (1080,1920)),
                ("Podcast", "Conversations that matter", (1920,1080)),
                ("Tutorial", "Learn something new", (1920,1080)),
                ("Food story", "Made with love", (1080,1920)),
                ("Fitness", "One step stronger", (1080,1920)),
                ("Event recap", "What a night", (1920,1080)),
                ("Real estate", "Welcome home", (1920,1080)),
                ("Social reel", "Make every moment count", (1080,1920)),
            ];
            let mut selected = None;
            ui.horizontal_wrapped(|ui| {
                for (i, (name, subtitle, size)) in choices.iter().enumerate() {
                    egui::Frame::new().fill(Color32::from_rgb(30, 34, 38)).inner_margin(18.0).corner_radius(4).show(ui, |ui| {
                        ui.set_min_size(vec2(190.0, 115.0));
                        ui.label(egui::RichText::new(format!("{:02}", i + 1)).size(26.0).color(super::kit::FOCUS));
                        ui.strong(*name);
                        ui.label(if size.0 < size.1 { "9:16 · Portrait" } else { "16:9 · Landscape" });
                        if ui.button("Use template →").clicked() { selected = Some((i, *name, *subtitle, *size)); }
                    });
                }
            });
            if let Some((i, name, subtitle, size)) = selected {
                self.start_home_project(name, size);
                let list = templates::templates();
                if let Some(t) = list.get(i % list.len().max(1)) {
                    self.place_template(t, Some(&[name.to_string(), subtitle.to_string()]), 0, (0.5,0.5));
                }
            }
            ui.add_space(28.0);
            ui.heading("Recent projects");
            if self.settings.recent_projects.is_empty() { ui.label("Your saved projects will appear here."); }
            for path in self.settings.recent_projects.clone() {
                ui.horizontal(|ui| {
                    let exists = path.is_file();
                    let name = path.file_stem().unwrap_or_default().to_string_lossy();
                    if ui.add_enabled(exists, egui::Button::new(name.as_ref())).clicked() { self.load_project(&path); }
                    ui.weak(path.display().to_string());
                    if !exists { ui.label("File unavailable"); }
                });
            }
        });
    }

    fn start_home_project(&mut self, name: &str, size: (usize, usize)) {
        self.stop_play();
        self.spare_feeds.clear();
        self.stills.clear();
        self.exifs.clear();
        self.edit = edit::EditState::default();
        self.picked = None;
        self.drawn_project = None;
        self.active_layer = None;
        self.active_mask = 0;
        self.selected = None;
        self.drawn = None;
        self.index_cache = None;
        self.project = Project::default();
        self.project.name = name.into();
        self.project.size = size;
        self.grade = Grade::default();
        let (w, h) = player::fit_size(size, 720);
        self.preview = Arc::new(Picture { w, h, px: vec![[0.025, 0.03, 0.045]; w * h] });
        self.picture_id = self.picture_id.wrapping_add(1);
        self.photo_path = None;
        self.photo_name = name.into();
        self.active = None;
        self.clip = None;
        self.shown_media = None;
        self.head = 0;
        self.history = vec![("New project".into(), self.project.clone())];
        self.history_at = 0;
        self.needs_activate = true;
        self.home = false;
    }
}
