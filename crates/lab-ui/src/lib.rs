//! The framework every demo is written against.
//!
//! A demo is a struct that implements [`Demo`] plus a [`DemoEntry`] that describes it
//! (where it sits in the library tree, its explanation text, how to create it).
//! The app shell owns the window, the library tree and the explanation panel; a demo
//! only draws its own area.

pub mod bits;
pub mod explain;
pub mod glossary;
pub mod heat;
pub mod plots;
pub mod scene;
pub mod theme;
pub mod widgets;

pub use egui;
pub use egui_plot;

/// Per-frame information handed to a demo.
pub struct DemoCx {
    /// Seconds since the app started.
    pub time: f64,
    /// Seconds since the previous frame (capped, so a stall does not cause a jump).
    pub dt: f64,
    /// Set by a demo to ask the shell to open another demo, optionally handing it data
    /// (for example "inspect this packet").
    pub open: Option<(&'static str, Vec<u8>)>,
}

impl DemoCx {
    pub fn new(time: f64, dt: f64) -> Self {
        Self { time, dt, open: None }
    }
}

pub trait Demo {
    /// Draw the demo and react to input. Called every frame.
    fn ui(&mut self, ui: &mut egui::Ui, cx: &mut DemoCx);

    /// Put the demo into a named state. Used by the "Show me" buttons of experiments.
    fn apply_preset(&mut self, _preset: &str) {}

    /// Accept data handed over from another demo.
    fn receive(&mut self, _data: &[u8]) {}
}

/// One "try this" experiment in the explanation panel.
pub struct Experiment {
    /// What to do.
    pub action: &'static str,
    /// What to look for.
    pub watch: &'static str,
    /// If set, a "Show me" button applies this preset.
    pub preset: Option<&'static str>,
}

/// The explanation shown next to a demo. Text may mark glossary terms as `[[term]]`
/// and separate paragraphs with a blank line.
pub struct Explanation {
    pub what: &'static str,
    pub why: &'static str,
    /// `(formula, the same thing in words)`.
    pub formulas: &'static [(&'static str, &'static str)],
    pub try_this: &'static [Experiment],
    /// What the simulation simplifies or ignores.
    pub leaves_out: &'static str,
    /// Where the facts come from (specification sections and so on).
    pub refs: &'static [&'static str],
}

pub struct DemoEntry {
    /// Stable identifier, e.g. `"ble-adv-packet"`.
    pub id: &'static str,
    pub title: &'static str,
    /// One sentence for the library list.
    pub blurb: &'static str,
    /// Position in the library tree, e.g. `["Wireless", "Bluetooth LE"]`.
    pub path: &'static [&'static str],
    pub explain: Explanation,
    pub make: fn() -> Box<dyn Demo>,
}

/// An ordered walk through demos.
pub struct LearningPath {
    pub id: &'static str,
    pub title: &'static str,
    pub blurb: &'static str,
    pub steps: &'static [&'static str],
}

/// A demo that is planned but not built yet; shown greyed out in the library tree.
pub struct Planned {
    pub title: &'static str,
    pub path: &'static [&'static str],
}

#[derive(Default)]
pub struct Registry {
    pub demos: Vec<DemoEntry>,
    pub paths: Vec<LearningPath>,
    pub planned: Vec<Planned>,
}

impl Registry {
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.demos.iter().position(|d| d.id == id)
    }

    /// Check the library for mistakes. Returns one message per problem.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (i, d) in self.demos.iter().enumerate() {
            if self.demos[..i].iter().any(|o| o.id == d.id) {
                out.push(format!("duplicate demo id '{}'", d.id));
            }
            if d.path.is_empty() {
                out.push(format!("{}: empty tree path", d.id));
            }
            let e = &d.explain;
            for (name, text) in [("what", e.what), ("why", e.why), ("leaves_out", e.leaves_out), ("blurb", d.blurb)] {
                if text.trim().len() < 20 {
                    out.push(format!("{}: section '{name}' is missing or too short", d.id));
                }
            }
            if e.try_this.len() < 2 {
                out.push(format!("{}: needs at least two experiments", d.id));
            }
            for text in [e.what, e.why, e.leaves_out].into_iter().chain(e.try_this.iter().flat_map(|x| [x.action, x.watch])) {
                for term in explain::terms_in(text) {
                    if glossary::lookup(term).is_none() {
                        out.push(format!("{}: glossary has no entry for [[{term}]]", d.id));
                    }
                }
            }
        }
        for p in &self.paths {
            for step in p.steps {
                if self.index_of(step).is_none() {
                    out.push(format!("path '{}': unknown demo '{step}'", p.id));
                }
            }
        }
        out
    }
}
