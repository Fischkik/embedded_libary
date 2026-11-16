//! A 2D picture made from numbers (a field, a spectrogram, a range-Doppler map),
//! drawn as one texture that is updated in place.

use egui::{Color32, ColorImage, Rect, TextureHandle, TextureOptions};

pub struct Heat {
    name: String,
    texture: Option<TextureHandle>,
}

impl Heat {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_owned(), texture: None }
    }

    /// Replace the picture: `color(i)` gives the colour of pixel `i` (row by row).
    pub fn update(&mut self, ctx: &egui::Context, w: usize, h: usize, smooth: bool, color: impl Fn(usize) -> Color32) {
        let image = ColorImage::new([w, h], (0..w * h).map(color).collect());
        let options = if smooth { TextureOptions::LINEAR } else { TextureOptions::NEAREST };
        match &mut self.texture {
            Some(t) => t.set(image, options),
            None => self.texture = Some(ctx.load_texture(self.name.clone(), image, options)),
        }
    }

    pub fn id(&self) -> Option<egui::TextureId> {
        self.texture.as_ref().map(|t| t.id())
    }

    /// Draw the picture stretched over `rect`.
    pub fn paint(&self, painter: &egui::Painter, rect: Rect) {
        if let Some(id) = self.id() {
            painter.image(id, rect, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
    }
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> Color32 {
    let c = |i: usize| (a[i] + (b[i] - a[i]) * t.clamp(0.0, 1.0)) as u8;
    Color32::from_rgb(c(0), c(1), c(2))
}

/// Colour for a value that swings around zero, `-1..=1`: blue for negative, orange for
/// positive, the background colour at zero. Used for wave fields.
pub fn diverging(v: f32, dark: bool) -> Color32 {
    let zero = if dark { [18.0, 21.0, 27.0] } else { [244.0, 245.0, 247.0] };
    let pos = if dark { [255.0, 170.0, 40.0] } else { [213.0, 94.0, 0.0] };
    let neg = if dark { [70.0, 170.0, 255.0] } else { [0.0, 114.0, 178.0] };
    let v = v.clamp(-1.0, 1.0);
    if v >= 0.0 { mix(zero, pos, v) } else { mix(zero, neg, -v) }
}

/// Colour for a level `0..=1`, dark to bright (a simple perceptual ramp).
/// Used for spectrograms and range-Doppler maps.
pub fn ramp(v: f32) -> Color32 {
    const STOPS: [[f32; 3]; 5] = [[13.0, 8.0, 60.0], [84.0, 15.0, 110.0], [187.0, 55.0, 84.0], [249.0, 142.0, 9.0], [252.0, 255.0, 164.0]];
    let x = v.clamp(0.0, 1.0) * 4.0;
    let i = (x as usize).min(3);
    mix(STOPS[i], STOPS[i + 1], x - i as f32)
}
