//! Reading photos in, and writing them back out with their metadata.

use crate::heic;
use crate::lut::LutBank;
use crate::tiff;
use crate::pipeline::{self, Grade, Picture};
use crate::video;
use image::metadata::Orientation;
use image::{DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageReader};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

pub const MAX_BATCH: usize = 10;
pub const PHOTO_EXTENSIONS: [&str; 8] = ["jpg", "jpeg", "png", "tif", "tiff", "webp", "heic", "avif"];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Format {
    Heic,
    #[default]
    Jpeg,
    Png,
    Tiff,
}

impl Format {
    pub const ALL: [Format; 4] = [Format::Heic, Format::Jpeg, Format::Png, Format::Tiff];

    pub fn name(self) -> &'static str {
        match self {
            Format::Heic => "HEIC",
            Format::Jpeg => "JPEG",
            Format::Png => "PNG",
            Format::Tiff => "TIFF",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Heic => "heic",
            Format::Jpeg => "jpg",
            Format::Png => "png",
            Format::Tiff => "tiff",
        }
    }

    /// JPEG is 8-bit by definition; HEIC's deep option is 10-bit.
    pub fn deep_label(self) -> Option<&'static str> {
        match self {
            Format::Jpeg => None,
            Format::Heic => Some("10-bit"),
            _ => Some("16-bit"),
        }
    }

    pub fn has_quality(self) -> bool {
        matches!(self, Format::Heic | Format::Jpeg)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhotoOptions {
    pub format: Format,
    pub deep: bool,
    pub quality: u8,
    pub metadata: bool,
}

impl Default for PhotoOptions {
    fn default() -> Self {
        Self { format: Format::Jpeg, deep: false, quality: 92, metadata: true }
    }
}

pub struct Opened {
    pub picture: Picture,
    pub exif: Option<Vec<u8>>,
}

/// Opens a photo at full depth, upright. Formats the image library can't read,
/// such as HEIC, go through ffmpeg.
pub fn open_photo(path: &Path) -> Result<Opened, String> {
    match open_direct(path) {
        Ok(o) => Ok(o),
        Err(first) => {
            let Ok(picture) = video::still(path) else { return Err(first) };
            let mut exif = std::fs::read(path).ok().and_then(|f| heic::read_exif(&f));
            if let Some(e) = exif.as_mut() {
                let _ = Orientation::remove_from_exif_chunk(e);
            }
            Ok(Opened { picture, exif })
        }
    }
}

fn open_direct(path: &Path) -> Result<Opened, String> {
    let reader = ImageReader::open(path).map_err(|e| e.to_string())?.with_guessed_format().map_err(|e| e.to_string())?;
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut exif = decoder.exif_metadata().ok().flatten();
    // The image library doesn't read tags out of TIFF files.
    if exif.is_none() && path.extension().is_some_and(|x| x.eq_ignore_ascii_case("tif") || x.eq_ignore_ascii_case("tiff")) {
        exif = std::fs::read(path).ok().and_then(|f| tiff::read_exif(&f));
    }
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    img.apply_orientation(orientation);
    // The pixels are upright now, so a saved tag would turn them a second time.
    if let Some(e) = exif.as_mut() {
        let _ = Orientation::remove_from_exif_chunk(e);
    }
    let rgb = img.into_rgb32f();
    let (w, h) = (rgb.width() as usize, rgb.height() as usize);
    Ok(Opened { picture: Picture::from_rgb32f(w, h, rgb.as_raw()), exif })
}

pub fn save_photo(path: &Path, pic: &Picture, o: &PhotoOptions, exif: Option<&[u8]>) -> Result<(), String> {
    let exif = exif.filter(|_| o.metadata);
    let (w, h) = (pic.w as u32, pic.h as u32);
    let file = || File::create(path).map(BufWriter::new).map_err(|e| e.to_string());
    let deep = o.deep && o.format.deep_label().is_some();
    let words = || -> Vec<u8> { pic.to_rgb16().iter().flat_map(|v| v.to_ne_bytes()).collect() };
    match o.format {
        Format::Heic => {
            let bytes = heic::encode(pic, o.quality, deep, exif)?;
            std::fs::write(path, bytes).map_err(|e| e.to_string())
        }
        Format::Jpeg => {
            let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(file()?, o.quality.clamp(1, 100));
            if let Some(e) = exif {
                let _ = enc.set_exif_metadata(e.to_vec());
            }
            enc.write_image(&pic.to_rgb8(), w, h, ExtendedColorType::Rgb8).map_err(|e| e.to_string())
        }
        Format::Png => {
            let mut enc = image::codecs::png::PngEncoder::new(file()?);
            if let Some(e) = exif {
                let _ = enc.set_exif_metadata(e.to_vec());
            }
            if deep {
                enc.write_image(&words(), w, h, ExtendedColorType::Rgb16)
            } else {
                enc.write_image(&pic.to_rgb8(), w, h, ExtendedColorType::Rgb8)
            }
            .map_err(|e| e.to_string())
        }
        Format::Tiff => std::fs::write(path, tiff::write(pic, deep, exif)?).map_err(|e| e.to_string()),
    }
}

/// The whole pipeline at full size, then the file.
pub fn export_photo(path: &Path, source: &Picture, g: &Grade, bank: &LutBank, o: &PhotoOptions, exif: Option<&[u8]>) -> Result<(), String> {
    let mut out = pipeline::render(source, g, bank, true);
    pipeline::apply_frame(&mut out, g.frame_ratio);
    save_photo(path, &out, o, exif)
}

/// What a batch does to video files.
#[derive(Clone, Copy, Debug)]
pub struct VideoOptions {
    pub codec: video::Codec,
    pub mov: bool,
    pub side: usize,
    pub ten_bit: bool,
    /// Quicker encoding, on the graphics chip when there is one, for a somewhat larger file.
    pub fast: bool,
}

impl Default for VideoOptions {
    fn default() -> Self {
        Self { codec: video::Codec::Hevc, mov: false, side: video::MAX_SIDE, ten_bit: true, fast: true }
    }
}

fn named(dir: &Path, source: &Path, extension: &str) -> PathBuf {
    let stem = source.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "export".into());
    let first = dir.join(format!("{stem}-graded.{extension}"));
    if !first.exists() {
        return first;
    }
    (2..).map(|n| dir.join(format!("{stem}-graded-{n}.{extension}"))).find(|p| !p.exists()).unwrap_or(first)
}

