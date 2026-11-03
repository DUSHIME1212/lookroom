//! Plugin packs. A pack is one JSON file that adds text animations and looks. Packs are
//! data only: nothing in one can run, so installing one is as safe as opening a project.
//! The format is in MECHANICS.md, and the pack that comes with the app is a working example.

use crate::looks::BuiltIn;
use crate::pipeline::Grade;
use crate::store;
use crate::textanim::{Anim, Ease, Order, Origin, Phase, Unit};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The group plugin looks show under among the presets.
pub const GROUP: &str = "Plugins";
/// Larger than this is not a pack anyone wrote by hand.
const LARGEST: u64 = 2 * 1024 * 1024;
const MOST_EACH: usize = 200;
const BUILT_IN: &str = include_str!("../plugins/lookroom-extras.json");

#[derive(Deserialize, Default)]
#[serde(default)]
struct PackFile {
    name: String,
    author: String,
    version: String,
    description: String,
    text_animations: Vec<AnimFile>,
    looks: Vec<LookFile>,
}

#[derive(Deserialize)]
#[serde(default)]
struct AnimFile {
    name: String,
    unit: Unit,
    order: Order,
    seconds: f32,
    delay: f32,
    ease: Ease,
    from: Origin,
}

impl Default for AnimFile {
    fn default() -> Self {
        Self { name: String::new(), unit: Unit::Letter, order: Order::Forward, seconds: 0.6, delay: 0.04, ease: Ease::Out, from: Origin::default() }
    }
}

#[derive(Deserialize)]
struct LookFile {
    name: String,
    #[serde(default)]
    grade: Grade,
}

/// A text animation from a pack, ready to put on a text.
#[derive(Clone, Debug)]
pub struct TextAnimation {
    pub name: String,
    pub pack: String,
    pub phase: Phase,
}

#[derive(Clone, Debug)]
pub struct Pack {
    pub name: String,
    pub author: String,
    pub version: String,
    pub description: String,
    /// None for the pack that comes with the app.
    pub file: Option<PathBuf>,
    pub animations: usize,
    pub looks: usize,
}

#[derive(Default)]
pub struct Plugins {
    pub packs: Vec<Pack>,
    pub animations: Vec<TextAnimation>,
    pub looks: Vec<BuiltIn>,
    /// Files that could not be used, and why.
    pub problems: Vec<String>,
}

/// Where packs are looked for: the person's own folder, then one beside the app.
pub fn dirs() -> Vec<PathBuf> {
    let mut v = vec![store::plugins_dir()];
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        v.push(dir.join("plugins"));
    }
    v
}

fn read(path: &Path) -> Result<String, String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > LARGEST {
        return Err("it is too large to be a plugin pack".into());
    }
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

fn parse(text: &str) -> Result<PackFile, String> {
    let p: PackFile = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if p.text_animations.is_empty() && p.looks.is_empty() {
        return Err("it adds no text animations or looks".into());
    }
    Ok(p)
}

pub fn load() -> Plugins {
    let mut out = Plugins::default();
    out.add(BUILT_IN, None);
    for dir in dirs() {
        let Ok(list) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<PathBuf> = list.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("json"))).collect();
        files.sort();
        for f in files {
            match read(&f) {
                Ok(text) => out.add(&text, Some(f)),
                Err(e) => out.problems.push(format!("{}: {e}", file_name(&f))),
            }
        }
    }
    out
}

fn file_name(p: &Path) -> String {
    p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().to_string())
}

impl Plugins {
    fn add(&mut self, text: &str, file: Option<PathBuf>) {
        let label = file.as_deref().map_or_else(|| "Built in".to_string(), file_name);
        let p = match parse(text) {
            Ok(p) => p,
            Err(e) => {
                self.problems.push(format!("{label}: {e}"));
                return;
            }
        };
        let fallback = file.as_deref().and_then(|f| f.file_stem()).map_or_else(|| "Plugin".to_string(), |s| s.to_string_lossy().to_string());
        let name = if p.name.trim().is_empty() { fallback } else { p.name.trim().to_string() };
        if self.packs.iter().any(|k| k.name == name) {
            self.problems.push(format!("{label}: a pack called {name} is already loaded"));
            return;
        }
        let animations: Vec<TextAnimation> = p
            .text_animations
            .into_iter()
            .filter(|a| !a.name.trim().is_empty())
            .take(MOST_EACH)
            .map(|a| TextAnimation {
                name: a.name.trim().to_string(),
                pack: name.clone(),
                phase: Phase {
                    kind: Anim::Custom,
                    unit: a.unit,
                    order: a.order,
                    seconds: if a.seconds.is_finite() { a.seconds.clamp(0.02, 10.0) } else { 0.6 },
                    delay: if a.delay.is_finite() { a.delay.clamp(0.0, 2.0) } else { 0.0 },
                    ease: a.ease,
                    from: a.from.tamed(),
                },
            })
            .collect();
        let looks: Vec<BuiltIn> = p
            .looks
            .into_iter()
            .filter(|l| !l.name.trim().is_empty())
            .take(MOST_EACH)
            .map(|l| BuiltIn { id: format!("plugin:{name}/{}", l.name.trim()), name: l.name.trim().to_string(), group: GROUP, grade: l.grade })
            .collect();
        self.packs.push(Pack {
            name,
            author: p.author,
            version: p.version,
            description: p.description,
            file,
            animations: animations.len(),
            looks: looks.len(),
        });
        self.animations.extend(animations);
        self.looks.extend(looks);
    }

