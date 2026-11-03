//! Colour maths shared by the engine: camera log curves, gamuts, hue spaces.
//! Log formulas and primaries are the makers' published ones; see MECHANICS.md.

use serde::{Deserialize, Serialize};

pub type M3 = [[f32; 3]; 3];

const D65: [f32; 2] = [0.3127, 0.3290];
const REC709: [[f32; 2]; 3] = [[0.64, 0.33], [0.30, 0.60], [0.15, 0.06]];
const REC2020: [[f32; 2]; 3] = [[0.708, 0.292], [0.170, 0.797], [0.131, 0.046]];
const SGAMUT3_CINE: [[f32; 2]; 3] = [[0.766, 0.275], [0.225, 0.800], [0.089, -0.087]];
const ARRI_WG3: [[f32; 2]; 3] = [[0.6840, 0.3130], [0.2210, 0.8480], [0.0861, -0.1020]];
const D_GAMUT: [[f32; 2]; 3] = [[0.71, 0.31], [0.21, 0.88], [0.09, -0.08]];
const RED_WG: [[f32; 2]; 3] = [[0.780308, 0.304253], [0.121595, 1.493994], [0.095612, -0.084589]];
const DAVINCI_WG: [[f32; 2]; 3] = [[0.8000, 0.3130], [0.1682, 0.9877], [0.0790, -0.1155]];

/// The camera format a clip was shot in.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum Source {
    #[default]
    Rec709,
    AppleLog,
    LogC3,
    SLog3,
    DLog,
    ILog,
    FLog2,
    Log3G10,
    Intermediate,
    Hlg,
    Pq,
}