pub struct Batch {
    pub total: u32,
    pub done: Arc<AtomicU32>,
    pub cancel: Arc<AtomicBool>,
    pub finished: Receiver<Vec<String>>,
}

/// Applies one look to each file in turn. Problems are collected, not fatal, so one
/// bad file doesn't lose the others.
pub fn start_batch(files: Vec<PathBuf>, dir: PathBuf, g: Grade, bank: LutBank, photo: PhotoOptions, clip: VideoOptions) -> Batch {
    let total = files.len() as u32;
    let done = Arc::new(AtomicU32::new(0));
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, finished) = mpsc::channel();
    let (count, stop) = (done.clone(), cancel.clone());
    std::thread::spawn(move || {
        let mut problems = Vec::new();
        for f in files {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let r = if video::is_video(&f) {
                batch_clip(&f, &dir, &g, &bank, &clip, &stop)
            } else {
                open_photo(&f).and_then(|o| {
                    let out = named(&dir, &f, photo.format.extension());
                    export_photo(&out, &o.picture, &g, &bank, &photo, o.exif.as_deref())
                })
            };
            if let Err(e) = r {
                problems.push(format!("{name}: {e}"));
            }
            count.fetch_add(1, Ordering::Relaxed);
        }
        let _ = tx.send(problems);
    });
    Batch { total, done, cancel, finished }
}