    /// The pack animation a phase came from, if it is still loaded.
    pub fn animation_of(&self, phase: &Phase) -> Option<&TextAnimation> {
        (phase.kind == Anim::Custom).then(|| self.animations.iter().find(|a| a.phase.from == phase.from))?
    }
}

/// Checks a pack and copies it into the person's plugins folder. Gives its name.
pub fn install(path: &Path) -> Result<String, String> {
    let text = read(path)?;
    let p = parse(&text)?;
    let dir = store::plugins_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = path.file_name().ok_or("That is not a file.")?;
    std::fs::write(dir.join(name), text).map_err(|e| e.to_string())?;
    Ok(if p.name.trim().is_empty() { file_name(path) } else { p.name })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded(text: &str) -> Plugins {
        let mut p = Plugins::default();
        p.add(text, None);
        p
    }

    #[test]
    fn the_pack_that_comes_with_the_app_loads_cleanly() {
        let p = loaded(BUILT_IN);
        assert!(p.problems.is_empty(), "{:?}", p.problems);
        assert_eq!(p.packs[0].name, "Lookroom Extras");
        assert!(p.animations.len() >= 8 && p.looks.len() >= 3);
        assert!(p.animations.iter().all(|a| a.phase.kind == Anim::Custom));
        assert!(p.looks.iter().all(|l| l.group == GROUP && l.id.starts_with("plugin:")));
    }

    #[test]
    fn missing_fields_take_sensible_values() {
        let p = loaded(r#"{"name":"Mini","text_animations":[{"name":"Up","from":{"dy":1}}]}"#);
        let a = &p.animations[0].phase;
        assert_eq!((a.unit, a.ease, a.from.opacity, a.from.scale), (Unit::Letter, Ease::Out, 0.0, 1.0));
    }

    #[test]
    fn broken_or_empty_packs_are_reported_not_loaded() {
        let p = loaded("{ not json");
        assert!(p.packs.is_empty() && p.problems.len() == 1);
        let q = loaded(r#"{"name":"Nothing"}"#);
        assert!(q.packs.is_empty() && q.problems[0].contains("adds no"));
    }

    #[test]
    fn wild_numbers_in_a_pack_are_tamed() {
        let p = loaded(r#"{"name":"Wild","text_animations":[{"name":"X","seconds":1e9,"delay":-4,"from":{"scale":99}}]}"#);
        let a = &p.animations[0].phase;
        assert_eq!((a.seconds, a.delay, a.from.scale), (10.0, 0.0, 8.0));
    }

    #[test]
    fn a_pack_cant_be_loaded_twice() {
        let mut p = loaded(BUILT_IN);
        p.add(BUILT_IN, None);
        assert_eq!(p.packs.len(), 1);
        assert_eq!(p.problems.len(), 1);
    }

    #[test]
    fn an_animation_put_on_a_text_still_knows_its_name() {
        let p = loaded(BUILT_IN);
        let phase = p.animations[2].phase;
        assert_eq!(p.animation_of(&phase).map(|a| a.name.as_str()), Some(p.animations[2].name.as_str()));
        assert!(p.animation_of(&Phase::preset(Anim::Fade)).is_none());
    }

    #[test]
    fn unknown_fields_are_ignored_so_newer_packs_still_load() {
        let p = loaded(r#"{"name":"Newer","shaders":[1,2],"looks":[{"name":"L","grade":{"exposure":0.5,"future_slider":3}}]}"#);
        assert!(p.problems.is_empty(), "{:?}", p.problems);
        assert_eq!(p.looks[0].grade.exposure, 0.5);
    }
}
