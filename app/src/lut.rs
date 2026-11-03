//! 3D colour lookup tables: reading and writing .cube files, and sampling them.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

#[derive(Clone)]
pub struct Lut3D {
    pub size: usize,
    /// Red changes fastest, matching the .cube file order.
    pub data: Vec<[f32; 3]>,
}

impl Lut3D {
    pub fn from_fn(size: usize, f: impl Fn([f32; 3]) -> [f32; 3]) -> Self {
        let mut data = Vec::with_capacity(size * size * size);
        let d = (size - 1) as f32;
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    data.push(f([r as f32 / d, g as f32 / d, b as f32 / d]));
                }
            }
        }
        Self { size, data }
    }

    /// Reads the text of a .cube file. Only 3D tables are supported.
    pub fn parse_cube(text: &str) -> Result<Self, String> {
        let mut size = 0usize;
        let mut dmin = [0.0f32; 3];
        let mut dmax = [1.0f32; 3];
        let mut data = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let upper = line.to_ascii_uppercase();
            if upper.starts_with("TITLE") {
                continue;
            }
            if upper.starts_with("LUT_1D_SIZE") {
                return Err("This file holds a 1D table; only 3D .cube files work here.".into());
            }
            if upper.starts_with("LUT_3D_SIZE") {
                size = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|v| v.parse().ok())
                    .ok_or("The table size is not readable.")?;
                continue;
            }
            if upper.starts_with("DOMAIN_MIN") || upper.starts_with("DOMAIN_MAX") {
                let v: Vec<f32> = line.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
                if v.len() == 3 {
                    let t = if upper.starts_with("DOMAIN_MIN") { &mut dmin } else { &mut dmax };
                    *t = [v[0], v[1], v[2]];
                }
                continue;
            }
            let v: Vec<f32> = line.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            if v.len() >= 3 {
                data.push([v[0], v[1], v[2]]);
            }
        }
        if !(2..=129).contains(&size) {
            return Err("No usable table size was found.".into());
        }
        if data.len() != size * size * size {
            return Err(format!(
                "Expected {} colours but found {}.",
                size * size * size,
                data.len()
            ));
        }
        // A non-standard domain is folded into 0..1 so sampling stays simple.
        if dmin != [0.0; 3] || dmax != [1.0; 3] {
            for px in &mut data {
                for i in 0..3 {
                    px[i] = (px[i] - dmin[i]) / (dmax[i] - dmin[i]).max(1e-6);
                }
            }
        }
        Ok(Self { size, data })
    }

    /// Reads a HaldCLUT picture: a square of side L cubed holding a table of size L squared.
    pub fn from_hald(w: usize, h: usize, px: &[[f32; 3]]) -> Result<Self, String> {
        let level = (1..=16).find(|l| l * l * l == w);
        let Some(level) = level.filter(|_| w == h && px.len() == w * h) else {
            return Err("This picture is not a HaldCLUT; its side must be a cube number, such as 512 or 1728.".into());
        };
        Ok(Self { size: level * level, data: px.to_vec() })
    }

    pub fn to_cube(&self, title: &str) -> String {
        let mut s = String::with_capacity(self.data.len() * 28);
        let _ = writeln!(s, "TITLE \"{title}\"");
        let _ = writeln!(s, "LUT_3D_SIZE {}", self.size);
        s.push_str("DOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1\n");
        for p in &self.data {
            let _ = writeln!(s, "{:.6} {:.6} {:.6}", p[0], p[1], p[2]);
        }
        s
    }

    /// Tetrahedral sample: reads four corners of a cell where trilinear reads eight, and
    /// follows the neutral axis more faithfully, which is why colour tools favour it.
    #[inline]
    pub fn sample(&self, c: [f32; 3]) -> [f32; 3] {
        let n = self.size;
        let top = (n - 1) as f32;
        let scaled = c.map(|v| v.clamp(0.0, 1.0) * top);
        // The cell's low corner, kept one short of the end so its far side always exists.
        let [x, y, z] = scaled.map(|v| (v as usize).min(n - 2));
        let (fx, fy, fz) = (scaled[0] - x as f32, scaled[1] - y as f32, scaled[2] - z as f32);
        let (dx, dy, dz) = (1, n, n * n);
        let at = x + y * dy + z * dz;
        // The cell splits into six tetrahedra; the order of the fractions says which.
        let (a, b, wa, wb, wc, wd) = if fx > fy {
            if fy > fz {
                (dx, dx + dy, 1.0 - fx, fx - fy, fy - fz, fz)
            } else if fx > fz {
                (dx, dx + dz, 1.0 - fx, fx - fz, fz - fy, fy)
            } else {
                (dz, dx + dz, 1.0 - fz, fz - fx, fx - fy, fy)
            }
        } else if fz > fy {
            (dz, dy + dz, 1.0 - fz, fz - fy, fy - fx, fx)
        } else if fz > fx {
            (dy, dy + dz, 1.0 - fy, fy - fz, fz - fx, fx)
        } else {
            (dy, dx + dy, 1.0 - fy, fy - fx, fx - fz, fz)
        };
        let d = &self.data;
        let (p0, p1, p2, p3) = (d[at], d[at + a], d[at + b], d[at + dx + dy + dz]);
        std::array::from_fn(|i| p0[i] * wa + p1[i] * wb + p2[i] * wc + p3[i] * wd)
    }
}

