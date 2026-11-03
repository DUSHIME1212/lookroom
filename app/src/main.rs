#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assets;
mod audio;
mod captions;
mod clips;
mod bench;
mod color;
mod crop;
mod curves;
mod effects;
mod export;
mod film;
mod graphics;
mod heic;
mod icon_set;
mod icons;
mod looks;
mod lut;
mod masks;
mod memes;
mod overlay;
mod pipeline;
mod player;
mod platform;
mod plugins;
mod scene;
mod scopes;
mod shared;
mod sound_set;
mod store;
mod textanim;
mod tiff;
mod timeline;
mod tools;
mod track;
mod ui;
mod video;

use pipeline::Grade;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32};

fn load_grade(path: &str) -> Result<Grade, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))
}

/// Grades a file without opening a window: `--render IN OUT [--grade FILE] [--codec NAME]`.
fn render_file(input: &str, out: &str, grade: Grade, codec: &str, deep: bool) -> Result<(), String> {
    let bank = looks::build_bank();
    let (input, out) = (Path::new(input), Path::new(out));
    if video::is_video(input) {
        let source = video::probe(input)?;
        let last = source.frames.saturating_sub(1);
        let grade = source.suited(&grade);
        let codec = match codec {
            "h264" => video::Codec::H264,
            "prores" => video::Codec::ProRes,
            _ => video::Codec::Hevc,
        };
        let job = video::Export { source, out: out.to_path_buf(), codec, side: video::MAX_SIDE, ten_bit: true, first: 0, last };
        video::export(&job, &grade, &bank, &AtomicU32::new(0), &AtomicBool::new(false))
    } else {
        let o = export::open_photo(input)?;
        let format = out.extension().and_then(|e| e.to_str()).map(str::to_lowercase);
        let format = match format.as_deref() {
            Some("heic") => export::Format::Heic,
            Some("png") => export::Format::Png,
            Some("tif" | "tiff") => export::Format::Tiff,
            _ => export::Format::Jpeg,
        };
        let opts = export::PhotoOptions { format, deep, ..export::PhotoOptions::default() };
        export::export_photo(out, &o.picture, &grade, &bank, &opts, o.exif.as_deref())
    }
}

/// Every built-in preset side by side, on the sample scene with a grey ramp and three
/// skin tones beneath it: `--contact OUT`.
fn contact_sheet(out: &str) -> Result<(), String> {
    let (cell_w, scene_h, strip_h, cols) = (320usize, 170usize, 40usize, 5usize);
    let cell_h = scene_h + strip_h;
    let skin: [[f32; 3]; 3] = [[0.93, 0.76, 0.66], [0.76, 0.55, 0.42], [0.42, 0.28, 0.2]];
    let scene = scene::sample_picture(1800, 1200).cover(cell_w, scene_h);
    let mut px = scene.px.clone();
    for _ in 0..strip_h {
        for x in 0..cell_w {
            px.push(if x < cell_w / 2 { [x as f32 / (cell_w / 2 - 1) as f32; 3] } else { skin[(x - cell_w / 2) * 3 / (cell_w - cell_w / 2)] });
        }
    }
    let base = pipeline::Picture { w: cell_w, h: cell_h, px };
    let bank = looks::build_bank();
    let list = looks::presets();
    let rows = list.len().div_ceil(cols);
    let (sw, sh) = (cell_w * cols, cell_h * rows);
    let mut sheet = vec![0u8; sw * sh * 3];
    for (i, p) in list.iter().enumerate() {
        let shot = pipeline::render(&base, &p.grade, &bank, true).to_rgb8();
        let (ox, oy) = (i % cols * cell_w, i / cols * cell_h);
        for y in 0..cell_h {
            let from = y * cell_w * 3;
            let to = ((oy + y) * sw + ox) * 3;
            sheet[to..to + cell_w * 3].copy_from_slice(&shot[from..from + cell_w * 3]);
        }
    }
    image::save_buffer(out, &sheet, sw as u32, sh as u32, image::ExtendedColorType::Rgb8).map_err(|e| e.to_string())
}

/// The app's logo at `side` pixels square, unpremultiplied, for windows and packages.
fn app_icon(side: u32) -> Option<(Vec<u8>, u32, u32)> {
    let logo = image::load_from_memory(include_bytes!("../assets/icon.png")).ok()?.to_rgba8();
    let sized = image::imageops::resize(&logo, side, side, image::imageops::FilterType::Lanczos3);
    Some((sized.into_raw(), side, side))
}

