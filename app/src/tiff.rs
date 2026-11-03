//! TIFF with camera metadata, which the image library neither reads nor writes.
//!
//! An Exif block is itself a small TIFF, and every offset in it counts from its own
//! start. So writing keeps the block at the front of the file, untouched, and adds the
//! picture and a new main directory after it. All the old offsets stay correct.

use crate::pipeline::Picture;

/// Tags that describe how pixels are stored. They belong to the file being written,
/// not to the camera, so they never carry over.
const LAYOUT: [u16; 26] = [
    254, 255, 256, 257, 258, 259, 262, 266, 273, 274, 277, 278, 279, 280, 281, 284, 317, 322, 323, 324, 325, 338, 339,
    340, 341, 513,
];
const THUMB_LENGTH: u16 = 514;
/// Tags whose value is the offset of another directory.
const SUB_DIRECTORIES: [u16; 3] = [34665, 34853, 40965];
const MAX_DEPTH: usize = 3;

const SHORT: u16 = 3;
const LONG: u16 = 4;
const RATIONAL: u16 = 5;

fn get_u16(b: &[u8], at: usize, big: bool) -> Option<u16> {
    let s: [u8; 2] = b.get(at..at + 2)?.try_into().ok()?;
    Some(if big { u16::from_be_bytes(s) } else { u16::from_le_bytes(s) })
}

fn get_u32(b: &[u8], at: usize, big: bool) -> Option<u32> {
    let s: [u8; 4] = b.get(at..at + 4)?.try_into().ok()?;
    Some(if big { u32::from_be_bytes(s) } else { u32::from_le_bytes(s) })
}

fn word16(v: u16, big: bool) -> [u8; 2] {
    if big { v.to_be_bytes() } else { v.to_le_bytes() }
}

fn word32(v: u32, big: bool) -> [u8; 4] {
    if big { v.to_be_bytes() } else { v.to_le_bytes() }
}

/// Byte order and the offset of the main directory.
fn header(b: &[u8]) -> Option<(bool, u32)> {
    let big = match b.get(..4)? {
        [0x49, 0x49, 42, 0] => false,
        [0x4D, 0x4D, 0, 42] => true,
        _ => return None,
    };
    Some((big, get_u32(b, 4, big)?))
}

/// The twelve-byte entries of one directory.
fn entries(b: &[u8], at: u32, big: bool) -> Option<Vec<[u8; 12]>> {
    let at = at as usize;
    let n = get_u16(b, at, big)? as usize;
    (0..n).map(|i| b.get(at + 2 + i * 12..at + 14 + i * 12)?.try_into().ok()).collect()
}

fn tag_of(e: &[u8; 12], big: bool) -> u16 {
    get_u16(e, 0, big).unwrap_or(0)
}

fn carries_over(tag: u16) -> bool {
    !LAYOUT.contains(&tag) && tag != THUMB_LENGTH
}

fn value_size(e: &[u8; 12], big: bool) -> Option<usize> {
    let unit = match get_u16(e, 2, big)? {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 => 8,
        _ => return None,
    };
    (get_u32(e, 4, big)? as usize).checked_mul(unit)
}

fn pad(out: &mut Vec<u8>) {
    if out.len() % 2 == 1 {
        out.push(0);
    }
}

/// Copies a directory and everything it points to onto the end of `out`.
fn copy(file: &[u8], list: &[[u8; 12]], big: bool, out: &mut Vec<u8>, depth: usize) -> Option<u32> {
    pad(out);
    let at = out.len();
    out.extend_from_slice(&word16(list.len() as u16, big));
    out.resize(at + 2 + list.len() * 12 + 4, 0);
    for (i, e) in list.iter().enumerate() {
        let mut entry = *e;
        let size = value_size(e, big)?;
        let tag = tag_of(e, big);
        if SUB_DIRECTORIES.contains(&tag) {
            if depth >= MAX_DEPTH {
                continue;
            }
            let sub = entries(file, get_u32(e, 8, big)?, big)?;
            let new = copy(file, &sub, big, out, depth + 1)?;
            entry[8..12].copy_from_slice(&word32(new, big));
        } else if size > 4 {
            let from = get_u32(e, 8, big)? as usize;
            let value = file.get(from..from.checked_add(size)?)?;
            pad(out);
            let new = out.len() as u32;
            out.extend_from_slice(value);
            entry[8..12].copy_from_slice(&word32(new, big));
        }
        out[at + 2 + i * 12..at + 14 + i * 12].copy_from_slice(&entry);
    }
    Some(at as u32)
}