/// Every table the app knows, by id.
#[derive(Default, Clone)]
pub struct LutBank {
    pub luts: HashMap<String, Arc<Lut3D>>,
}

impl LutBank {
    pub fn get(&self, id: &str) -> Option<&Arc<Lut3D>> {
        self.luts.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_table_changes_nothing() {
        let lut = Lut3D::from_fn(17, |c| c);
        let c = lut.sample([0.123, 0.456, 0.789]);
        assert!((c[0] - 0.123).abs() < 1e-4 && (c[1] - 0.456).abs() < 1e-4 && (c[2] - 0.789).abs() < 1e-4);
    }

    #[test]
    fn corners_and_edges_of_the_table_are_exact() {
        let lut = Lut3D::from_fn(9, |c| [c[0] * 0.5, c[1] * c[1], 1.0 - c[2]]);
        assert_eq!(lut.sample([0.0; 3]), [0.0, 0.0, 1.0]);
        assert_eq!(lut.sample([1.0; 3]), [0.5, 1.0, 0.0]);
        assert_eq!(lut.sample([2.0, -1.0, 1.0]), [0.5, 0.0, 0.0]);
    }

    #[test]
    fn every_tetrahedron_blends_to_the_truth_on_a_straight_table() {
        let lut = Lut3D::from_fn(5, |c| [0.2 * c[0] + 0.3 * c[1] + 0.5 * c[2], c[1], c[2]]);
        for c in [[0.61, 0.33, 0.12], [0.61, 0.12, 0.33], [0.12, 0.61, 0.33], [0.33, 0.61, 0.12], [0.12, 0.33, 0.61], [0.33, 0.12, 0.61]] {
            let want = 0.2 * c[0] + 0.3 * c[1] + 0.5 * c[2];
            assert!((lut.sample(c)[0] - want).abs() < 1e-5, "{c:?}");
        }
    }

    #[test]
    fn cube_text_round_trips() {
        let lut = Lut3D::from_fn(5, |c| [c[0] * 0.5, c[1], 1.0 - c[2]]);
        let back = Lut3D::parse_cube(&lut.to_cube("test")).unwrap();
        assert_eq!(back.size, 5);
        for (a, b) in lut.data.iter().zip(back.data.iter()) {
            assert!((a[0] - b[0]).abs() < 1e-5 && (a[2] - b[2]).abs() < 1e-5);
        }
    }

    #[test]
    fn a_hald_picture_becomes_a_table() {
        let side = 64usize;
        let px: Vec<[f32; 3]> = Lut3D::from_fn(16, |c| c).data;
        let lut = Lut3D::from_hald(side, side, &px).unwrap();
        assert_eq!(lut.size, 16);
        assert!((lut.sample([0.2, 0.6, 0.4])[1] - 0.6).abs() < 0.01);
        assert!(Lut3D::from_hald(100, 100, &vec![[0.0; 3]; 10000]).is_err());
    }

    #[test]
    fn wrong_colour_count_is_refused() {
        assert!(Lut3D::parse_cube("LUT_3D_SIZE 2
0 0 0
").is_err());
    }

    /// Optional check against a real .cube file: set CUBE_PATH to run it.
    #[test]
    fn reads_a_real_file_when_given() {
        if let Ok(p) = std::env::var("CUBE_PATH") {
            let lut = Lut3D::parse_cube(&std::fs::read_to_string(p).unwrap()).unwrap();
            assert!(lut.size >= 2);
        }
    }
}
