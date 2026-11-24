use crate::prelude::*;
use egui_plot::{MarkerShape, Points};
use sim_core::radar::{aoa_angle, aoa_estimates, aoa_phase, aoa_unambiguous_rad, wrap_pi};
use sim_core::rng::Rng;

const F0: f64 = 24.125e9;
/// The radar sits at the bottom centre of a 8 m x 4 m room (world y grows downwards).
const RADAR: Pos2 = Pos2 { x: 0.0, y: 4.0 };

pub struct RadarAngle {
    target: Pos2,
    /// Antenna spacing in wavelengths.
    spacing_wl: f64,
    noise_deg: f64,
}

impl Default for RadarAngle {
    fn default() -> Self {
        Self { target: at_angle(30.0), spacing_wl: 0.5, noise_deg: 3.0 }
    }
}

/// A target 3 m from the radar at `deg` from straight ahead.
fn at_angle(deg: f64) -> Pos2 {
    let a = deg.to_radians();
    pos2((3.0 * a.sin()) as f32, (4.0 - 3.0 * a.cos()) as f32)
}

impl RadarAngle {
    fn theta(&self) -> f64 {
        let d = self.target - RADAR;
        (d.x as f64).atan2(-(d.y as f64).min(-1e-3))
    }
}

impl Demo for RadarAngle {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "Two receive antennas", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "spacing", |ui| {
                    ui.add(egui::Slider::new(&mut self.spacing_wl, 0.25..=2.0).text("antenna spacing").suffix(" wavelengths").fixed_decimals(2))
                        .on_hover_text("Distance between the two receive antennas, in wavelengths. At 24 GHz one wavelength is 12.4 mm.")
                });
                widgets::wrap(ui, "noise", |ui| {
                    ui.add(egui::Slider::new(&mut self.noise_deg, 0.0..=30.0).text("phase noise").suffix(" °").fixed_decimals(0))
                        .on_hover_text("How much each phase measurement is disturbed by receiver noise (root mean square). A weak echo gives more noise.")
                });
            });
        });
        let lambda = C / F0;
        let d = self.spacing_wl * lambda;
        let theta = self.theta();
        let true_phase = aoa_phase(d, theta, lambda);
        let measured = wrap_pi(true_phase);
        let estimate = aoa_angle(measured, d, lambda);
        let fov = aoa_unambiguous_rad(d, lambda);
        let estimates = aoa_estimates(theta, d, lambda, self.noise_deg.to_radians(), 40, &mut Rng::new(1));

        ui.label(RichText::new("Seen from above: drag the target").strong());
        let world = egui::Rect::from_min_max(pos2(-4.0, 0.0), pos2(4.0, 4.35));
        scene::scene(ui, "aoa", world, 260.0, |s| {
            for side in [-1.0, 1.0] {
                let a = side * fov;
                let end = RADAR + vec2((5.0 * a.sin()) as f32, -(5.0 * a.cos()) as f32);
                s.painter.line_segment([s.pos(RADAR), s.pos(end)], (1.0, pal.dim));
            }
            for e in estimates.iter().flatten() {
                let end = RADAR + vec2((4.5 * e.sin()) as f32, -(4.5 * e.cos()) as f32);
                s.painter.line_segment([s.pos(RADAR), s.pos(end)], (1.0, pal.trace[1].gamma_multiply(0.35)));
            }
            if let Some(e) = estimate
                && (e - theta).abs() > 1f64.to_radians() {
                    let end = RADAR + vec2((5.0 * e.sin()) as f32, -(5.0 * e.cos()) as f32);
                    s.painter.line_segment([s.pos(RADAR), s.pos(end)], (2.0, pal.bad));
                    let ghost = RADAR + (end - RADAR).normalized() * (self.target - RADAR).length();
                    s.painter.circle_stroke(s.pos(ghost), 8.0, (2.0, pal.bad));
                }
            s.painter.line_segment([s.pos(RADAR), s.pos(self.target)], (1.5, pal.trace[2]));
            let half = (0.08 * self.spacing_wl) as f32;
            for x in [-half, half] {
                s.painter.circle_filled(s.pos(RADAR + vec2(x, -0.05)), 4.0, pal.trace[0]);
            }
            s.painter.rect_filled(egui::Rect::from_center_size(s.pos(RADAR), vec2(30.0, 8.0)), 2.0, pal.text);
            let mut t = self.target;
            s.handle("target", &mut t, 10.0).on_hover_text("Drag the target around the room.");
            t.y = t.y.min(3.9);
            self.target = t;
            s.painter.circle_filled(s.pos(self.target), 8.0, pal.trace[2]);
        });
        widgets::key(ui, &[(pal.trace[2], "target and its true direction"), (pal.bad, "where the radar thinks it is, if different"), (pal.dim, "edges of the unambiguous field of view"), (pal.trace[1], "faint: directions measured with noise"), (pal.trace[0], "the antennas (spacing drawn much larger)")]);

        ui.columns(2, |cols| {
            cols[0].label(RichText::new("The echo at each antenna, as an arrow").strong());
            plots::plot_bare("phasors", 200.0).data_aspect(1.0).include_x(-1.3).include_x(1.3).include_y(-1.3).include_y(1.3).show_axes(false).show(&mut cols[0], |p| {
                let circle: Vec<[f64; 2]> = (0..=72).map(|k| k as f64 * std::f64::consts::TAU / 72.0).map(|a| [a.cos(), a.sin()]).collect();
                p.line(plots::line("circle", circle, pal.grid));
                p.line(plots::line("antenna 1", vec![[0.0, 0.0], [1.0, 0.0]], pal.trace[0]).width(3.0));
                p.line(plots::line("antenna 2", vec![[0.0, 0.0], [measured.cos(), measured.sin()]], pal.trace[1]).width(3.0));
            });
            widgets::key(&mut cols[0], &[(pal.trace[0], "antenna 1"), (pal.trace[1], "antenna 2")]);
            cols[1].label(RichText::new("Angle the radar computes, for every true angle").strong());
            plots::plot_bare("estimate", 200.0).x_axis_label("true angle [°]").y_axis_label("computed [°]").include_x(-90.0).include_x(90.0).include_y(-90.0).include_y(90.0).show(&mut cols[1], |p| {
                let mut seg: Vec<[f64; 2]> = Vec::new();
                for k in -900..=900 {
                    let th = (k as f64 / 10.0).to_radians();
                    let e = aoa_angle(wrap_pi(aoa_phase(d, th, lambda)), d, lambda).map(f64::to_degrees);
                    match (e, seg.last()) {
                        (Some(e), Some(last)) if (e - last[1]).abs() < 20.0 => seg.push([th.to_degrees(), e]),
                        (e, _) => {
                            if seg.len() > 1 {
                                p.line(plots::line("computed", std::mem::take(&mut seg), pal.trace[0]).width(2.0));
                            }
                            seg = e.map(|e| vec![[th.to_degrees(), e]]).unwrap_or_default();
                        }
                    }
                }
                p.line(plots::line("computed", seg, pal.trace[0]).width(2.0));
                let pts: Vec<[f64; 2]> = estimates.iter().flatten().map(|e| [theta.to_degrees(), e.to_degrees()]).collect();
                p.points(Points::new("with noise", pts).radius(2.5).color(pal.trace[1]));
                p.points(Points::new("target", vec![[theta.to_degrees(), estimate.unwrap_or(f64::NAN).to_degrees()]]).shape(MarkerShape::Diamond).radius(7.0).color(pal.trace[2]));
            });
            widgets::key(&mut cols[1], &[(pal.trace[1], "40 measurements with noise"), (pal.trace[2], "the target now")]);
        });

        let found: Vec<f64> = estimates.iter().flatten().copied().collect();
        let mean = found.iter().sum::<f64>() / found.len().max(1) as f64;
        let spread = (found.iter().map(|e| (e - mean).powi(2)).sum::<f64>() / found.len().max(1) as f64).sqrt();
        widgets::formula(
            ui,
            format!(
                "wavelength = c / f = {}, spacing d = {:.2} wavelengths = {}\n\
                 phase difference = 360° x d / wavelength x sin(angle) = 360° x {:.2} x sin({:.1}°) = {:.1}°\n\
                 a phase is only known between -180° and +180°: measured {:.1}°\n\
                 angle = asin(phase / (360° x d / wavelength)) = {}\n\
                 unambiguous while |sin(angle)| <= wavelength / 2d: up to ±{:.1}°   noise scatter: ±{:.1}°",
                si(lambda, "m"),
                self.spacing_wl,
                si(d, "m"),
                self.spacing_wl,
                theta.to_degrees(),
                true_phase.to_degrees(),
                measured.to_degrees(),
                estimate.map_or("none".to_owned(), |e| format!("{:.1}°", e.to_degrees())),
                fov.to_degrees(),
                spread.to_degrees(),
            ),
        );
        let ok = estimate.is_some_and(|e| (e - theta).abs() < 1f64.to_radians());
        widgets::status(
            ui,
            ok,
            if ok {
                format!("the radar finds the target at {:.1}°", theta.to_degrees())
            } else {
                format!("a ghost: the target is at {:.1}°, the radar reports {}", theta.to_degrees(), estimate.map_or("nothing".to_owned(), |e| format!("{:.1}°", e.to_degrees())))
            },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "wide" => {
                self.spacing_wl = 1.0;
                self.target = at_angle(40.0);
            }
            "noisy" => {
                self.noise_deg = 10.0;
                self.target = at_angle(5.0);
            }
            "far-apart" => {
                self.noise_deg = 10.0;
                self.spacing_wl = 2.0;
                self.target = at_angle(5.0);
            }
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "radar-angle",
        title: "Angle of arrival with two antennas",
        blurb: "One antenna tells you that something is there. Two antennas side by side tell you in which direction.",
        path: &["Radar"],
        make: || Box::new(RadarAngle::default()),
        explain: Explanation {
            what: "A radar with two receive antennas side by side, seen from above, and a target you can drag. The left graph \
                shows the echo as it arrives at each antenna, drawn as an arrow whose angle is its [[phase]]. The right \
                graph shows, for every possible direction, the angle the radar computes from the two arrows; the diamond is \
                the target now, the small dots are repeated measurements disturbed by noise.\n\n\
                If the computed direction is wrong, a red line and circle show where the radar thinks the target is.",
            why: "A radar measures distance from the delay of the echo and speed from its [[Doppler shift]]. Direction needs \
                something else: a second receive antenna.\n\n\
                An echo from straight ahead reaches both antennas at the same moment. An echo from the side travels a little \
                further to the far antenna: d sin(angle) further, where d is the spacing. At 24 GHz the [[wavelength]] is \
                only 12.4 mm, so even a fraction of a millimetre is a large part of a wave, and the two antennas receive the \
                wave at different points of its cycle. That phase difference, 360° x d / wavelength x sin(angle), is \
                measured by comparing the two receivers, and the angle follows by turning the formula round.\n\n\
                The catch: a phase repeats every 360°. The receiver can only tell where in the cycle the wave is, so it \
                always reads a value between -180° and +180°. As long as the antennas are at most half a wavelength apart, \
                the true phase difference never leaves that range, and every angle from -90° to +90° is unique. With \
                wider spacing, large angles produce phase differences beyond 180°; the reading wraps round, and the radar \
                computes an angle on the other side - a ghost.\n\n\
                Wider spacing has an advantage, though: the phase changes faster with the angle, so the same noise in the \
                phase measurement means a smaller error in the angle. Designs therefore choose between a wide view (half a \
                wavelength) and precise angles in a narrow view, or use more than two antennas to get both.",
            formulas: &[
                ("phase difference = 360° x spacing / wavelength x sin(angle)", "Half a wavelength apart, target at 30°: 360° x 0.5 x 0.5 = 90°."),
                ("unambiguous while |sin(angle)| <= wavelength / (2 x spacing)", "One wavelength apart: only up to ±30°. A target at 40° reads as -20.9°."),
            ],
            try_this: &[
                Experiment {
                    action: "Set the spacing to one wavelength, target at 40°.",
                    watch: "the radar reporting -20.9°, on the wrong side. Drag the target inside the grey lines at ±30° and the ghost disappears.",
                    preset: Some("wide"),
                },
                Experiment {
                    action: "Keep half a wavelength, add 10° of phase noise.",
                    watch: "the measurements scattering by about ±3° around the true angle.",
                    preset: Some("noisy"),
                },
                Experiment {
                    action: "Same noise, but two wavelengths apart.",
                    watch: "the scatter shrinking to under 1° - and the unambiguous view narrowing to ±14.5°.",
                    preset: Some("far-apart"),
                },
            ],
            leaves_out: "The target is a single point far away compared with the spacing, so the wave arrives as a straight front. \
                Only one direction is measured (left and right, not up and down; that needs a second pair of antennas). Two \
                targets at the same distance and speed would mix in both antennas and give one wrong angle; real radars \
                first separate targets by distance and speed (see the range-Doppler demo), then measure the angle of each. \
                Real chips use arrays of 4 to 16 virtual antennas and an FFT across them instead of one phase difference.",
            refs: &[
                "Two-element interferometer / uniform linear array: phase difference 2 pi d sin(theta) / lambda, unambiguous for d <= lambda / 2 (standard antenna-array result; the code's tests check the 30° -> 90° case and the ghost at d = lambda).",
            ],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_scatter_matches_the_experiments() {
        let lambda = C / F0;
        let spread = |wl: f64| {
            let e: Vec<f64> = aoa_estimates(5f64.to_radians(), wl * lambda, lambda, 10f64.to_radians(), 40, &mut Rng::new(1)).into_iter().flatten().collect();
            let m = e.iter().sum::<f64>() / e.len() as f64;
            (e.iter().map(|x| (x - m).powi(2)).sum::<f64>() / e.len() as f64).sqrt().to_degrees()
        };
        assert!((2.5..4.0).contains(&spread(0.5)), "{}", spread(0.5));
        assert!(spread(2.0) < 1.0, "{}", spread(2.0));
    }
}
