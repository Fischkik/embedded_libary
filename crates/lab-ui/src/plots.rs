//! Plot helpers so every demo's graphs look and behave the same.

use egui::Color32;
use egui_plot::{Legend, Line, Plot, PlotPoints};

/// A plot with the house defaults: fixed height, legend, no scroll-wheel capture
/// (so the page can still be scrolled), drag to pan, ctrl+scroll or right-drag to zoom,
/// double-click to reset.
pub fn plot<'a>(id: &str, height: f32) -> Plot<'a> {
    Plot::new(id.to_owned())
        .height(height)
        .legend(Legend::default())
        .allow_scroll(false)
        .show_axes([true, true])
        .set_margin_fraction(egui::vec2(0.02, 0.08))
}

/// A named line from `[x, y]` points.
///
/// egui_plot's "band" shortcut (which draws dense lines as one min/max shape per pixel
/// column) is switched off: it turns square waveforms made of a few corner points into
/// filled wedges. Our series are small enough not to need it.
pub fn line<'a>(name: &str, points: Vec<[f64; 2]>, color: Color32) -> Line<'a> {
    Line::new(name.to_owned(), PlotPoints::from(points)).color(color).width(1.6).allow_band(false)
}

/// `[x, y]` points for samples spaced `dx` apart starting at `x0`.
pub fn series(ys: &[f64], x0: f64, dx: f64) -> Vec<[f64; 2]> {
    ys.iter().enumerate().map(|(i, y)| [x0 + i as f64 * dx, *y]).collect()
}

/// Points of a square-edged digital waveform between `t0` and `t1`.
/// `edges` are `(time, new level)`; the trace is drawn from `base` (low) to `base + height` (high).
pub fn digital(edges: &[(f64, bool)], initial: bool, t0: f64, t1: f64, base: f64, height: f64) -> Vec<[f64; 2]> {
    let y = |level: bool| if level { base + height } else { base };
    let mut level = initial;
    let mut pts = Vec::with_capacity(edges.len() * 2 + 2);
    for &(t, l) in edges.iter().filter(|e| e.0 < t0) {
        let _ = t;
        level = l;
    }
    pts.push([t0, y(level)]);
    for &(t, l) in edges.iter().filter(|e| e.0 >= t0 && e.0 <= t1) {
        pts.push([t, y(level)]);
        pts.push([t, y(l)]);
        level = l;
    }
    pts.push([t1, y(level)]);
    pts
}

/// The four corners of a filled bar from `x0` to `x1` in lane `y0..y1`, for `Polygon`.
pub fn bar(x0: f64, x1: f64, y0: f64, y1: f64) -> Vec<[f64; 2]> {
    vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digital_trace_has_vertical_edges() {
        let pts = digital(&[(1.0, true), (3.0, false)], false, 0.0, 4.0, 0.0, 1.0);
        assert_eq!(pts, vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [3.0, 1.0], [3.0, 0.0], [4.0, 0.0]]);
        // An edge before the window sets the starting level.
        let pts = digital(&[(1.0, true)], false, 2.0, 4.0, 0.0, 1.0);
        assert_eq!(pts, vec![[2.0, 1.0], [4.0, 1.0]]);
    }
}