impl Source {
    pub const ALL: [Source; 11] = [
        Source::Rec709,
        Source::AppleLog,
        Source::LogC3,
        Source::SLog3,
        Source::DLog,
        Source::ILog,
        Source::FLog2,
        Source::Log3G10,
        Source::Intermediate,
        Source::Hlg,
        Source::Pq,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Source::Rec709 => "Rec.709",
            Source::AppleLog => "Apple Log",
            Source::LogC3 => "ARRI LogC3",
            Source::SLog3 => "Sony S-Log3",
            Source::DLog => "DJI D-Log",
            Source::ILog => "Insta360 I-Log",
            Source::FLog2 => "Fujifilm F-Log2",
            Source::Log3G10 => "RED Log3G10",
            Source::Intermediate => "DaVinci Intermediate",
            Source::Hlg => "HDR HLG",
            Source::Pq => "HDR PQ",
        }
    }

    pub fn gamut_name(self) -> &'static str {
        match self {
            Source::Rec709 => "Rec.709",
            Source::AppleLog | Source::ILog | Source::FLog2 | Source::Hlg | Source::Pq => "Rec.2020",
            Source::LogC3 => "ARRI Wide Gamut 3",
            Source::SLog3 => "S-Gamut3.Cine",
            Source::DLog => "D-Gamut",
            Source::Log3G10 => "REDWideGamutRGB",
            Source::Intermediate => "DaVinci Wide Gamut",
        }
    }

    fn primaries(self) -> [[f32; 2]; 3] {
        match self {
            Source::Rec709 => REC709,
            Source::AppleLog | Source::ILog | Source::FLog2 | Source::Hlg | Source::Pq => REC2020,
            Source::LogC3 => ARRI_WG3,
            Source::SLog3 => SGAMUT3_CINE,
            Source::DLog => D_GAMUT,
            Source::Log3G10 => RED_WG,
            Source::Intermediate => DAVINCI_WG,
        }
    }

    /// Log signal to scene light, where 0.18 is middle grey.
    #[inline]
    pub fn decode(self, v: f32) -> f32 {
        match self {
            Source::Rec709 => to_linear(v),
            Source::AppleLog => {
                let (r0, rt, c, b, g, d) = (-0.05641088, 0.01, 47.28711236, 0.00964052, 0.08550479, 0.69336945);
                let pt = c * (rt - r0) * (rt - r0);
                if v < 0.0 {
                    r0
                } else if v < pt {
                    (v / c).sqrt() + r0
                } else {
                    2f32.powf((v - d) / g) - b
                }
            }
            Source::LogC3 => {
                let (cut, a, b, c, d, e, f) = (0.010591, 5.555556, 0.052272, 0.247190, 0.385537, 5.367655, 0.092809);
                if v > e * cut + f { (10f32.powf((v - d) / c) - b) / a } else { (v - f) / e }
            }
            Source::SLog3 => {
                let x = v * 1023.0;
                if x >= 171.210_29 {
                    10f32.powf((x - 420.0) / 261.5) * 0.19 - 0.01
                } else {
                    (x - 95.0) * 0.01125 / (171.210_29 - 95.0)
                }
            }
            Source::DLog => {
                if v <= 0.14 { (v - 0.0929) / 6.025 } else { (10f32.powf(3.89616 * v - 2.27752) - 0.0108) / 0.9892 }
            }
            Source::ILog => {
                let (a, b, d, e, t, cut) = (5.77837328, 0.09055934, 0.623992, 0.280055, 0.01, 0.154402);
                if v < cut { (v - b) / a } else { 10f32.powf((v - d) / e) - t }
            }
            Source::FLog2 => {
                let (a, b, c, d, e, f, cut) = (5.555556, 0.064829, 0.245281, 0.384316, 8.799461, 0.092864, 0.100686685);
                if v >= cut { 10f32.powf((v - d) / c) / a - b / a } else { (v - f) / e }
            }
            Source::Log3G10 => {
                let (a, b, c, g) = (0.224282, 155.975327, 0.01, 15.1927);
                if v < 0.0 { v / g - c } else { (10f32.powf(v / a) - 1.0) / b - c }
            }
            Source::Intermediate => di_decode(v),
            Source::Hlg => {
                let (a, b, c) = (0.17883277, 0.28466892, 0.55991073);
                let v = v.max(0.0);
                HLG_GAIN * if v <= 0.5 { v * v / 3.0 } else { (((v - c) / a).exp() + b) / 12.0 }
            }
            Source::Pq => {
                let (m1, m2, c1, c2, c3) = (0.1593017578125, 78.84375, 0.8359375, 18.8515625, 18.6875);
                let p = v.max(0.0).powf(1.0 / m2);
                let nits = 10000.0 * ((p - c1).max(0.0) / (c2 - c3 * p)).powf(1.0 / m1);
                nits / PQ_GREY_NITS * 0.18
            }
        }
    }
}

/// BT.2408 puts 18% grey at 38% of the HLG signal; this brings that back to 0.18.
const HLG_GAIN: f32 = 3.74;
/// BT.2408's 18% grey on a PQ display.
const PQ_GREY_NITS: f32 = 26.0;

const DI_A: f32 = 0.0075;
const DI_B: f32 = 7.0;
const DI_C: f32 = 0.07329248;
const DI_M: f32 = 10.44426855;
const DI_LIN_CUT: f32 = 0.00262409;
const DI_LOG_CUT: f32 = 0.02740668;

#[inline]
pub fn di_decode(t: f32) -> f32 {
    if t <= DI_LOG_CUT { t / DI_M } else { 2f32.powf(t / DI_C - DI_B) - DI_A }
}