/// Every text animation as a filmstrip, one per row, drawn the way playback and export
/// draw it: `--text-sheet OUT`.
fn text_sheet(out: &str) -> Result<(), String> {
    use textanim::{Anim, Ease, Keyed, Motion, Phase};
    use timeline::{Body, Index, Item, Project, ROW_FX2};
    let (cell_w, cell_h, len) = (300usize, 84usize, 60u32);
    const TIMES: [u32; 7] = [0, 2, 5, 9, 14, 22, 40];
    let mut rows: Vec<Motion> = Anim::ALL.iter().skip(1).map(|a| Motion { enter: Phase::preset(*a), ..Motion::default() }).collect();
    let mut keyed = Motion { font: "Georgia".into(), italic: true, ..Motion::default() };
    let at = |x: f32, scale: f32, rotate: f32, opacity: f32| Keyed { x, y: 0.5, scale, rotate, opacity };
    keyed.set_key(0, at(0.2, 0.5, -20.0, 0.0));
    keyed.set_key(30, at(0.8, 1.3, 20.0, 1.0));
    keyed.keys[0].ease = Ease::Back;
    rows.push(keyed);
    // Then every animation the plugin packs add.
    rows.extend(plugins::load().animations.into_iter().map(|a| Motion { enter: a.phase, ..Motion::default() }));
    let (sw, sh) = (cell_w * TIMES.len(), cell_h * rows.len());
    let mut sheet = vec![0u8; sw * sh * 3];
    for (r, motion) in rows.iter().enumerate() {
        let mut p = Project::default();
        let id = p.new_id();
        p.items.push(Item { id, row: ROW_FX2, at: 0, len, body: Body::Text { text: "Moving type".into(), size: 300.0, colour: [1.0; 3], x: 0.5, y: 0.5, motion: motion.clone() } });
        let index = Index::of(&p);
        let mut laid = player::Laid::default();
        for (c, f) in TIMES.iter().enumerate() {
            let mut pic = pipeline::Picture { w: cell_w, h: cell_h, px: vec![[0.09, 0.1, 0.13]; cell_w * cell_h] };
            laid.apply(&mut pic, &p, &index, *f, &Default::default());
            let shot = pic.to_rgb8();
            let (ox, oy) = (c * cell_w, r * cell_h);
            for y in 0..cell_h - 1 {
                let (from, to) = (y * cell_w * 3, ((oy + y) * sw + ox) * 3);
                sheet[to..to + (cell_w - 1) * 3].copy_from_slice(&shot[from..from + (cell_w - 1) * 3]);
            }
        }
    }
    image::save_buffer(out, &sheet, sw as u32, sh as u32, image::ExtendedColorType::Rgb8).map_err(|e| e.to_string())
}

/// Builds a project from files and exports it, for testing the whole path without a
/// window: `--edit-test OUT FILES...`. Sound files go on the music track.
fn edit_test(out: &str, files: &[String]) -> Result<(), String> {
    use timeline::{Body, Fx, Item, Kind, Media, Project, ROW_FX, ROW_FX2, ROW_MUSIC, Wipe};
    let mut p = Project::default();
    for f in files {
        let path = Path::new(f);
        let id = p.new_id();
        let name = f.clone();
        if video::is_sound(path) {
            let seconds = video::sound_seconds(path)?;
            p.media.push(Media { id, path: Some(path.into()), name: name.clone(), kind: Kind::Audio, seconds, info: None, has_audio: true, size: (0, 0) });
            let (at, len, item) = (0, p.frames(seconds), p.new_id());
            p.items.push(Item { id: item, row: ROW_MUSIC, at, len, body: Body::Audio { media: id, start: 0.0, volume: 0.5, name, speed: 1.0 } });
        } else {
            let i = video::probe(path)?;
            p.media.push(Media { id, path: Some(path.into()), name, kind: Kind::Video, seconds: i.seconds(i.frames), has_audio: i.has_audio, size: (i.w, i.h), info: Some(i) });
            p.insert_clip(id, p.clips.len());
            p.suit_first(id);
            let last = p.clips.len() - 1;
            p.suit_clip(last);
        }
    }
    p.set_transition(1, Wipe::Dissolve, p.frames(1.0));
    let (a, b, c) = (p.new_id(), p.new_id(), p.frames(1.5));
    p.items.push(Item { id: a, row: ROW_FX, at: 0, len: c, body: Body::Effect { kind: Fx::BlackWhite, amount: 1.0 } });
    p.items.push(Item { id: b, row: ROW_FX2, at: c, len: c, body: Body::Text { text: "Lookroom".into(), size: 120.0, colour: [1.0; 3], x: 0.5, y: 0.5, motion: Default::default() } });
    let scene = std::sync::Arc::new(player::Scene { project: p, bank: looks::build_bank(), stills: Default::default() });
    let opts = export::VideoOptions { side: 1280, ..Default::default() };
    player::export(scene, Path::new(out), &opts, &AtomicU32::new(0), &AtomicBool::new(false))
}

