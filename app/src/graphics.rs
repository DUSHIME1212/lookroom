//! Designs brought in from Figma, or anything else that saves SVG. Each top-level layer
//! becomes an item of its own, drawn alone, so it can be moved and animated by itself the
//! way Jitter treats a Figma frame. Designs are kept in %APPDATA%\Lookroom\Graphics.

use crate::overlay::{Stamp, fonts, stamp_of};
use crate::store;
use resvg::{tiny_skia, usvg};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

/// Larger than this is not a design anyone pasted.
const LARGEST: usize = 32 * 1024 * 1024;
/// No layer is drawn wider or taller than this.
const MOST_PX: f32 = 8192.0;

pub struct Layer {
    pub name: String,
    /// Left, top, width and height, in the design's own units.
    pub bounds: [f32; 4],
}

pub struct Design {
    pub size: (f32, f32),
    pub layers: Vec<Layer>,
    tree: usvg::Tree,
    /// Child positions from the root down to the group holding the layers.
    within: Vec<usize>,
}

fn layer_group(tree: &usvg::Tree) -> (&usvg::Group, Vec<usize>) {
    // A Figma frame often arrives as one plain group around everything. Its children are
    // the layers. A group that moves its children is kept as one layer, so nothing shifts.
    let mut g = tree.root();
    let mut path = Vec::new();
    while let [usvg::Node::Group(only)] = g.children() {
        if !only.transform().is_identity() {
            break;
        }
        g = only;
        path.push(0);
    }
    (g, path)
}

fn group_at<'a>(tree: &'a usvg::Tree, path: &[usize]) -> Option<&'a usvg::Group> {
    let mut g = tree.root();
    for i in path {
        match g.children().get(*i)? {
            usvg::Node::Group(inner) => g = inner,
            _ => return None,
        }
    }
    Some(g)
}

/// A friendlier name than an id like "Frame_12": spaces back, and a number when there's none.
fn name_of(id: &str, n: usize) -> String {
    let named = id.replace('_', " ").trim().to_string();
    if named.is_empty() { format!("Layer {}", n + 1) } else { named }
}

pub fn parse(text: &str) -> Result<Design, String> {
    if text.len() > LARGEST {
        return Err("That design is too large.".into());
    }
    let tree = usvg::Tree::from_str(text, &fonts()).map_err(|e| format!("That isn't an SVG the app can read: {e}"))?;
    let (group, within) = layer_group(&tree);
    let layers: Vec<Layer> = group
        .children()
        .iter()
        .enumerate()
        .filter_map(|(n, node)| {
            let b = node.abs_layer_bounding_box()?;
            Some(Layer { name: name_of(node.id(), n), bounds: [b.x(), b.y(), b.width(), b.height()] })
        })
        .collect();
    if layers.is_empty() {
        return Err("That design has nothing to draw.".into());
    }
    let size = (tree.size().width(), tree.size().height());
    Ok(Design { size, layers, tree, within })
}

pub fn dir() -> PathBuf {
    store::data_dir().join("Graphics")
}

/// A name from the text itself, so pasting the same design twice keeps one file.
fn file_name(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}.svg")
}

/// Checks a design, keeps a copy, and gives where it is with what's in it.
pub fn import(text: &str) -> Result<(String, Arc<Design>), String> {
    let design = Arc::new(parse(text)?);
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let path = dir().join(file_name(text));
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    let file = path.to_string_lossy().to_string();
    cache().lock().unwrap_or_else(|e| e.into_inner()).insert(file.clone(), design.clone());
    Ok((file, design))
}

fn cache() -> &'static Mutex<HashMap<String, Arc<Design>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Design>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// A kept design, read once and then shared.
pub fn load(file: &str) -> Option<Arc<Design>> {
    if let Some(d) = cache().lock().unwrap_or_else(|e| e.into_inner()).get(file) {
        return Some(d.clone());
    }
    let d = Arc::new(parse(&std::fs::read_to_string(file).ok()?).ok()?);
    cache().lock().unwrap_or_else(|e| e.into_inner()).insert(file.to_string(), d.clone());
    Some(d)
}

impl Design {
    /// One layer by itself, \`scale\` pixels to each of the design's units.
    pub fn draw(&self, layer: usize, scale: f32) -> Option<Stamp> {
        let node = group_at(&self.tree, &self.within)?.children().get(layer)?;
        let b = node.abs_layer_bounding_box()?;
        let w = (b.width() * scale).ceil().clamp(1.0, MOST_PX) as u32;
        let h = (b.height() * scale).ceil().clamp(1.0, MOST_PX) as u32;
        let mut map = tiny_skia::Pixmap::new(w, h)?;
        resvg::render_node(node, tiny_skia::Transform::from_scale(scale, scale), &mut map.as_mut())?;
        Some(stamp_of(&map))
    }

    /// The scale that fits the whole design inside a picture.
    pub fn fit(&self, w: f32, h: f32) -> f32 {
        (w / self.size.0.max(1.0)).min(h / self.size.1.max(1.0))
    }

    /// Where a layer's centre sits on a picture of this shape, from 0 to 1 each way.
    pub fn place(&self, layer: usize, w: f32, h: f32) -> (f32, f32) {
        let [x, y, lw, lh] = self.layers[layer].bounds;
        let s = self.fit(w, h);
        (0.5 + (x + lw / 2.0 - self.size.0 / 2.0) * s / w, 0.5 + (y + lh / 2.0 - self.size.1 / 2.0) * s / h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shaped like Figma's export: a frame group around named layers.
    const FRAME: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="200" viewBox="0 0 400 200">
        <g id="Frame_1">
            <rect id="Background" width="400" height="200" fill="#202030"/>
            <circle id="Sun" cx="300" cy="60" r="40" fill="#ffcc00"/>
            <rect x="20" y="140" width="100" height="40" fill="#ff3366"/>
        </g></svg>"##;

    #[test]
    fn a_frame_opens_into_its_layers_with_their_names() {
        let d = parse(FRAME).unwrap();
        let names: Vec<&str> = d.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Background", "Sun", "Layer 3"]);
        assert_eq!(d.size, (400.0, 200.0));
        assert_eq!(d.layers[1].bounds, [260.0, 20.0, 80.0, 80.0]);
    }

    #[test]
    fn a_layer_draws_alone_at_its_own_size() {
        let d = parse(FRAME).unwrap();
        let sun = d.draw(1, 2.0).unwrap();
        assert_eq!((sun.w, sun.h), (160, 160));
        let middle = sun.px[80 + 80 * 160];
        assert!(middle[3] > 0.99 && middle[0] > 0.9 && middle[2] < 0.1, "{middle:?}");
        assert!(sun.px[0][3] < 0.01, "a corner outside the circle is clear");
    }

    #[test]
    fn layers_land_where_the_design_had_them() {
        let d = parse(FRAME).unwrap();
        let (x, y) = d.place(1, 1920.0, 1080.0);
        // The design fills the width; the sun sits right of centre, near the top.
        assert!((x - 0.75).abs() < 1e-4 && (y - (0.5 - 40.0 * 4.8 / 1080.0)).abs() < 1e-4, "{x} {y}");
    }

    #[test]
    fn broken_or_empty_designs_are_refused() {
        assert!(parse("not svg").is_err());
        assert!(parse(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"></svg>"#).is_err());
    }

    #[test]
    fn the_same_design_gets_the_same_file() {
        assert_eq!(file_name(FRAME), file_name(FRAME));
        assert_ne!(file_name(FRAME), file_name("<svg/>"));
    }
}
