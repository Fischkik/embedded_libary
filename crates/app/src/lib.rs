//! The app shell: library on the left, the demo in the middle, its explanation on the
//! right. Independent of the windowing code in `main.rs`, so tests can drive it headless.

use lab_ui::{Demo, DemoCx, Registry, explain, theme};
use serde::{Deserialize, Serialize};

/// What is remembered between runs.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Saved {
    pub current: String,
    pub visited: Vec<String>,
    pub library_open: bool,
    pub explain_open: bool,
    pub show_paths: bool,
    pub active_path: String,
}

impl Default for Saved {
    fn default() -> Self {
        Self { current: String::new(), visited: Vec::new(), library_open: true, explain_open: true, show_paths: true, active_path: String::new() }
    }
}

pub struct Shell {
    pub reg: Registry,
    pub saved: Saved,
    current: usize,
    demo: Box<dyn Demo>,
    search: String,
    styled: bool,
}

impl Shell {
    pub fn new(reg: Registry, saved: Saved) -> Self {
        assert!(!reg.demos.is_empty(), "the registry has no demos");
        let current = reg.index_of(&saved.current).unwrap_or(0);
        let demo = (reg.demos[current].make)();
        let mut shell = Self { reg, saved, current, demo, search: String::new(), styled: false };
        shell.mark_visited();
        shell
    }