fn batch_clip(f: &Path, dir: &Path, g: &Grade, bank: &LutBank, o: &VideoOptions, stop: &AtomicBool) -> Result<(), String> {
    let source = video::probe(f)?;
    let last = source.frames.saturating_sub(1);
    let extension = o.codec.extension(o.mov);
    let g = source.suited(g);
    let job = video::Export { out: named(dir, f, extension), source, codec: o.codec, side: o.side, ten_bit: o.ten_bit, fast: o.fast, first: 0, last };
    video::export(&job, &g, bank, &AtomicU32::new(0), stop)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lookroom-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir.join(name)
    }

    fn ramp() -> Picture {
        Picture { w: 64, h: 32, px: (0..64 * 32).map(|i| [(i % 64) as f32 / 63.0, 0.5, 0.25]).collect() }
    }

    /// The smallest valid Exif block: a TIFF header and one empty directory.
    const EXIF: [u8; 14] = [0x49, 0x49, 0x2A, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0];

    #[test]
    fn sixteen_bit_files_keep_their_depth() {
        for format in [Format::Png, Format::Tiff] {
            let p = temp(&format!("deep.{}", format.extension()));
            let o = PhotoOptions { format, deep: true, ..PhotoOptions::default() };
            save_photo(&p, &ramp(), &o, None).unwrap();
            let img = image::open(&p).unwrap();
            assert!(matches!(img, DynamicImage::ImageRgb16(_)), "{format:?} came back as {:?}", img.color());
            let back = open_photo(&p).unwrap().picture;
            let (a, b) = (back.px[63][0], 1.0);
            assert!((a - b).abs() < 1e-3);
            assert!((back.px[20][0] - 20.0 / 63.0).abs() < 1e-3);
        }
    }

    #[test]
    fn jpeg_and_png_carry_metadata_through_exactly() {
        for format in [Format::Jpeg, Format::Png] {
            let p = temp(&format!("meta.{}", format.extension()));
            let o = PhotoOptions { format, ..PhotoOptions::default() };
            save_photo(&p, &ramp(), &o, Some(&EXIF)).unwrap();
            assert_eq!(open_photo(&p).unwrap().exif.as_deref(), Some(&EXIF[..]), "{format:?}");
        }
    }

    #[test]
    fn tiff_carries_a_camera_name_through() {
        // One tag: the camera maker, short enough to sit inside its directory entry.
        let mut block = vec![0x49, 0x49, 0x2A, 0, 8, 0, 0, 0, 1, 0, 0x0F, 1, 2, 0, 4, 0, 0, 0];
        block.extend_from_slice(b"Cam\0");
        block.extend_from_slice(&[0; 4]);
        let p = temp("meta.tiff");
        let o = PhotoOptions { format: Format::Tiff, deep: true, ..PhotoOptions::default() };
        save_photo(&p, &ramp(), &o, Some(&block)).unwrap();
        let back = open_photo(&p).unwrap();
        assert!(back.exif.unwrap().windows(4).any(|w| w == b"Cam\0"));
        assert_eq!((back.picture.w, back.picture.h), (64, 32));
        assert!((back.picture.px[20][0] - 20.0 / 63.0).abs() < 1e-3);
    }

    #[test]
    fn turning_metadata_off_drops_it() {
        let p = temp("plain.jpg");
        let o = PhotoOptions { metadata: false, ..PhotoOptions::default() };
        save_photo(&p, &ramp(), &o, Some(&EXIF)).unwrap();
        assert!(open_photo(&p).unwrap().exif.is_none());
    }

    #[test]
    fn a_batch_finishes_what_it_can_and_reports_the_rest() {
        let dir = temp("batch");
        let _ = std::fs::create_dir_all(&dir);
        let good = dir.join("good.png");
        save_photo(&good, &ramp(), &PhotoOptions { format: Format::Png, ..PhotoOptions::default() }, None).unwrap();
        let missing = dir.join("not-there.png");
        let out = dir.join("out");
        let _ = std::fs::create_dir_all(&out);
        let g = Grade { exposure: 1.0, ..Grade::default() };
        let b = start_batch(vec![good, missing], out.clone(), g, LutBank::default(), PhotoOptions::default(), VideoOptions::default());
        let problems = b.finished.recv().unwrap();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("not-there"));
        assert_eq!(b.done.load(Ordering::Relaxed), 2);
        assert!(out.join("good-graded.jpg").exists());
    }

    #[test]
    fn a_name_already_in_use_is_not_overwritten() {
        let dir = temp("names");
        let _ = std::fs::create_dir_all(&dir);
        let first = named(&dir, Path::new("clip.mov"), "mp4");
        std::fs::write(&first, b"x").unwrap();
        let second = named(&dir, Path::new("clip.mov"), "mp4");
        assert_ne!(first, second);
        let _ = std::fs::remove_file(first);
    }
}
