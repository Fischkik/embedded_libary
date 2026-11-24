use crate::prelude::*;
use egui_plot::{HLine, MarkerShape, Points, Polygon};
use sim_core::radar::{CfarScene, ca_cfar, cfar_alpha};

const CELLS: usize = 128;
const CLUTTER: (usize, usize) = (44, 84);
/// (range cell, signal-to-noise ratio in dB): a weak target, one inside the clutter area,
/// and a strong target with a weaker neighbour four cells away.
const TARGETS: [(usize, f64); 4] = [(20, 8.0), (64, 24.0), (100, 20.0), (104, 14.0)];

pub struct RadarCfar {
    pfa_exp: f64,
    train: usize,
    guard: usize,
    clutter_db: f64,
    fixed_db: f64,
    seed: u64,
}

impl Default for RadarCfar {
    fn default() -> Self {
        Self { pfa_exp: -3.0, train: 8, guard: 2, clutter_db: 10.0, fixed_db: 12.0, seed: 4 }
    }
}

/// What a detector found: (targets found, false alarms, detected cells).
pub(crate) struct Outcome {
    found: Vec<usize>,
    false_alarms: usize,
    cells: Vec<usize>,
}

fn judge(power: &[f64], threshold: &[f64]) -> Outcome {
    let cells: Vec<usize> = (0..power.len()).filter(|&i| power[i] > threshold[i]).collect();
    let near = |c: usize, t: usize| c.abs_diff(t) <= 1;
    let found = TARGETS.iter().map(|t| t.0).filter(|&t| cells.iter().any(|&c| near(c, t))).collect();
    let false_alarms = cells.iter().filter(|&&c| !TARGETS.iter().any(|t| near(c, t.0))).count();
    Outcome { found, false_alarms, cells }
}

impl RadarCfar {
    fn run(&self) -> (Vec<f64>, Vec<f64>, Outcome, Outcome) {
        let scene = CfarScene { cells: CELLS, clutter_db: self.clutter_db, clutter_cells: CLUTTER, targets: TARGETS.to_vec() };
        let power = scene.profile(self.seed);
        let adaptive = ca_cfar(&power, self.train, self.guard, 10f64.powf(self.pfa_exp));
        let fixed = vec![10f64.powf(self.fixed_db / 10.0); CELLS];
        let (a, f) = (judge(&power, &adaptive), judge(&power, &fixed));
        (power, adaptive, a, f)
    }
}

fn db(x: f64) -> f64 {
    10.0 * x.max(1e-12).log10()
}