#[inline]
pub fn di_encode(l: f32) -> f32 {
    if l <= DI_LIN_CUT { l * DI_M } else { DI_C * ((l + DI_A).log2() + DI_B) }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum ToneMap {
    Clip,
    #[default]
    Filmic,
    Soft,
}

impl ToneMap {
    pub const ALL: [ToneMap; 3] = [ToneMap::Clip, ToneMap::Filmic, ToneMap::Soft];
    pub fn name(self) -> &'static str {
        match self {
            ToneMap::Clip => "None",
            ToneMap::Filmic => "Filmic",
            ToneMap::Soft => "Soft Rolloff",
        }
    }

    #[inline]
    fn apply(self, x: f32) -> f32 {
        let x = x.max(0.0);
        match self {
            ToneMap::Clip => x,
            ToneMap::Filmic => {
                let x = x * 0.8;
                (x * (2.51 * x + 0.03) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
            }
            ToneMap::Soft => {
                let x = x * 1.1;
                x * (1.0 + x / 36.0) / (1.0 + x)
            }
        }
    }
}

/// Settings of the Color Space Transform tool.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Cst {
    pub source: Source,
    pub tone: ToneMap,
    /// Grades in DaVinci Wide Gamut / Intermediate, then converts at the very end.
    pub wide: bool,
}

impl Cst {
    pub fn is_neutral(&self) -> bool {
        self.source == Source::Rec709 && !self.wide
    }
}

/// `Cst` with its matrices worked out, ready to use on every pixel.
#[derive(Clone)]
pub struct CstStep {
    source: Source,
    tone: ToneMap,
    wide: bool,
    to_display: M3,
    to_wide: M3,
    wide_to_display: M3,
}

impl CstStep {
    pub fn new(c: &Cst) -> Self {
        let display = rgb_to_xyz(REC709);
        let wide = rgb_to_xyz(DAVINCI_WG);
        let from = rgb_to_xyz(c.source.primaries());
        Self {
            source: c.source,
            tone: c.tone,
            wide: c.wide,
            to_display: mul(&invert(&display), &from),
            to_wide: mul(&invert(&wide), &from),
            wide_to_display: mul(&invert(&display), &wide),
        }
    }

    fn tone_of_source(&self) -> ToneMap {
        if self.source == Source::Rec709 { ToneMap::Clip } else { self.tone }
    }

    #[inline]
    pub fn input(&self, c: [f32; 3]) -> [f32; 3] {
        if self.source == Source::Rec709 && !self.wide {
            return c;
        }
        let s = self.source;
        let scene = [s.decode(c[0]), s.decode(c[1]), s.decode(c[2])];
        if self.wide {
            apply(&self.to_wide, scene).map(di_encode)
        } else {
            let tone = self.tone_of_source();
            apply(&self.to_display, scene).map(|v| to_srgb(tone.apply(v)))
        }
    }

    /// The last colour step when grading in the wide space.
    #[inline]
    pub fn output(&self, c: [f32; 3]) -> [f32; 3] {
        if !self.wide {
            return c;
        }
        let tone = self.tone_of_source();
        apply(&self.wide_to_display, c.map(di_decode)).map(|v| to_srgb(tone.apply(v)))
    }
}

fn rgb_to_xyz(p: [[f32; 2]; 3]) -> M3 {
    let xyz = |c: [f32; 2]| [c[0] / c[1], 1.0, (1.0 - c[0] - c[1]) / c[1]];
    let (r, g, b, w) = (xyz(p[0]), xyz(p[1]), xyz(p[2]), xyz(D65));
    let m = [[r[0], g[0], b[0]], [r[1], g[1], b[1]], [r[2], g[2], b[2]]];
    let s = apply(&invert(&m), w);
    [
        [m[0][0] * s[0], m[0][1] * s[1], m[0][2] * s[2]],
        [m[1][0] * s[0], m[1][1] * s[1], m[1][2] * s[2]],
        [m[2][0] * s[0], m[2][1] * s[1], m[2][2] * s[2]],
    ]
}

#[inline]
pub fn apply(m: &M3, c: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * c[0] + m[0][1] * c[1] + m[0][2] * c[2],
        m[1][0] * c[0] + m[1][1] * c[1] + m[1][2] * c[2],
        m[2][0] * c[0] + m[2][1] * c[1] + m[2][2] * c[2],
    ]
}

