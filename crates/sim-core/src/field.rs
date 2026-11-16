//! A 2D wave field: the wave equation solved on a grid (finite differences in time).
//!
//! This is the same maths that describes the electric field of a radio wave seen from
//! above, a ripple tank, or sound in a flat room. Each step, a cell looks at its four
//! neighbours; a wave therefore travels [`COURANT`] cells per step, in every direction.

/// Distance a wave travels per time step, in cells. Must stay below 0.707 for stability.
pub const COURANT: f32 = 0.5;

#[derive(Clone, Debug)]
pub struct Field {
    pub w: usize,
    pub h: usize,
    cur: Vec<f32>,
    prev: Vec<f32>,
    wall: Vec<bool>,
    absorb: Vec<f32>,
    pub steps: u64,
}

impl Field {
    /// Create a field. `border` cells along each edge soak up waves so the edges act
    /// like open space instead of mirrors.
    pub fn new(w: usize, h: usize, border: usize) -> Self {
        let mut absorb = vec![1.0f32; w * h];
        if border > 0 {
            for y in 0..h {
                for x in 0..w {
                    let d = x.min(y).min(w - 1 - x).min(h - 1 - y);
                    if d < border {
                        let depth = (border - d) as f32 / border as f32;
                        absorb[y * w + x] = 1.0 - 0.09 * depth * depth;
                    }
                }
            }
        }
        Self { w, h, cur: vec![0.0; w * h], prev: vec![0.0; w * h], wall: vec![false; w * h], absorb, steps: 0 }
    }

    pub fn clear(&mut self) {
        self.cur.fill(0.0);
        self.prev.fill(0.0);
        self.steps = 0;
    }

    pub fn clear_walls(&mut self) {
        self.wall.fill(false);
    }

    pub fn set_wall(&mut self, x: usize, y: usize, on: bool) {
        if x < self.w && y < self.h {
            self.wall[y * self.w + x] = on;
        }
    }

    pub fn is_wall(&self, x: usize, y: usize) -> bool {
        x < self.w && y < self.h && self.wall[y * self.w + x]
    }

    pub fn values(&self) -> &[f32] {
        &self.cur
    }

    pub fn get(&self, x: usize, y: usize) -> f32 {
        self.cur[y * self.w + x]
    }

    /// Push the field at one cell (a "soft" source: waves pass through it undisturbed).
    pub fn inject(&mut self, x: usize, y: usize, amount: f32) {
        if x > 0 && y > 0 && x < self.w - 1 && y < self.h - 1 && !self.wall[y * self.w + x] {
            self.cur[y * self.w + x] += amount;
        }
    }

    /// Advance one time step. Walls are perfect reflectors (the field is held at zero).
    pub fn step(&mut self) {
        let (w, h) = (self.w, self.h);
        let c2 = COURANT * COURANT;
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let i = y * w + x;
                if self.wall[i] {
                    self.prev[i] = 0.0;
                    continue;
                }
                let neighbours = self.cur[i - 1] + self.cur[i + 1] + self.cur[i - w] + self.cur[i + w];
                let next = 2.0 * self.cur[i] - self.prev[i] + c2 * (neighbours - 4.0 * self.cur[i]);
                self.prev[i] = next * self.absorb[i];
            }
        }
        std::mem::swap(&mut self.cur, &mut self.prev);
        // `prev` now holds the old `cur`; damp it too so the border really absorbs.
        for (p, a) in self.prev.iter_mut().zip(&self.absorb) {
            *p *= a;
        }
        self.steps += 1;
    }
}

/// Wavelength in cells of a source that completes one cycle every `period_steps` steps.
pub fn wavelength_cells(period_steps: f32) -> f32 {
    COURANT * period_steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waves_travel_at_the_courant_speed() {
        let mut f = Field::new(201, 201, 0);
        f.inject(100, 100, 1.0);
        for _ in 0..100 {
            f.step();
        }
        // After 100 steps the front is 50 cells out: something at 40, nothing at 60.
        assert!(f.get(140, 100).abs() > 1e-4, "{}", f.get(140, 100));
        assert!(f.get(160, 100).abs() < 1e-6, "{}", f.get(160, 100));
    }

    #[test]
    fn stays_bounded_with_a_continuous_source() {
        let mut f = Field::new(120, 120, 20);
        for n in 0..3000 {
            f.inject(60, 60, (n as f32 * std::f32::consts::TAU / 24.0).sin());
            f.step();
        }
        let max = f.values().iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!(max.is_finite() && max < 50.0, "{max}");
    }

    #[test]
    fn a_wall_casts_a_shadow_and_the_border_absorbs() {
        let run = |wall: bool, border: usize| {
            let mut f = Field::new(160, 120, border);
            if wall {
                (0..160).for_each(|x| f.set_wall(x, 60, true));
            }
            let mut behind = 0f32;
            for n in 0..600 {
                f.inject(80, 30, (n as f32 * std::f32::consts::TAU / 20.0).sin());
                f.step();
                behind = behind.max(f.get(80, 90).abs());
            }
            (behind, f)
        };
        assert!(run(true, 20).0 < 1e-6);
        assert!(run(false, 20).0 > 0.01);
        // With an absorbing border the field settles lower than inside a closed mirror box.
        let energy = |f: &Field| f.values().iter().map(|v| v * v).sum::<f32>();
        assert!(energy(&run(false, 20).1) < 0.5 * energy(&run(false, 0).1));
    }
}