    pub fn current_id(&self) -> &'static str {
        self.reg.demos[self.current].id
    }

    fn mark_visited(&mut self) {
        let id = self.current_id();
        self.saved.current = id.to_owned();
        if !self.saved.visited.iter().any(|v| v == id) {
            self.saved.visited.push(id.to_owned());
        }
    }

    /// Open a demo in its starting state, optionally handing it data.
    pub fn select(&mut self, index: usize, data: Option<&[u8]>) {
        self.current = index.min(self.reg.demos.len() - 1);
        self.demo = (self.reg.demos[self.current].make)();
        if let Some(d) = data {
            self.demo.receive(d);
        }
        self.mark_visited();
    }

    pub fn apply_preset(&mut self, preset: &str) {
        self.demo = (self.reg.demos[self.current].make)();
        self.demo.apply_preset(preset);
    }

    /// The learning path used for previous/next: the one last clicked in if it contains
    /// the current demo, otherwise the first path that does.
    fn path_position(&self) -> Option<(usize, usize)> {
        let id = self.current_id();
        let find = |p: usize| self.reg.paths[p].steps.iter().position(|s| *s == id).map(|i| (p, i));
        self.reg
            .paths
            .iter()
            .position(|p| p.id == self.saved.active_path)
            .and_then(find)
            .or_else(|| (0..self.reg.paths.len()).find_map(find))
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if !self.styled {
            theme::install(ui.ctx());
            self.styled = true;
        }
        let (time, dt) = ui.input(|i| (i.time, i.stable_dt.min(0.1) as f64));
        let mut go: Option<(usize, Option<Vec<u8>>)> = None;
        let mut preset: Option<&'static str> = None;

        egui::Panel::top("top_bar").show(ui, |ui| {
            ui.add_space(3.0);
            self.top_bar(ui, &mut go);
            ui.add_space(3.0);
        });

        let mut library_open = self.saved.library_open;
        egui::Panel::left("library").resizable(true).default_size(285.0).min_size(200.0).show_collapsible(ui, &mut library_open, |ui| {
            self.library(ui, &mut go);
        });
        self.saved.library_open = library_open;

        let mut explain_open = self.saved.explain_open;
        egui::Panel::right("explain").resizable(true).default_size(400.0).min_size(260.0).show_collapsible(ui, &mut explain_open, |ui| {
            egui::ScrollArea::vertical().id_salt("explain_scroll").auto_shrink(false).show(ui, |ui| {
                preset = explain::show(ui, &self.reg.demos[self.current]);
            });
        });
        self.saved.explain_open = explain_open;

        egui::CentralPanel::default().show(ui, |ui| {
            let mut cx = DemoCx::new(time, dt);
            egui::ScrollArea::vertical().id_salt("demo_scroll").auto_shrink(false).show(ui, |ui| {
                ui.push_id(self.reg.demos[self.current].id, |ui| self.demo.ui(ui, &mut cx));
            });
            if let Some((id, data)) = cx.open {
                if let Some(i) = self.reg.index_of(id) {
                    go = Some((i, Some(data)));
                }
            }
        });

        if let Some(p) = preset {
            self.apply_preset(p);
        }
        if let Some((index, data)) = go {
            self.select(index, data.as_deref());
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, go: &mut Option<(usize, Option<Vec<u8>>)>) {
        let entry = &self.reg.demos[self.current];
        let breadcrumb = format!("{} > {}", entry.path.join(" > "), entry.title);
        ui.horizontal(|ui| {
            ui.toggle_value(&mut self.saved.library_open, "Library").on_hover_text("Show or hide the list of demos");
            ui.label(egui::RichText::new("Embedded Lab").strong());
            ui.separator();
            ui.label(egui::RichText::new(breadcrumb).weak());

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let dark = ui.visuals().dark_mode;
                if ui.selectable_label(dark, "Dark").on_hover_text("Dark colours").clicked() {
                    ui.ctx().set_theme(egui::Theme::Dark);
                }
                if ui.selectable_label(!dark, "Light").on_hover_text("Light colours").clicked() {
                    ui.ctx().set_theme(egui::Theme::Light);
                }
                ui.separator();
                ui.toggle_value(&mut self.saved.explain_open, "Explanation").on_hover_text("Show or hide the explanation panel");
                if ui.button("Reset demo").on_hover_text("Put this demo back to how it started").clicked() {
                    *go = Some((self.current, None));
                }
                if let Some((p, i)) = self.path_position() {
                    let steps = self.reg.paths[p].steps;
                    ui.separator();
                    let next = steps.get(i + 1).and_then(|id| self.reg.index_of(id));
                    if ui.add_enabled(next.is_some(), egui::Button::new("Next >")).on_hover_text("Next step of this learning path").clicked() {
                        *go = next.map(|n| (n, None));
                    }
                    ui.label(format!("step {} of {}", i + 1, steps.len()));
                    let prev = i.checked_sub(1).and_then(|j| self.reg.index_of(steps[j]));
                    if ui.add_enabled(prev.is_some(), egui::Button::new("< Back")).on_hover_text("Previous step of this learning path").clicked() {
                        *go = prev.map(|n| (n, None));
                    }
                }
            });
        });
    }

    fn library(&mut self, ui: &mut egui::Ui, go: &mut Option<(usize, Option<Vec<u8>>)>) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.saved.show_paths, true, "Learning paths").on_hover_text("Demos in a sensible order, starting from the basics");
            ui.selectable_value(&mut self.saved.show_paths, false, "All topics").on_hover_text("Everything, sorted by field");
        });
        ui.separator();
        egui::ScrollArea::vertical().id_salt("library_scroll").auto_shrink(false).show(ui, |ui| {
            if self.saved.show_paths {
                self.paths(ui, go);
            } else {
                ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search demos"));
                ui.add_space(4.0);
                self.tree(ui, &[], go);
            }
        });
    }

    fn paths(&mut self, ui: &mut egui::Ui, go: &mut Option<(usize, Option<Vec<u8>>)>) {
        let pal = theme::palette(ui);
        for p in &self.reg.paths {
            let done = p.steps.iter().filter(|s| self.saved.visited.iter().any(|v| v == *s)).count();
            ui.label(egui::RichText::new(p.title).strong().size(16.0));
            ui.label(egui::RichText::new(p.blurb).small().color(pal.dim));
            ui.label(egui::RichText::new(format!("{done} of {} opened", p.steps.len())).small().color(pal.dim));
            for (n, step) in p.steps.iter().enumerate() {
                let Some(i) = self.reg.index_of(step) else { continue };
                let visited = self.saved.visited.iter().any(|v| v == step);
                let mark = if visited { "x" } else { " " };
                let text = format!("[{mark}] {}. {}", n + 1, self.reg.demos[i].title);
                let selected = i == self.current && self.path_position().is_some_and(|(pp, _)| self.reg.paths[pp].id == p.id);
                if ui.selectable_label(selected, text).on_hover_text(self.reg.demos[i].blurb).clicked() {
                    self.saved.active_path = p.id.to_owned();
                    *go = Some((i, None));
                }
            }
            ui.add_space(12.0);
        }
    }

    /// The library as a tree, built from each demo's `path`. `prefix` is the branch being drawn.
    fn tree(&self, ui: &mut egui::Ui, prefix: &[&'static str], go: &mut Option<(usize, Option<Vec<u8>>)>) {
        let pal = theme::palette(ui);
        let query = self.search.trim().to_lowercase();
        let matches = |title: &str, blurb: &str| query.is_empty() || title.to_lowercase().contains(&query) || blurb.to_lowercase().contains(&query);

        for (i, d) in self.reg.demos.iter().enumerate() {
            if d.path == prefix && matches(d.title, d.blurb) && ui.selectable_label(i == self.current, d.title).on_hover_text(d.blurb).clicked() {
                *go = Some((i, None));
            }
        }
        for p in &self.reg.planned {
            if p.path == prefix && matches(p.title, "") {
                ui.label(egui::RichText::new(format!("{}  (planned)", p.title)).color(pal.dim)).on_hover_text("Not built yet - part of the roadmap.");
            }
        }

        let mut children: Vec<&'static str> = Vec::new();
        let demo_paths = self.reg.demos.iter().filter(|d| matches(d.title, d.blurb)).map(|d| d.path);
        let planned_paths = self.reg.planned.iter().filter(|p| matches(p.title, "")).map(|p| p.path);
        for path in demo_paths.chain(planned_paths) {
            if path.len() > prefix.len() && path.starts_with(prefix) && !children.contains(&path[prefix.len()]) {
                children.push(path[prefix.len()]);
            }
        }
        for child in children {
            let mut branch = prefix.to_vec();
            branch.push(child);
            let has_built = self.reg.demos.iter().any(|d| d.path.starts_with(&branch));
            egui::CollapsingHeader::new(child).id_salt(branch.join("/")).default_open(has_built || !query.is_empty()).show(ui, |ui| self.tree(ui, &branch, go));
        }
    }
}