impl Demo for RadarCfar {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "Detector settings", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "pfa", |ui| {
                    ui.add(egui::Slider::new(&mut self.pfa_exp, -6.0..=-1.0).text("false alarm rate 10^").fixed_decimals(1))
                        .on_hover_text("The share of empty cells that may be reported as a target by mistake. -3 means one cell in a thousand.")
                });
                widgets::wrap(ui, "train", |ui| ui.add(egui::Slider::new(&mut self.train, 2..=24).text("training cells per side")).on_hover_text("How many neighbouring cells on each side are averaged to estimate the noise around the cell under test."));
                widgets::wrap(ui, "guard", |ui| ui.add(egui::Slider::new(&mut self.guard, 0..=6).text("guard cells per side")).on_hover_text("Cells right next to the cell under test that are left out of the average, so that a target's own spread does not count as noise."));
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "clutter", |ui| ui.add(egui::Slider::new(&mut self.clutter_db, 0.0..=25.0).text("clutter above the noise").suffix(" dB").fixed_decimals(0)).on_hover_text("Echoes of rain, ground or a wall area raise the background in the middle of the range."));
                widgets::wrap(ui, "fixed", |ui| ui.add(egui::Slider::new(&mut self.fixed_db, 0.0..=30.0).text("fixed threshold").suffix(" dB").fixed_decimals(0)).on_hover_text("For comparison: one threshold for every cell, set by hand above the noise."));
                if ui.button("new noise").on_hover_text("Draw a new random noise pattern; the targets stay.").clicked() {
                    self.seed += 1;
                }
            });
        });
        let (power, adaptive, a, f) = self.run();

        ui.label(RichText::new("Received power along the range, and two ways to decide what is a target").strong());
        plots::plot_bare("cfar", 260.0).x_axis_label("range cell").y_axis_label("power [dB above noise]").include_x(0.0).include_x(CELLS as f64).include_y(-15.0).include_y(32.0).show(ui, |p| {
            let fill = pal.dim.gamma_multiply(0.15);
            p.polygon(Polygon::new("clutter", plots::bar(CLUTTER.0 as f64, CLUTTER.1 as f64, -15.0, 32.0)).fill_color(fill).stroke((0.0, fill)));
            p.line(plots::line("power", power.iter().enumerate().map(|(i, v)| [i as f64, db(*v)]).collect(), pal.trace[4]).width(1.5));
            p.hline(HLine::new("fixed", self.fixed_db).color(pal.trace[1]).style(egui_plot::LineStyle::dashed_dense()));
            p.line(plots::line("CFAR", adaptive.iter().enumerate().filter(|(_, t)| t.is_finite()).map(|(i, t)| [i as f64, db(*t)]).collect(), pal.trace[2]).width(2.0));
            let mark = |o: &Outcome, y: f64| o.cells.iter().map(|&c| [c as f64, y]).collect::<Vec<_>>();
            p.points(Points::new("CFAR hits", mark(&a, 29.0)).shape(MarkerShape::Down).radius(5.0).color(pal.trace[2]));
            p.points(Points::new("fixed hits", mark(&f, 26.0)).shape(MarkerShape::Down).radius(5.0).color(pal.trace[1]));
            p.points(Points::new("targets", TARGETS.iter().map(|t| [t.0 as f64, -12.0]).collect::<Vec<_>>()).shape(MarkerShape::Up).radius(6.0).color(pal.trace[3]));
        });
        widgets::key(
            ui,
            &[(pal.trace[4], "received power"), (pal.trace[2], "CFAR threshold and its detections"), (pal.trace[1], "fixed threshold and its detections"), (pal.trace[3], "where the real targets are"), (pal.dim, "shaded: clutter area")],
        );

        let n = 2 * self.train;
        let pfa = 10f64.powf(self.pfa_exp);
        widgets::formula(
            ui,
            format!(
                "threshold = alpha x average of the {n} training cells\n\
                 alpha = N x (Pfa^(-1/N) - 1) = {n} x ({pfa:.0e}^(-1/{n}) - 1) = {:.2} = {:.1} dB\n\
                 CFAR:  {} of 4 targets found, {} false alarm(s)\n\
                 fixed: {} of 4 targets found, {} false alarm(s)",
                cfar_alpha(n, pfa),
                db(cfar_alpha(n, pfa)),
                a.found.len(),
                a.false_alarms,
                f.found.len(),
                f.false_alarms,
            ),
        );
        widgets::status(ui, a.false_alarms <= f.false_alarms, format!("CFAR: {} false alarm(s), fixed threshold: {}", a.false_alarms, f.false_alarms));
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "clutter" => self.clutter_db = 20.0,
            "strict" => self.pfa_exp = -6.0,
            "guard" => self.guard = 4,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "radar-cfar",
        title: "Detection thresholds (CFAR)",
        blurb: "Where does noise end and a target begin? A threshold that measures the noise around each cell keeps false alarms rare, whatever the background.",
        path: &["Radar"],
        make: || Box::new(RadarCfar::default()),
        explain: Explanation {
            what: "The received power of a radar along its range: one value per range cell, as an FMCW radar delivers it (see the \
                FMCW demo). There is noise everywhere, a shaded area with extra [[clutter]], and four real targets, marked \
                with triangles at the bottom.\n\n\
                Two detectors decide which cells hold a target: a fixed threshold (orange dashed line) and a [[CFAR]] \
                threshold (green), which follows the background. The markers at the top show what each one reports.",
            why: "Noise never stops: every cell of a range profile holds some random power, and now and then a cell of pure \
                noise is large. Any threshold therefore produces [[false alarm]]s; the question is only how often. A \
                threshold set high enough to keep false alarms rare in quiet surroundings is useless where the background \
                is higher - rain, the ground, a wall - because there the noise crosses it all the time.\n\n\
                Constant false alarm rate (CFAR) detection measures the background separately for each cell. For the cell \
                under test it averages the power of a few training cells on both sides, skipping a few guard cells right \
                next to it so that the target's own spread does not count as background. The threshold is that average \
                times a factor alpha.\n\n\
                Alpha follows from the false alarm rate you can accept. For noise as it comes out of a square-law detector \
                (exponentially distributed power), the chance that a noise cell crosses alpha x the average of N cells is \
                (1 + alpha / N)^(-N), whatever the noise level. Solving for alpha gives the formula below. The rate stays \
                constant when the background rises - hence the name.\n\n\
                The average of a few cells is itself noisy, so alpha must be larger than it would be with a perfectly known \
                noise level: CFAR needs a somewhat stronger target than an ideal detector. More training cells reduce \
                that loss, but stretch the average over a wider area, where the background may change and where other \
                targets may sit. A strong target among the training cells raises the threshold for its neighbour: masking.",
            formulas: &[
                ("threshold = alpha x average of N training cells,  alpha = N x (Pfa^(-1/N) - 1)", "N = 16 cells, Pfa = 10^-4: alpha = 16 x (10^0.25 - 1) = 12.45, which is 11 dB above the average."),
                ("chance that pure noise crosses it = (1 + alpha / N)^(-N) = Pfa", "With Pfa = 10^-3, one cell in a thousand is reported by mistake - in quiet areas and in clutter alike."),
            ],
            try_this: &[
                Experiment {
                    action: "Raise the clutter to 20 dB.",
                    watch: "the fixed threshold reporting most of the clutter cells, while CFAR lifts its threshold with the background. The target inside the clutter is now only 4 dB above it, and CFAR rightly misses it.",
                    preset: Some("clutter"),
                },
                Experiment {
                    action: "Demand a false alarm rate of one in a million.",
                    watch: "the CFAR threshold rising 4 dB, and the weak target at cell 20 being lost. Fewer false alarms cost sensitivity.",
                    preset: Some("strict"),
                },
                Experiment {
                    action: "Widen the guard to 4 cells per side.",
                    watch: "the weaker of the two close targets (cell 104) being found. With 2 guard cells, its strong neighbour at cell 100 sat among its training cells and pushed the threshold above it.",
                    preset: Some("guard"),
                },
            ],
            leaves_out: "Each target occupies one cell here; real echoes spread into neighbouring cells. The noise is ideal \
                (independent, exponentially distributed power). Variants that cope better with clutter edges and close \
                targets exist: greatest-of and smallest-of CFAR use one side of the window only, ordered-statistic CFAR \
                takes a ranked value instead of the average. Real radars apply CFAR in two dimensions, across range and \
                speed (see the range-Doppler demo). The cells near the ends use only the training cells that exist.",
            refs: &[
                "Cell-averaging CFAR: threshold = alpha x mean of the training cells (MathWorks, \"Constant False Alarm Rate (CFAR) Detection\"); for exponential noise Pfa = (1 + alpha/N)^(-N), checked here by a Monte Carlo test over 200 000 cells.",
                "M. A. Richards, Fundamentals of Radar Signal Processing, chapter 6 (threshold detection, CFAR).",
            ],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The experiments promise particular outcomes; check them on the exact noise shown.
    #[test]
    fn experiments_show_what_they_promise() {
        let run = |preset: &str| {
            let mut d = RadarCfar::default();
            d.apply_preset(preset);
            d.run()
        };
        let (_, _, a, f) = run("");
        assert!(f.false_alarms > a.false_alarms, "default: fixed {} vs cfar {}", f.false_alarms, a.false_alarms);
        assert!(!a.found.contains(&104), "default: 104 is masked {:?}", a.found);
        let (_, _, a, f) = run("clutter");
        assert!(f.false_alarms >= 20 && a.false_alarms <= 2, "clutter: fixed {} cfar {}", f.false_alarms, a.false_alarms);
        assert!(!a.found.contains(&64), "clutter: 64 should be lost {:?}", a.found);
        let (_, _, a, _) = run("strict");
        assert!(!a.found.contains(&20) && a.found.contains(&100), "strict {:?}", a.found);
        let (_, _, a, _) = run("guard");
        assert!(a.found.contains(&104) && a.found.contains(&100), "guard {:?}", a.found);
        // "4 dB": 10^-3 -> 10^-6 with N = 16.
        assert!((db(cfar_alpha(16, 1e-6)) - db(cfar_alpha(16, 1e-3)) - 4.0).abs() < 0.1);
    }
}