/// The camera's tags from a TIFF file, packed into an Exif block any writer here can use.
pub fn read_exif(file: &[u8]) -> Option<Vec<u8>> {
    let (big, first) = header(file)?;
    let keep: Vec<[u8; 12]> = entries(file, first, big)?.into_iter().filter(|e| carries_over(tag_of(e, big))).collect();
    if keep.is_empty() {
        return None;
    }
    let mut out = file[..4].to_vec();
    out.extend_from_slice(&[0; 4]);
    let at = copy(file, &keep, big, &mut out, 0)?;
    out[4..8].copy_from_slice(&word32(at, big));
    Some(out)
}

fn entry(tag: u16, kind: u16, count: u32, value: u32, big: bool) -> [u8; 12] {
    let mut e = [0; 12];
    e[..2].copy_from_slice(&word16(tag, big));
    e[2..4].copy_from_slice(&word16(kind, big));
    e[4..8].copy_from_slice(&word32(count, big));
    if kind == SHORT && count == 1 {
        e[8..10].copy_from_slice(&word16(value as u16, big));
    } else {
        e[8..12].copy_from_slice(&word32(value, big));
    }
    e
}

/// An uncompressed RGB TIFF, keeping the byte order of any Exif block it carries.
pub fn write(pic: &Picture, deep: bool, exif: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let exif = exif.filter(|e| header(e).is_some());
    let big = exif.and_then(header).is_some_and(|h| h.0);
    let mut out = exif.map_or_else(|| vec![0x49, 0x49, 42, 0, 0, 0, 0, 0], <[u8]>::to_vec);
    pad(&mut out);

    let strip = out.len() as u64;
    let px: Vec<u8> = if deep {
        let w = pic.to_rgb16();
        w.iter().flat_map(|v| word16(*v, big)).collect()
    } else {
        pic.to_rgb8()
    };
    let strip_len = px.len() as u64;
    out.extend_from_slice(&px);
    pad(&mut out);

    let depth: u16 = if deep { 16 } else { 8 };
    let bits = out.len() as u64;
    for _ in 0..3 {
        out.extend_from_slice(&word16(depth, big));
    }
    let dpi = out.len() as u64;
    out.extend_from_slice(&word32(72, big));
    out.extend_from_slice(&word32(1, big));
    let directory = out.len() as u64;
    if directory > u32::MAX as u64 - 4096 {
        return Err("That picture is too large for a TIFF file.".into());
    }

    let (w, h) = (pic.w as u32, pic.h as u32);
    let mut list = vec![
        entry(256, LONG, 1, w, big),
        entry(257, LONG, 1, h, big),
        entry(258, SHORT, 3, bits as u32, big),
        entry(259, SHORT, 1, 1, big),
        entry(262, SHORT, 1, 2, big),
        entry(273, LONG, 1, strip as u32, big),
        entry(274, SHORT, 1, 1, big),
        entry(277, SHORT, 1, 3, big),
        entry(278, LONG, 1, h, big),
        entry(279, LONG, 1, strip_len as u32, big),
        entry(284, SHORT, 1, 1, big),
    ];
    if let Some(e) = exif
        && let Some((_, first)) = header(e)
        && let Some(old) = entries(e, first, big)
    {
        list.extend(old.into_iter().filter(|o| carries_over(tag_of(o, big))));
    }
    for (tag, value, kind) in [(282, dpi as u32, RATIONAL), (283, dpi as u32, RATIONAL), (296, 2, SHORT)] {
        if !list.iter().any(|e| tag_of(e, big) == tag) {
            list.push(entry(tag, kind, 1, value, big));
        }
    }
    list.sort_by_key(|e| tag_of(e, big));

    out.extend_from_slice(&word16(list.len() as u16, big));
    for e in &list {
        out.extend_from_slice(e);
    }
    out.extend_from_slice(&[0; 4]);
    out[4..8].copy_from_slice(&word32(directory as u32, big));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp() -> Picture {
        Picture { w: 8, h: 4, px: (0..32).map(|i| [i as f32 / 31.0, 0.5, 0.25]).collect() }
    }

    /// An Exif block with a camera name in the main directory and a date in its sub-directory.
    fn exif(big: bool) -> Vec<u8> {
        let mut b = if big { vec![0x4D, 0x4D, 0, 42] } else { vec![0x49, 0x49, 42, 0] };
        b.extend_from_slice(&word32(8, big));
        let (name, date) = (b"TestCam\0", b"2026:11:02 12:00:00\0");
        let (name_at, sub_at) = (8 + 2 + 3 * 12 + 4, 8 + 2 + 3 * 12 + 4 + 8);
        let date_at = sub_at + 2 + 12 + 4;
        b.extend_from_slice(&word16(3, big));
        b.extend_from_slice(&entry(271, 2, 8, name_at as u32, big));
        b.extend_from_slice(&entry(274, SHORT, 1, 6, big));
        b.extend_from_slice(&entry(34665, LONG, 1, sub_at as u32, big));
        b.extend_from_slice(&[0; 4]);
        b.extend_from_slice(name);
        b.extend_from_slice(&word16(1, big));
        b.extend_from_slice(&entry(36867, 2, 20, date_at as u32, big));
        b.extend_from_slice(&[0; 4]);
        b.extend_from_slice(date);
        b
    }

    fn tag_text(b: &[u8], tag: u16) -> Option<String> {
        let (big, first) = header(b)?;
        let mut list = entries(b, first, big)?;
        if let Some(p) = list.iter().find(|e| tag_of(e, big) == 34665).copied() {
            list.extend(entries(b, get_u32(&p, 8, big)?, big)?);
        }
        let e = list.iter().find(|e| tag_of(e, big) == tag)?;
        let (at, n) = (get_u32(e, 8, big)? as usize, get_u32(e, 4, big)? as usize);
        Some(String::from_utf8_lossy(b.get(at..at + n - 1)?).to_string())
    }

    #[test]
    fn camera_tags_survive_writing_in_either_byte_order() {
        for big in [false, true] {
            let f = write(&ramp(), true, Some(&exif(big))).unwrap();
            assert_eq!(tag_text(&f, 271).as_deref(), Some("TestCam"));
            assert_eq!(tag_text(&f, 36867).as_deref(), Some("2026:11:02 12:00:00"));
        }
    }

    #[test]
    fn the_stored_turn_is_replaced_because_pixels_are_already_upright() {
        let f = write(&ramp(), false, Some(&exif(false))).unwrap();
        let (big, first) = header(&f).unwrap();
        let list = entries(&f, first, big).unwrap();
        let o = list.iter().find(|e| tag_of(e, big) == 274).unwrap();
        assert_eq!(get_u16(o, 8, big), Some(1));
        assert_eq!(list.iter().filter(|e| tag_of(e, big) == 274).count(), 1);
    }

    #[test]
    fn directories_are_in_tag_order_as_the_format_requires() {
        let f = write(&ramp(), true, Some(&exif(true))).unwrap();
        let (big, first) = header(&f).unwrap();
        let tags: Vec<u16> = entries(&f, first, big).unwrap().iter().map(|e| tag_of(e, big)).collect();
        assert!(tags.windows(2).all(|w| w[0] < w[1]), "{tags:?}");
    }

    #[test]
    fn tags_read_back_out_of_a_tiff_file() {
        for big in [false, true] {
            let file = write(&ramp(), true, Some(&exif(big))).unwrap();
            let block = read_exif(&file).unwrap();
            assert_eq!(tag_text(&block, 271).as_deref(), Some("TestCam"));
            assert_eq!(tag_text(&block, 36867).as_deref(), Some("2026:11:02 12:00:00"));
            // What comes out holds no pixels, so it stays small.
            assert!(block.len() < 200, "{}", block.len());
        }
    }

    #[test]
    fn a_plain_file_has_no_tags_worth_keeping() {
        let file = write(&ramp(), false, None).unwrap();
        let block = read_exif(&file);
        // Only the resolution tags this writer adds remain.
        assert!(block.is_none_or(|b| tag_text(&b, 271).is_none()));
    }

    #[test]
    fn broken_blocks_are_ignored_rather_than_trusted() {
        assert!(write(&ramp(), false, Some(&[1, 2, 3])).is_ok());
        let mut bad = exif(false);
        bad.truncate(20);
        assert!(read_exif(&bad).is_none());
    }
}
