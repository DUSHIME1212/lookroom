//! Timings of the app's hot paths: `--bench [CLIP]`. Run before and after a change, so
//! speed work is judged by numbers and not by what looks slow in the code.

use crate::looks;
use crate::pipeline::{self, Grade, Picture};
use crate::timeline::{Body, Fx, Index, Item, Kind, Media, Project, ROW_FX, ROW_FX2};
use crate::video;
use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

/// A big project, to show what scales badly: `clips` photo clips, each with a real grade.
pub fn stress(clips: usize, items: usize) -> Project {
    let mut p = Project { fps: 24.0, size: (1280, 720), ..Project::default() };
    let media = p.new_id();
    p.media.push(Media { id: media, path: None, name: "Stress".into(), kind: Kind::Photo, seconds: 0.0, info: None, has_audio: false, size: (1280, 720) });
    let look = looks::presets()[0].grade.clone();
    for i in 0..clips {
        let at = p.clips.len();
        p.insert_clip(media, at);
        let c = &mut p.clips[i];
        c.len = 48;
        c.grade = look.clone().into();
    }
    for i in 0..items {
        let id = p.new_id();
        let (row, body) = if i % 2 == 0 {
            (ROW_FX, Body::Effect { kind: Fx::BlackWhite, amount: 0.5 })
        } else {
            (ROW_FX2, Body::Text { text: format!("Title {i}"), size: 96.0, colour: [1.0; 3], x: 0.5, y: 0.5, motion: Default::default() })
        };
        p.items.push(Item { id, row, at: i as u32 * 12, len: 24, body });
    }
    p
}

fn timed(out: &mut String, label: &str, runs: u32, mut f: impl FnMut(u32)) {
    let began = Instant::now();
    for i in 0..runs {
        f(i);
    }
    let us = began.elapsed().as_secs_f64() * 1e6 / runs as f64;
    if us < 1000.0 {
        let _ = writeln!(out, "{label:<44} {us:>10.2} us");
    } else {
        let _ = writeln!(out, "{label:<44} {:>10.2} ms", us / 1000.0);
    }
}

pub fn run(clip: Option<&str>) -> String {
    let mut o = String::new();
    timed(&mut o, "build all colour tables (startup)", 1, |_| {
        std::hint::black_box(looks::build_bank());
    });
    let bank = looks::build_bank();

    let p = stress(500, 2000);
    let edited = {
        let mut q = p.clone();
        q.clips[250].grade.exposure = 0.5;
        q
    };
    let total = p.total();
    timed(&mut o, "copy the project (undo, every job)", 200, |_| {
        std::hint::black_box(p.clone());
    });
    timed(&mut o, "compare: nothing changed (every frame)", 200, |_| {
        std::hint::black_box(p == p.clone());
    });
    timed(&mut o, "compare: one clip changed", 200, |_| {
        std::hint::black_box(p == edited);
    });
    timed(&mut o, "build the timeline index", 200, |_| {
        std::hint::black_box(Index::of(&p));
    });
    let index = Index::of(&p);
    timed(&mut o, "which clip is at a frame", 2000, |i| {
        std::hint::black_box(index.shot(&p, i * 7 % total));
    });
    timed(&mut o, "which items are at a frame", 2000, |i| {
        let f = i * 7 % total;
        std::hint::black_box(index.items_in(&p.items, f, f + 1));
    });

    let (w, h) = (1280, 720);
    let px: Vec<[f32; 3]> = (0..w * h).map(|i| [(i % w) as f32 / w as f32, (i / w) as f32 / h as f32, 0.4]).collect();
    let pic = Picture { w, h, px };
    let g = looks::presets()[0].grade.clone();
    timed(&mut o, "bake one clip's colour table", 5, |_| {
        std::hint::black_box(pipeline::bake_lut(&g, &bank, 33));
    });
    let table = pipeline::bake_lut(&g, &bank, 33);
    timed(&mut o, "table lookup, 1 million pixels, one thread", 5, |_| {
        for c in pic.px.iter().take(1_000_000) {
            std::hint::black_box(table.sample(*c));
        }
    });
    timed(&mut o, "play: colour one 720p frame", 30, |f| {
        std::hint::black_box(pipeline::render_play(&pic, &Grade::default(), &table, &bank, f));
    });
    timed(&mut o, "pause: full render of one 720p frame", 5, |f| {
        std::hint::black_box(pipeline::render_at(&pic, &g, &bank, true, true, f));
    });
    timed(&mut o, "drag: table and render of the same frame", 5, |f| {
        let table = pipeline::bake_lut(&g, &bank, crate::film::PLAY_TABLE);
        std::hint::black_box(pipeline::render_play(&pic, &g, &table, &bank, f));
    });
    timed(&mut o, "turn a 720p frame into a screen image", 30, |_| {
        std::hint::black_box(crate::player::image_of(&pic));
    });

    if let Some(path) = clip
        && let Ok(info) = video::probe(Path::new(path))
    {
        let (w, h) = info.size_within(1280);
        let feed = video::Feed::new(info.clone(), w, h);
        let at = info.frames / 2;
        let wait = |f: u32| {
            feed.seek(f);
            while let Ok((got, _)) = feed.frames.recv_timeout(Duration::from_secs(10)) {
                if got == f {
                    break;
                }
            }
        };
        wait(at);
        timed(&mut o, "paused: step one frame forward", 10, |i| wait(at + i + 1));
        timed(&mut o, "paused: step one frame back", 10, |i| wait(at + 10 - i));
        timed(&mut o, "paused: jump back to a frame just seen", 10, |i| wait(at + (i % 3)));
        // Ground nothing has read yet, so these still have to start ffmpeg once each.
        let all = info.frames;
        timed(&mut o, "paused: first step back into unseen footage", 3, |i| wait(all / (4 << i)));
        timed(&mut o, "paused: jump ahead into unseen footage", 3, |i| wait(all - all / (4 << i)));
    }
    o
}
