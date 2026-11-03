//! Icons from the Hugeicons free set, drawn to a texture at the size they are shown,
//! so they stay crisp at any window scale. The bottom row's tools live here too.

pub use crate::icon_set::Icon;
use eframe::egui::epaint::{Mesh, Vertex, WHITE_UV};
use eframe::egui::{Color32, ColorImage, Context, Painter, Pos2, Rect, Shape, TextureHandle, TextureOptions, pos2};
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Filters,
    Film,
    Effects,
    Masks,
    Presets,
    Crop,
    Luts,
}

pub const TOOLS: [Tool; 7] = [
    Tool::Filters,
    Tool::Film,
    Tool::Effects,
    Tool::Masks,
    Tool::Presets,
    Tool::Crop,
    Tool::Luts,
];

impl Tool {
    pub fn tip(self) -> &'static str {
        match self {
            Tool::Filters => "Color",
            Tool::Film => "Film",
            Tool::Effects => "Effects",
            Tool::Masks => "Masks",
            Tool::Presets => "Presets",
            Tool::Crop => "Crop",
            Tool::Luts => "LUT Library",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Tool::Filters => Icon::SlidersHorizontal,
            Tool::Film => Icon::FilmRoll01,
            Tool::Effects => Icon::Prism,
            Tool::Masks => Icon::Layers01,
            Tool::Presets => Icon::MaskTheater01,
            Tool::Crop => Icon::Crop,
            Tool::Luts => Icon::Cube,
        }
    }

    /// Film, prism and cube carry colour in the tool row, left to right.
    pub fn colours(self) -> Option<&'static [Color32]> {
        const FILM: [Color32; 4] = [
            Color32::from_rgb(255, 92, 96),
            Color32::from_rgb(255, 176, 64),
            Color32::from_rgb(106, 214, 130),
            Color32::from_rgb(92, 150, 255),
        ];
        const PRISM: [Color32; 3] = [
            Color32::from_rgb(96, 170, 255),
            Color32::from_rgb(120, 110, 255),
            Color32::from_rgb(255, 120, 150),
        ];
        const CUBE: [Color32; 3] = [
            Color32::from_rgb(120, 225, 150),
            Color32::from_rgb(250, 175, 70),
            Color32::from_rgb(235, 90, 190),
        ];
        match self {
            Tool::Film => Some(&FILM),
            Tool::Effects => Some(&PRISM),
            Tool::Luts => Some(&CUBE),
            _ => None,
        }
    }
}

thread_local! {
    static CACHE: RefCell<HashMap<(Icon, u32), TextureHandle>> = RefCell::new(HashMap::new());
}

/// White on clear, ready to be tinted. Colours are premultiplied.
pub fn rasterize(icon: Icon, px: u32) -> Option<ColorImage> {
    let svg = icon.svg().replace("currentColor", "#fff");
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).ok()?;
    let mut map = resvg::tiny_skia::Pixmap::new(px, px)?;
    let s = px as f32 / tree.size().width();
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(s, s), &mut map.as_mut());
    Some(ColorImage::from_rgba_premultiplied([px as usize, px as usize], map.data()))
}

fn texture(ctx: &Context, icon: Icon, px: u32) -> Option<TextureHandle> {
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(t) = cache.get(&(icon, px)) {
            return Some(t.clone());
        }
        let img = rasterize(icon, px)?;
        let t = ctx.load_texture(format!("icon-{icon:?}-{px}"), img, TextureOptions::LINEAR);
        cache.insert((icon, px), t.clone());
        Some(t)
    })
}

fn side_px(ctx: &Context, r: Rect) -> u32 {
    (r.width() * ctx.pixels_per_point()).round().max(4.0) as u32
}

pub fn paint(p: &Painter, icon: Icon, r: Rect, tint: Color32) {
    let Some(t) = texture(p.ctx(), icon, side_px(p.ctx(), r)) else { return };
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    p.image(t.id(), r, uv, tint);
}

/// The same icon with its colour running across it.
pub fn paint_gradient(p: &Painter, icon: Icon, r: Rect, colours: &[Color32]) {
    let Some(t) = texture(p.ctx(), icon, side_px(p.ctx(), r)) else { return };
    let mut m = Mesh::with_texture(t.id());
    let n = colours.len().max(2) - 1;
    for i in 0..=n {
        let f = i as f32 / n as f32;
        let x = r.left() + r.width() * f;
        let c = colours[i.min(colours.len() - 1)];
        let at = |pos: Pos2, v: f32| Vertex { pos, uv: pos2(f, v), color: c };
        m.vertices.push(at(pos2(x, r.top()), 0.0));
        m.vertices.push(at(pos2(x, r.bottom()), 1.0));
        if i > 0 {
            let b = (i as u32 - 1) * 2;
            m.add_triangle(b, b + 1, b + 2);
            m.add_triangle(b + 1, b + 2, b + 3);
        }
    }
    p.add(Shape::mesh(m));
}

/// A filled band of colours, for hue bars and slider tracks.
pub fn colour_band(p: &Painter, r: Rect, colours: &[Color32]) {
    let mut m = Mesh::default();
    let n = colours.len().max(2) - 1;
    for i in 0..=n {
        let x = r.left() + r.width() * i as f32 / n as f32;
        let c = colours[i.min(colours.len() - 1)];
        m.vertices.push(Vertex { pos: pos2(x, r.top()), uv: WHITE_UV, color: c });
        m.vertices.push(Vertex { pos: pos2(x, r.bottom()), uv: WHITE_UV, color: c });
        if i > 0 {
            let b = (i as u32 - 1) * 2;
            m.add_triangle(b, b + 1, b + 2);
            m.add_triangle(b + 1, b + 2, b + 3);
        }
    }
    p.add(Shape::mesh(m));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_icon_draws_something() {
        for t in TOOLS {
            let img = rasterize(t.icon(), 32).expect("icon parses");
            let inked = img.pixels.iter().filter(|c| c.a() > 40).count();
            assert!(inked > 20, "{t:?} is nearly empty");
        }
    }

    #[test]
    fn icons_are_redrawn_for_the_size_they_are_shown() {
        let small = rasterize(Icon::Crop, 16).unwrap();
        let big = rasterize(Icon::Crop, 48).unwrap();
        assert_eq!(small.size, [16, 16]);
        assert_eq!(big.size, [48, 48]);
    }
}