/// Tracks a square around a point: `--track CLIP X Y SIZE`, printing the frame-by-frame move.
fn track_file(path: &str, x: f32, y: f32, size: f32, back: bool) -> Result<(), String> {
    let info = video::probe(Path::new(path))?;
    let mask = masks::Mask { x, y, size_x: size, size_y: size, ..masks::Mask::new(masks::Shape::Rectangle) };
    let end = info.frames.saturating_sub(1);
    let (from, to) = if back { (end, 0) } else { (0, end) };
    let run = track::start(info.clone(), mask, from, to);
    for u in run.updates.iter() {
        match u {
            track::Update::Key(k) => println!("{} {:.4} {:.4} {:.4} {:.2}", k.frame, k.x, k.y, k.size_x, k.angle),
            track::Update::Finished(r) => return r,
        }
    }
    Ok(())
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut start = ui::Start::default();
    let mut size = [1360.0f32, 840.0];
    let mut render = None;
    let mut codec = String::new();
    let mut grade_file = None;
    let mut deep = false;
    let mut i = 1;
    let next = |i: &mut usize| {
        *i += 1;
        args.get(*i).cloned()
    };
    while i < args.len() {
        match args[i].as_str() {
            "--screenshot" => start.screenshot = next(&mut i),
            "--tool" => start.tool = next(&mut i),
            "--input" => start.input = next(&mut i),
            "--grade" => grade_file = next(&mut i),
            "--scope" => start.scope = next(&mut i),
            "--sheet" => start.sheet = next(&mut i),
            "--frame" => start.frame = next(&mut i).and_then(|v| v.parse().ok()),
            "--compare" => start.compare = true,
            "--deep" => deep = true,
            "--dump" => start.dump = next(&mut i),
            "--play-test" => start.play_test = next(&mut i),
            "--ui-bench" => start.ui_bench = next(&mut i),
            "--icon" => {
                // The app's icon as a PNG, for the macOS and Linux packages.
                let (out, side) = (next(&mut i).unwrap_or_else(|| "icon.png".into()), next(&mut i).and_then(|s| s.parse().ok()).unwrap_or(512));
                let saved = app_icon(side).ok_or("The icon could not be drawn.".to_string()).and_then(|(px, w, h)| image::save_buffer(&out, &px, w, h, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string()));
                if let Err(e) = saved {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
                return Ok(());
            }
            "--bench" => {
                let clip = next(&mut i);
                print!("{}", bench::run(clip.as_deref()));
                return Ok(());
            }
            "--codec" => codec = next(&mut i).unwrap_or_default(),
            "--size" => {
                if let Some((w, h)) = next(&mut i).as_deref().and_then(|s| s.split_once('x')) {
                    size = [w.parse().unwrap_or(size[0]), h.parse().unwrap_or(size[1])];
                }
            }
            "--render" => render = next(&mut i).zip(next(&mut i)),
            "--edit-test" => {
                let Some(out) = next(&mut i) else { std::process::exit(2) };
                if let Err(e) = edit_test(&out, &args[i + 1..]) {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
                return Ok(());
            }
            "--text-sheet" => {
                if let Err(e) = text_sheet(&next(&mut i).unwrap_or_default()) {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
                return Ok(());
            }
            "--contact" => {
                if let Err(e) = contact_sheet(&next(&mut i).unwrap_or_default()) {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
                return Ok(());
            }
            "--track" | "--track-back" => {
                let back = args[i] == "--track-back";
                let spec = (next(&mut i), next(&mut i), next(&mut i), next(&mut i));
                let (Some(p), Some(x), Some(y), Some(s)) = spec else { std::process::exit(2) };
                let n = |v: &str| v.parse().unwrap_or(0.5);
                if let Err(e) = track_file(&p, n(&x), n(&y), n(&s), back) {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
                return Ok(());
            }
            other => start.open.push(other.to_string()),
        }
        i += 1;
    }
    if let Some(f) = &grade_file {
        match load_grade(f) {
            Ok(g) => start.grade = Some(g),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }
    if let Some((input, out)) = render {
        if let Err(e) = render_file(&input, &out, start.grade.unwrap_or_default(), &codec, deep) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return Ok(());
    }

    let mut viewport = eframe::egui::ViewportBuilder::default().with_title("Lookroom").with_app_id("lookroom").with_inner_size(size).with_min_inner_size([380.0, 640.0]);
    if let Some((rgba, width, height)) = app_icon(256) {
        viewport = viewport.with_icon(eframe::egui::IconData { rgba, width, height });
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("Lookroom", options, Box::new(move |cc| Ok(Box::new(ui::App::new(cc, start)))))
}