fn mul(a: &M3, b: &M3) -> M3 {
    let mut o = [[0.0; 3]; 3];
    for (y, row) in o.iter_mut().enumerate() {
        for (x, v) in row.iter_mut().enumerate() {
            *v = a[y][0] * b[0][x] + a[y][1] * b[1][x] + a[y][2] * b[2][x];
        }
    }
    o
}

fn invert(m: &M3) -> M3 {
    let [[a, b, c], [d, e, f], [g, h, i]] = *m;
    let (ca, cb, cc) = (e * i - f * h, f * g - d * i, d * h - e * g);
    let det = 1.0 / (a * ca + b * cb + c * cc);
    [
        [ca * det, (c * h - b * i) * det, (b * f - c * e) * det],
        [cb * det, (a * i - c * g) * det, (c * d - a * f) * det],
        [cc * det, (b * g - a * h) * det, (a * e - b * d) * det],
    ]
}

#[inline]
pub fn to_linear(v: f32) -> f32 {
    let v = v.max(0.0);
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

#[inline]
pub fn to_srgb(v: f32) -> f32 {
    let v = v.max(0.0);
    if v < 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

/// Hue in degrees, then saturation and value from 0 to 1.
#[inline]
pub fn to_hsv(c: [f32; 3]) -> [f32; 3] {
    let mx = c[0].max(c[1]).max(c[2]);
    let mn = c[0].min(c[1]).min(c[2]);
    let d = mx - mn;
    if d < 1e-6 || mx <= 0.0 {
        return [0.0, 0.0, mx];
    }
    let h = if mx == c[0] {
        ((c[1] - c[2]) / d).rem_euclid(6.0)
    } else if mx == c[1] {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    };
    [h * 60.0, d / mx, mx]
}

#[inline]
pub fn from_hsv(hsv: [f32; 3]) -> [f32; 3] {
    let [h, s, v] = hsv;
    let h = h.rem_euclid(360.0) / 60.0;
    let i = h.floor();
    let f = h - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i as i32 % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// Display colour to Oklab, the space where lightness can move without moving hue.
#[inline]
pub fn to_oklab(c: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = c.map(to_linear);
    let l = (0.41222147 * r + 0.53633254 * g + 0.05144599 * b).max(0.0).cbrt();
    let m = (0.21190349 * r + 0.6806995 * g + 0.10739698 * b).max(0.0).cbrt();
    let s = (0.08830246 * r + 0.28171884 * g + 0.6299787 * b).max(0.0).cbrt();
    [
        0.21045426 * l + 0.7936178 * m - 0.00407205 * s,
        1.9779985 * l - 2.4285922 * m + 0.4505937 * s,
        0.02590404 * l + 0.78277177 * m - 0.80867577 * s,
    ]
}

#[inline]
pub fn from_oklab(lab: [f32; 3]) -> [f32; 3] {
    let [l, a, b] = lab;
    let lm = l + 0.39633778 * a + 0.21580376 * b;
    let mm = l - 0.10556135 * a - 0.06385417 * b;
    let sm = l - 0.08948418 * a - 1.2914855 * b;
    let (x, y, z) = (lm * lm * lm, mm * mm * mm, sm * sm * sm);
    [
        to_srgb(4.0767417 * x - 3.3077116 * y + 0.23096994 * z),
        to_srgb(-1.268438 * x + 2.6097574 * y - 0.3413194 * z),
        to_srgb(-0.0041960863 * x - 0.7034186 * y + 1.7076147 * z),
    ]
}

/// Raised-cosine window on the colour circle: 1 at `centre`, 0 at `half` away.
#[inline]
pub fn hue_window(h: f32, centre: f32, half: f32) -> f32 {
    let d = hue_gap(h, centre).abs();
    if d >= half { 0.0 } else { 0.5 + 0.5 * (std::f32::consts::PI * d / half).cos() }
}

/// Signed shortest way round the colour circle, from -180 to 180.
#[inline]
pub fn hue_gap(h: f32, centre: f32) -> f32 {
    (h - centre + 180.0).rem_euclid(360.0) - 180.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, by: f32) -> bool {
        (a - b).abs() < by
    }

    #[test]
    fn each_log_curve_puts_grey_where_the_maker_says() {
        assert!(close(Source::SLog3.decode(420.0 / 1023.0), 0.18, 0.002));
        assert!(close(Source::LogC3.decode(0.391), 0.18, 0.003));
        assert!(close(Source::Log3G10.decode(1.0 / 3.0), 0.18, 0.02));
        assert!(close(Source::FLog2.decode(0.3910), 0.18, 0.01));
        assert!(close(Source::AppleLog.decode(0.4877), 0.18, 0.01));
    }

    #[test]
    fn i_log_meets_its_published_reference_points() {
        assert!(close(Source::ILog.decode(93.0 / 1023.0), 0.0, 0.001));
        assert!(close(Source::ILog.decode(432.0 / 1023.0), 0.18, 0.002));
        assert!(close(Source::ILog.decode(1.0), 22.0, 0.2));
    }

    #[test]
    fn i_log_has_no_step_where_its_two_parts_meet() {
        let (a, b) = (Source::ILog.decode(0.15439), Source::ILog.decode(0.15441));
        assert!((b - a).abs() < 0.001, "{a} {b}");
    }

    #[test]
    fn hdr_grey_lands_on_scene_grey() {
        assert!(close(Source::Hlg.decode(0.38), 0.18, 0.01));
        assert!(close(Source::Pq.decode(0.38), 0.18, 0.015));
    }

    #[test]
    fn hdr_highlights_stay_above_diffuse_white() {
        let s = CstStep::new(&Cst { source: Source::Pq, ..Cst::default() });
        let (white, peak) = (s.input([0.58; 3])[1], s.input([0.75; 3])[1]);
        assert!(white > 0.75 && peak >= white && peak <= 1.0, "{white} {peak}");
    }

    #[test]
    fn intermediate_round_trips() {
        for l in [0.001, 0.01, 0.18, 1.0, 16.0] {
            assert!(close(di_decode(di_encode(l)), l, l * 1e-3 + 1e-4));
        }
    }

    #[test]
    fn rec709_passes_through_when_nothing_is_set() {
        let s = CstStep::new(&Cst::default());
        assert_eq!(s.input([0.2, 0.5, 0.8]), [0.2, 0.5, 0.8]);
    }

    #[test]
    fn wide_workflow_returns_to_the_same_colour() {
        let s = CstStep::new(&Cst { wide: true, ..Cst::default() });
        let c = [0.3, 0.55, 0.7];
        let back = s.output(s.input(c));
        assert!((0..3).all(|i| close(back[i], c[i], 0.004)), "{back:?}");
    }

    #[test]
    fn log_grey_lands_near_display_mid_grey() {
        let s = CstStep::new(&Cst { source: Source::SLog3, ..Cst::default() });
        let c = s.input([420.0 / 1023.0; 3]);
        assert!(close(c[0], c[1], 0.01) && (0.4..0.62).contains(&c[0]), "{c:?}");
    }

    #[test]
    fn matrices_keep_white_white() {
        let s = CstStep::new(&Cst { source: Source::LogC3, ..Cst::default() });
        let w = apply(&s.to_display, [1.0; 3]);
        assert!(w.iter().all(|v| close(*v, 1.0, 1e-3)), "{w:?}");
    }

    #[test]
    fn hsv_and_oklab_round_trip() {
        let c = [0.25, 0.6, 0.45];
        let a = from_hsv(to_hsv(c));
        let b = from_oklab(to_oklab(c));
        assert!((0..3).all(|i| close(a[i], c[i], 1e-3) && close(b[i], c[i], 2e-3)), "{a:?} {b:?}");
    }
}
