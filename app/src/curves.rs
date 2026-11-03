//! Editable curves: the Y/R/G/B tone curves and the six hue and saturation curves.
//! Points run 0..1 on both axes; the second group is neutral at 0.5.

use serde::{Deserialize, Serialize};

pub const TABLE: usize = 256;
pub type Points = Vec<[f32; 2]>;
/// Degrees of hue between the bottom and top of a Hue vs Hue graph.
pub const HUE_REACH: f32 = 120.0;
/// Change in light between the bottom and top of a graph that sets luminance.
pub const LIGHT_REACH: f32 = 1.4;

/// The six modes of the Curves tool, in menu order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    HueHue,
    HueSat,
    HueLum,
    LumSat,
    SatSat,
    SatLum,
}

impl Mode {
    pub const ALL: [Mode; 6] = [Mode::HueHue, Mode::HueSat, Mode::HueLum, Mode::LumSat, Mode::SatSat, Mode::SatLum];

    pub fn name(self) -> &'static str {
        match self {
            Mode::HueHue => "Hue vs Hue",
            Mode::HueSat => "Hue vs Sat",
            Mode::HueLum => "Hue vs Lum",
            Mode::LumSat => "Lum vs Sat",
            Mode::SatSat => "Sat vs Sat",
            Mode::SatLum => "Sat vs Lum",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Mode::HueHue => "Hue Vs Hue",
            Mode::HueSat => "Hue Vs Sat",
            Mode::HueLum => "Hue Vs Lum",
            Mode::LumSat => "Lum Vs Sat",
            Mode::SatSat => "Sat Vs Sat",
            Mode::SatLum => "Sat Vs Lum",
        }
    }

    /// What the two number boxes under the graph are called.
    pub fn labels(self) -> (&'static str, &'static str) {
        match self {
            Mode::HueHue => ("Input Hue", "Hue Rotate"),
            Mode::HueSat => ("Input Hue", "Saturation"),
            Mode::HueLum => ("Input Hue", "Luminance"),
            Mode::LumSat => ("Input Lum", "Saturation"),
            Mode::SatSat => ("Input Sat", "Saturation"),
            Mode::SatLum => ("Input Sat", "Luminance"),
        }
    }

    /// A point's position along the graph, in the unit its box shows.
    pub fn input_shown(self, x: f32) -> f32 {
        x * if self.by_hue() { 360.0 } else { 100.0 }
    }

    pub fn input_stored(self, v: f32) -> f32 {
        (v / if self.by_hue() { 360.0 } else { 100.0 }).clamp(0.0, 1.0)
    }

    /// A point's height, in what the curve actually does at that height.
    pub fn effect_shown(self, y: f32) -> f32 {
        match self {
            Mode::HueHue => (y - 0.5) * HUE_REACH,
            Mode::HueSat | Mode::LumSat | Mode::SatSat => y * 2.0,
            Mode::HueLum | Mode::SatLum => (y - 0.5) * LIGHT_REACH,
        }
    }

    pub fn effect_stored(self, v: f32) -> f32 {
        let y = match self {
            Mode::HueHue => v / HUE_REACH + 0.5,
            Mode::HueSat | Mode::LumSat | Mode::SatSat => v / 2.0,
            Mode::HueLum | Mode::SatLum => v / LIGHT_REACH + 0.5,
        };
        y.clamp(0.0, 1.0)
    }

    /// Hue curves wrap around from the right edge to the left.
    pub fn wraps(self) -> bool {
        matches!(self, Mode::HueHue | Mode::HueSat | Mode::HueLum)
    }

    pub fn by_hue(self) -> bool {
        self.wraps()
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Curves {
    pub master: Points,
    pub red: Points,
    pub green: Points,
    pub blue: Points,
    pub hue_hue: Points,
    pub hue_sat: Points,
    pub hue_lum: Points,
    pub lum_sat: Points,
    pub sat_sat: Points,
    pub sat_lum: Points,
}

impl Default for Curves {
    fn default() -> Self {
        let line = vec![[0.0, 0.0], [1.0, 1.0]];
        Self {
            master: line.clone(),
            red: line.clone(),
            green: line.clone(),
            blue: line,
            hue_hue: Vec::new(),
            hue_sat: Vec::new(),
            hue_lum: Vec::new(),
            lum_sat: Vec::new(),
            sat_sat: Vec::new(),
            sat_lum: Vec::new(),
        }
    }
}

impl Curves {
    pub fn mode_mut(&mut self, m: Mode) -> &mut Points {
        match m {
            Mode::HueHue => &mut self.hue_hue,
            Mode::HueSat => &mut self.hue_sat,
            Mode::HueLum => &mut self.hue_lum,
            Mode::LumSat => &mut self.lum_sat,
            Mode::SatSat => &mut self.sat_sat,
            Mode::SatLum => &mut self.sat_lum,
        }
    }

    pub fn channel_mut(&mut self, i: usize) -> &mut Points {
        match i {
            0 => &mut self.master,
            1 => &mut self.red,
            2 => &mut self.green,
            _ => &mut self.blue,
        }
    }

    fn has_tone(&self) -> bool {
        [&self.master, &self.red, &self.green, &self.blue].iter().any(|p| !is_line(p))
    }

    fn has_colour(&self) -> bool {
        [&self.hue_hue, &self.hue_sat, &self.hue_lum, &self.lum_sat, &self.sat_sat, &self.sat_lum]
            .iter()
            .any(|p| p.iter().any(|q| (q[1] - 0.5).abs() > 1e-4))
    }

    #[cfg(test)]
    pub fn is_neutral(&self) -> bool {
        !self.has_tone() && !self.has_colour()
    }
}

pub fn is_line(p: &[[f32; 2]]) -> bool {
    p.iter().all(|q| (q[0] - q[1]).abs() < 1e-4)
}

/// One curve sampled evenly, so a pixel only has to look it up.
#[derive(Clone)]
pub struct Table(Box<[f32; TABLE]>);

impl Table {
    pub fn tone(p: &[[f32; 2]]) -> Self {
        Self(sample(p, [0.0, 1.0], false))
    }

    pub fn flat(p: &[[f32; 2]], wraps: bool) -> Self {
        Self(sample(p, [0.5, 0.5], wraps))
    }

    #[inline]
    pub fn get(&self, x: f32) -> f32 {
        let t = x.clamp(0.0, 1.0) * (TABLE - 1) as f32;
        let i = (t as usize).min(TABLE - 2);
        let f = t - i as f32;
        self.0[i] * (1.0 - f) + self.0[i + 1] * f
    }
}

/// All the tables for one render; hue tables wrap, tone tables do not.
pub struct Baked {
    pub tone: Option<[Table; 4]>,
    pub colour: Option<[Table; 6]>,
}

impl Baked {
    pub fn new(c: &Curves) -> Self {
        let tone = c.has_tone().then(|| {
            [Table::tone(&c.master), Table::tone(&c.red), Table::tone(&c.green), Table::tone(&c.blue)]
        });
        let colour = c.has_colour().then(|| {
            [
                Table::flat(&c.hue_hue, true),
                Table::flat(&c.hue_sat, true),
                Table::flat(&c.hue_lum, true),
                Table::flat(&c.lum_sat, false),
                Table::flat(&c.sat_sat, false),
                Table::flat(&c.sat_lum, false),
            ]
        });
        Self { tone, colour }
    }
}

/// Smooth curve through the points that never overshoots between them.
fn sample(points: &[[f32; 2]], ends: [f32; 2], wraps: bool) -> Box<[f32; TABLE]> {
    let mut pts: Vec<[f32; 2]> = points.to_vec();
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
    pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-4);
    if pts.is_empty() {
        pts = vec![[0.0, ends[0]], [1.0, ends[1]]];
    }
    if wraps {
        // A full turn of the curve on each side, so the slope agrees across the join too.
        let turn: Vec<[f32; 2]> = pts.clone();
        let mut around: Vec<[f32; 2]> = turn.iter().map(|q| [q[0] - 1.0, q[1]]).collect();
        around.extend_from_slice(&turn);
        around.extend(turn.iter().map(|q| [q[0] + 1.0, q[1]]));
        pts = around;
    }
    let mut out = Box::new([0.0; TABLE]);
    if pts.len() == 1 {
        out.fill(pts[0][1]);
        return out;
    }
    let tangents = slopes(&pts);
    for (i, o) in out.iter_mut().enumerate() {
        *o = eval(&pts, &tangents, i as f32 / (TABLE - 1) as f32);
    }
    out
}

/// Fritsch and Carlson's limited slopes.
fn slopes(p: &[[f32; 2]]) -> Vec<f32> {
    let n = p.len();
    let secant: Vec<f32> = p.windows(2).map(|w| (w[1][1] - w[0][1]) / (w[1][0] - w[0][0]).max(1e-5)).collect();
    let mut m = vec![0.0; n];
    m[0] = secant[0];
    m[n - 1] = secant[n - 2];
    for i in 1..n - 1 {
        m[i] = if secant[i - 1] * secant[i] <= 0.0 { 0.0 } else { (secant[i - 1] + secant[i]) / 2.0 };
    }
    for i in 0..n - 1 {
        let s = secant[i];
        if s.abs() < 1e-6 {
            m[i] = 0.0;
            m[i + 1] = 0.0;
            continue;
        }
        let (a, b) = (m[i] / s, m[i + 1] / s);
        let h = (a * a + b * b).sqrt();
        if h > 3.0 {
            m[i] = 3.0 * a / h * s;
            m[i + 1] = 3.0 * b / h * s;
        }
    }
    m
}

fn eval(p: &[[f32; 2]], m: &[f32], x: f32) -> f32 {
    if x <= p[0][0] {
        return p[0][1];
    }
    let n = p.len();
    if x >= p[n - 1][0] {
        return p[n - 1][1];
    }
    let i = p.partition_point(|q| q[0] <= x).saturating_sub(1).min(n - 2);
    let (a, b) = (p[i], p[i + 1]);
    let h = (b[0] - a[0]).max(1e-5);
    let t = (x - a[0]) / h;
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * a[1]
        + (t3 - 2.0 * t2 + t) * h * m[i]
        + (-2.0 * t3 + 3.0 * t2) * b[1]
        + (t3 - t2) * h * m[i + 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_straight_curve_is_a_straight_table() {
        let t = Table::tone(&[[0.0, 0.0], [1.0, 1.0]]);
        assert!((t.get(0.37) - 0.37).abs() < 1e-3);
    }

    #[test]
    fn the_curve_goes_through_every_point_without_overshoot() {
        let pts = [[0.0, 0.0], [0.5, 0.8], [0.6, 0.82], [1.0, 1.0]];
        let t = Table::tone(&pts);
        assert!((t.get(0.5) - 0.8).abs() < 0.01);
        for i in 0..TABLE {
            let v = t.0[i];
            assert!((0.0..=1.0).contains(&v));
            if i > 0 {
                assert!(v + 1e-4 >= t.0[i - 1], "dip at {i}");
            }
        }
    }

    #[test]
    fn hue_curves_meet_at_the_wrap() {
        let t = Table::flat(&[[0.1, 0.8], [0.6, 0.3]], true);
        assert!((t.get(0.0) - t.get(1.0)).abs() < 0.01);
    }

    #[test]
    fn the_number_boxes_undo_their_own_conversion() {
        for m in Mode::ALL {
            let (x, y) = (0.37, 0.71);
            assert!((m.input_stored(m.input_shown(x)) - x).abs() < 1e-5, "{m:?}");
            assert!((m.effect_stored(m.effect_shown(y)) - y).abs() < 1e-5, "{m:?}");
        }
    }

    #[test]
    fn a_flat_point_shows_no_change() {
        assert_eq!(Mode::HueHue.effect_shown(0.5), 0.0);
        assert_eq!(Mode::HueLum.effect_shown(0.5), 0.0);
        assert_eq!(Mode::HueSat.effect_shown(0.5), 1.0);
    }

    #[test]
    fn empty_curves_are_neutral() {
        assert!(Curves::default().is_neutral());
        let c = Curves { hue_sat: vec![[0.3, 0.9]], ..Curves::default() };
        assert!(!c.is_neutral());
    }
}
