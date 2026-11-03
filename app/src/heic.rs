//! Reads and writes HEIC stills. ffmpeg can encode the picture but has no HEIC container,
//! so the file is assembled here from its HEVC stream, following ISO/IEC 23008-12.
//! Large pictures are split into 512-pixel tiles, the same layout phones use.

use crate::pipeline::Picture;
use crate::video;
use rayon::prelude::*;
use std::io::Write;
use std::process::Stdio;

const NAL_VPS: u8 = 32;
const NAL_SPS: u8 = 33;
const NAL_PPS: u8 = 34;
const BT709: u16 = 1;
const SRGB_TRANSFER: u16 = 13;
const TILE: usize = 512;
/// The longest side kept as a single picture; beyond this hardware decoders may refuse it.
const GRID_ABOVE: usize = 4096;
/// Association numbers stay in one byte below this.
const SMALL_INDEX: usize = 127;

fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut o = Vec::with_capacity(body.len() + 8);
    o.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
    o.extend_from_slice(kind);
    o.extend_from_slice(body);
    o
}

fn full(kind: &[u8; 4], version: u8, flags: u32, body: &[u8]) -> Vec<u8> {
    let mut b = vec![version];
    b.extend_from_slice(&flags.to_be_bytes()[1..]);
    b.extend_from_slice(body);
    boxed(kind, &b)
}

/// Splits an Annex B stream into its units, without their start codes.
fn units(stream: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= stream.len() {
        if stream[i] == 0 && stream[i + 1] == 0 && stream[i + 2] == 1 {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::new();
    for (n, s) in starts.iter().enumerate() {
        let mut end = starts.get(n + 1).map_or(stream.len(), |e| e - 3);
        while end > *s && stream[end - 1] == 0 {
            end -= 1;
        }
        out.push(&stream[*s..end]);
    }
    out
}

#[inline]
fn kind_of(unit: &[u8]) -> u8 {
    (unit[0] >> 1) & 0x3F
}

/// Removes the escape bytes the stream adds so its data can't look like a start code.
fn unescape(unit: &[u8]) -> Vec<u8> {
    let mut o = Vec::with_capacity(unit.len());
    let mut zeros = 0;
    for b in unit {
        if zeros >= 2 && *b == 3 {
            zeros = 0;
            continue;
        }
        zeros = if *b == 0 { zeros + 1 } else { 0 };
        o.push(*b);
    }
    o
}

/// The decoder setup box, which carries the three parameter sets.
fn hvcc(vps: &[u8], sps: &[u8], pps: &[u8], depth: u8) -> Result<Vec<u8>, String> {
    let raw = unescape(sps);
    // After the two-byte header and one byte of ids: 12 bytes of profile and level.
    let ptl = raw.get(3..15).ok_or("The encoder's stream settings are too short.")?;
    let mut b = vec![1];
    b.extend_from_slice(ptl);
    b.extend_from_slice(&[0xF0, 0x00]); // min_spatial_segmentation_idc
    b.push(0xFC); // parallelismType
    b.push(0xFC | 1); // chroma_format_idc: 4:2:0
    b.push(0xF8 | (depth - 8));
    b.push(0xF8 | (depth - 8));
    b.extend_from_slice(&[0, 0]); // avgFrameRate
    b.push(0x0F); // lengthSizeMinusOne: 3, four-byte lengths
    b.push(3);
    for (kind, unit) in [(NAL_VPS, vps), (NAL_SPS, sps), (NAL_PPS, pps)] {
        b.push(0x80 | kind);
        b.extend_from_slice(&1u16.to_be_bytes());
        b.extend_from_slice(&(unit.len() as u16).to_be_bytes());
        b.extend_from_slice(unit);
    }
    Ok(boxed(b"hvcC", &b))
}

/// One encoded picture, ready for the container.
struct Coded {
    config: Vec<u8>,
    data: Vec<u8>,
}

fn coded(stream: &[u8], depth: u8) -> Result<Coded, String> {
    let list = units(stream);
    let find = |k: u8| list.iter().find(|u| kind_of(u) == k).copied();
    let (Some(vps), Some(sps), Some(pps)) = (find(NAL_VPS), find(NAL_SPS), find(NAL_PPS)) else {
        return Err("The encoder gave no stream settings.".into());
    };
    let mut data = Vec::new();
    for u in list.iter().filter(|u| kind_of(u) < 32) {
        data.extend_from_slice(&(u.len() as u32).to_be_bytes());
        data.extend_from_slice(u);
    }
    if data.is_empty() {
        return Err("The encoder gave no picture.".into());
    }
    Ok(Coded { config: hvcc(vps, sps, pps, depth)?, data })
}

fn infe(id: u16, kind: &[u8; 4], hidden: bool) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&id.to_be_bytes());
    b.extend_from_slice(&0u16.to_be_bytes());
    b.extend_from_slice(kind);
    b.push(0);
    full(b"infe", 2, hidden as u32, &b)
}

fn iloc_entry(id: u16, at: (u32, u32)) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&id.to_be_bytes());
    b.extend_from_slice(&0u16.to_be_bytes()); // data_reference_index
    b.extend_from_slice(&1u16.to_be_bytes()); // extent_count
    b.extend_from_slice(&at.0.to_be_bytes());
    b.extend_from_slice(&at.1.to_be_bytes());
    b
}

fn ispe(w: u32, h: u32) -> Vec<u8> {
    let mut b = w.to_be_bytes().to_vec();
    b.extend_from_slice(&h.to_be_bytes());
    full(b"ispe", 0, 0, &b)
}

/// What the file holds. The offsets come from `Places`, because they depend on this box's size.
struct Plan {
    w: u32,
    h: u32,
    depth: u8,
    configs: Vec<Vec<u8>>,
    /// Which decoder setup each coded picture uses.
    uses: Vec<usize>,
    tiled: bool,
    has_exif: bool,
    /// Quarter turns anticlockwise for a viewer to apply. Phones store portraits this way.
    turn: u8,
}

#[derive(Default, Clone)]
struct Places {
    coded: Vec<(u32, u32)>,
    grid: (u32, u32),
    exif: (u32, u32),
}

fn meta(plan: &Plan, at: &Places) -> Vec<u8> {
    let count = plan.uses.len() as u16;
    let primary = 1u16;
    let first_coded = if plan.tiled { 2 } else { 1 };
    let exif_id = first_coded + count;
    let items = count + plan.tiled as u16 + plan.has_exif as u16;

    let mut body = full(b"hdlr", 0, 0, &{
        let mut b = vec![0; 4];
        b.extend_from_slice(b"pict");
        b.extend_from_slice(&[0; 12]);
        b.push(0);
        b
    });
    body.extend(full(b"pitm", 0, 0, &primary.to_be_bytes()));

    let mut loc = vec![0x44, 0x00]; // four-byte offsets and lengths, no base offset
    loc.extend_from_slice(&items.to_be_bytes());
    if plan.tiled {
        loc.extend(iloc_entry(primary, at.grid));
    }
    for (i, place) in at.coded.iter().enumerate() {
        loc.extend(iloc_entry(first_coded + i as u16, *place));
    }
    if plan.has_exif {
        loc.extend(iloc_entry(exif_id, at.exif));
    }
    body.extend(full(b"iloc", 0, 0, &loc));

    let mut inf = items.to_be_bytes().to_vec();
    if plan.tiled {
        inf.extend(infe(primary, b"grid", false));
    }
    for i in 0..count {
        inf.extend(infe(first_coded + i, b"hvc1", plan.tiled));
    }
    if plan.has_exif {
        inf.extend(infe(exif_id, b"Exif", false));
    }
    body.extend(full(b"iinf", 0, 0, &inf));

    let mut refs = Vec::new();
    if plan.tiled {
        let mut r = primary.to_be_bytes().to_vec();
        r.extend_from_slice(&count.to_be_bytes());
        for i in 0..count {
            r.extend_from_slice(&(first_coded + i).to_be_bytes());
        }
        refs.extend(boxed(b"dimg", &r));
    }
    if plan.has_exif {
        let mut r = exif_id.to_be_bytes().to_vec();
        r.extend_from_slice(&1u16.to_be_bytes());
        r.extend_from_slice(&primary.to_be_bytes());
        refs.extend(boxed(b"cdsc", &r));
    }
    if !refs.is_empty() {
        body.extend(full(b"iref", 0, 0, &refs));
    }

    // Properties are numbered from one, in the order they are listed.
    let mut props: Vec<u8> = plan.configs.concat();
    let n = plan.configs.len();
    let whole = n + 1;
    props.extend(ispe(plan.w, plan.h));
    let tile = n + 2;
    if plan.tiled {
        props.extend(ispe(TILE as u32, TILE as u32));
    }
    let pixi = n + 2 + plan.tiled as usize;
    let d = plan.depth;
    props.extend(full(b"pixi", 0, 0, &[3, d, d, d]));
    let colr = pixi + 1;
    props.extend(boxed(b"colr", &{
        let mut b = b"nclx".to_vec();
        for v in [BT709, SRGB_TRANSFER, BT709] {
            b.extend_from_slice(&v.to_be_bytes());
        }
        b.push(0x00); // limited range, which is what the encoder is given
        b
    }));
    let irot = colr + 1;
    let turned = plan.turn > 0;
    if turned {
        props.extend(boxed(b"irot", &[plan.turn]));
    }

    let wide = irot > SMALL_INDEX;
    let link = |list: &mut Vec<u8>, index: usize, essential: bool| {
        if wide {
            list.extend_from_slice(&((index as u16) | ((essential as u16) << 15)).to_be_bytes());
        } else {
            list.push(index as u8 | ((essential as u8) << 7));
        }
    };
    let mut ipma = (count as u32 + plan.tiled as u32).to_be_bytes().to_vec();
    if plan.tiled {
        ipma.extend_from_slice(&primary.to_be_bytes());
        ipma.push(3 + turned as u8);
        link(&mut ipma, whole, false);
        link(&mut ipma, pixi, false);
        link(&mut ipma, colr, false);
        if turned {
            link(&mut ipma, irot, true);
        }
    }
    for (i, config) in plan.uses.iter().enumerate() {
        ipma.extend_from_slice(&(first_coded + i as u16).to_be_bytes());
        let spins = turned && !plan.tiled;
        ipma.push(4 + spins as u8);
        link(&mut ipma, config + 1, true);
        link(&mut ipma, if plan.tiled { tile } else { whole }, false);
        link(&mut ipma, pixi, false);
        link(&mut ipma, colr, false);
        if spins {
            link(&mut ipma, irot, true);
        }
    }
    let mut iprp = boxed(b"ipco", &props);
    iprp.extend(full(b"ipma", 0, wide as u32, &ipma));
    body.extend(boxed(b"iprp", &iprp));
    full(b"meta", 0, 0, &body)
}

/// Lays out the file. Offsets depend on the header's size, so its size is measured first.
fn assemble(streams: &[Vec<u8>], w: u32, h: u32, depth: u8, grid: Option<(usize, usize)>, turn: u8, exif: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let pictures: Vec<Coded> = streams.iter().map(|s| coded(s, depth)).collect::<Result<_, _>>()?;
    // Tiles nearly always share one setup, and each copy would cost a kilobyte.
    let mut configs: Vec<Vec<u8>> = Vec::new();
    let mut uses = Vec::with_capacity(pictures.len());
    for p in &pictures {
        let at = configs.iter().position(|c| *c == p.config).unwrap_or_else(|| {
            configs.push(p.config.clone());
            configs.len() - 1
        });
        uses.push(at);
    }
    // Exif items begin with how far the TIFF header sits into the data, here zero.
    let exif_data: Option<Vec<u8>> = exif.map(|e| {
        let mut d = 0u32.to_be_bytes().to_vec();
        d.extend_from_slice(e);
        d
    });
    let grid_data: Option<Vec<u8>> = grid.map(|(rows, cols)| {
        let large = w > u16::MAX as u32 || h > u16::MAX as u32;
        let mut d = vec![0, large as u8, (rows - 1) as u8, (cols - 1) as u8];
        if large {
            d.extend_from_slice(&w.to_be_bytes());
            d.extend_from_slice(&h.to_be_bytes());
        } else {
            d.extend_from_slice(&(w as u16).to_be_bytes());
            d.extend_from_slice(&(h as u16).to_be_bytes());
        }
        d
    });

    let plan = Plan { w, h, depth, configs, uses, tiled: grid.is_some(), has_exif: exif.is_some(), turn };
    let ftyp = boxed(b"ftyp", b"heic\0\0\0\0mif1heic");
    let blank = Places { coded: vec![(0, 0); pictures.len()], ..Places::default() };
    let start = (ftyp.len() + meta(&plan, &blank).len() + 8) as u64;

    let mut payload = Vec::new();
    let mut places = Places::default();
    let put = |payload: &mut Vec<u8>, data: &[u8]| {
        let at = (start + payload.len() as u64, data.len() as u64);
        payload.extend_from_slice(data);
        at
    };
    let small = |v: (u64, u64)| -> Result<(u32, u32), String> {
        u32::try_from(v.0).ok().zip(u32::try_from(v.1).ok()).ok_or_else(|| "That picture is too large for a HEIC file.".to_string())
    };
    if let Some(d) = &exif_data {
        places.exif = small(put(&mut payload, d))?;
    }
    if let Some(d) = &grid_data {
        places.grid = small(put(&mut payload, d))?;
    }
    for p in &pictures {
        let at = small(put(&mut payload, &p.data))?;
        places.coded.push(at);
    }
    let mut out = ftyp;
    out.extend(meta(&plan, &places));
    out.extend(boxed(b"mdat", &payload));
    Ok(out)
}

/// A region as 16-bit RGB. Past the picture's edge the last pixels repeat,
/// because every tile has to be the same size.
fn region(pic: &Picture, x0: usize, y0: usize, w: usize, h: usize) -> Vec<u8> {
    let mut b = Vec::with_capacity(w * h * 6);
    for y in y0..y0 + h {
        let row = y.min(pic.h - 1) * pic.w;
        for x in x0..x0 + w {
            for v in pic.px[row + x.min(pic.w - 1)] {
                b.extend_from_slice(&((v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16).to_le_bytes());
            }
        }
    }
    b
}

fn hevc(bytes: Vec<u8>, w: usize, h: usize, crf: u32, ten_bit: bool, threads: &str) -> Result<Vec<u8>, String> {
    let (format, profile) = if ten_bit { ("yuv420p10le", "main10") } else { ("yuv420p", "mainstillpicture") };
    let mut child = video::tool("ffmpeg")
        .args(["-y", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgb48le"])
        .arg("-s")
        .arg(format!("{w}x{h}"))
        .args(["-i", "pipe:0", "-frames:v", "1", "-vf"])
        .arg("scale=out_color_matrix=bt709:out_range=tv:flags=accurate_rnd+full_chroma_int")
        .args(["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "iec61966-2-1", "-color_range", "tv"])
        .args(["-c:v", "libx265", "-preset", "medium", "-pix_fmt", format, "-profile:v", profile])
        .arg("-crf")
        .arg(crf.to_string())
        // Telling the 8-bit encoder there is one frame labels the result a still picture.
        // At 10 bits the same hint gives a range-extension label fewer decoders accept.
        .arg("-x265-params")
        .arg(format!("log-level=error{}:bframes=0:repeat-headers=1:info=0{threads}", if ten_bit { "" } else { ":total-frames=1" }))
        .args(["-f", "hevc", "pipe:1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| video::cannot_start(&e))?;
    let mut stdin = child.stdin.take().ok_or("The encoder did not open.")?;
    let writer = std::thread::spawn(move || stdin.write_all(&bytes));
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let _ = writer.join();
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(out.stdout)
}

/// `quality` runs 1 to 100, as for JPEG.
pub fn encode(pic: &Picture, quality: u8, ten_bit: bool, exif: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let (w, h) = (pic.w & !1, pic.h & !1);
    if w < 2 || h < 2 {
        return Err("The picture is too small.".into());
    }
    let crf = 51 - (quality.clamp(1, 100) as u32 * 39 / 100);
    let depth = if ten_bit { 10 } else { 8 };
    if w.max(h) <= GRID_ABOVE {
        let stream = hevc(region(pic, 0, 0, w, h), w, h, crf, ten_bit, "")?;
        return assemble(&[stream], w as u32, h as u32, depth, None, 0, exif);
    }
    let (cols, rows) = (w.div_ceil(TILE), h.div_ceil(TILE));
    let cells: Vec<(usize, usize)> = (0..rows).flat_map(|r| (0..cols).map(move |c| (r, c))).collect();
    // Many small encoders at once, so each is held to its own share of the processor.
    let streams: Vec<Vec<u8>> = cells
        .par_iter()
        .map(|(r, c)| hevc(region(pic, c * TILE, r * TILE, TILE, TILE), TILE, TILE, crf, ten_bit, ":pools=2:frame-threads=1"))
        .collect::<Result<_, _>>()?;
    assemble(&streams, w as u32, h as u32, depth, Some((rows, cols)), 0, exif)
}

/// The body of the first box of a kind in a run of boxes.
fn find<'a>(b: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    let mut at = 0;
    while at + 8 <= b.len() {
        let size = u32::from_be_bytes(b[at..at + 4].try_into().ok()?) as usize;
        let (head, size) = match size {
            0 => (8, b.len() - at),
            1 => (16, u64::from_be_bytes(b.get(at + 8..at + 16)?.try_into().ok()?) as usize),
            s => (8, s),
        };
        if size < head {
            return None;
        }
        if &b[at + 4..at + 8] == kind {
            return b.get(at + head..at.checked_add(size)?);
        }
        at = at.checked_add(size)?;
    }
    None
}

/// Reads big-endian numbers of the widths the location box declares.
struct Cursor<'a> {
    b: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn take(&mut self, n: usize) -> Option<u64> {
        let v = self.b.get(self.at..self.at + n)?.iter().fold(0u64, |a, x| (a << 8) | *x as u64);
        self.at += n;
        Some(v)
    }
}

/// The item number of the Exif block, from the item list.
fn exif_item(meta: &[u8]) -> Option<u32> {
    let iinf = find(meta, b"iinf")?;
    let wide = *iinf.first()? != 0;
    let list = iinf.get(if wide { 8 } else { 6 }..)?;
    let mut at = 0;
    while at + 8 <= list.len() {
        let size = u32::from_be_bytes(list[at..at + 4].try_into().ok()?) as usize;
        let body = list.get(at + 8..at + size)?;
        let version = *body.first()?;
        let (id, kind_at) = match version {
            2 => (u16::from_be_bytes(body.get(4..6)?.try_into().ok()?) as u32, 8),
            3 => (u32::from_be_bytes(body.get(4..8)?.try_into().ok()?), 10),
            _ => (0, usize::MAX - 8),
        };
        if version >= 2 && body.get(kind_at..kind_at + 4)? == b"Exif" {
            return Some(id);
        }
        at += size.max(8);
    }
    None
}

/// Where an item's bytes are in the file, joining its pieces.
fn item_data(file: &[u8], meta: &[u8], item: u32) -> Option<Vec<u8>> {
    let iloc = find(meta, b"iloc")?;
    let version = *iloc.first()?;
    let (sizes, more) = (*iloc.get(4)?, *iloc.get(5)?);
    let (offset_size, length_size) = ((sizes >> 4) as usize, (sizes & 15) as usize);
    let (base_size, index_size) = ((more >> 4) as usize, (more & 15) as usize);
    let mut c = Cursor { b: iloc, at: 6 };
    let id_size = if version < 2 { 2 } else { 4 };
    for _ in 0..c.take(id_size)? {
        let id = c.take(id_size)? as u32;
        let method = if version >= 1 { c.take(2)? & 15 } else { 0 };
        c.take(2)?; // data_reference_index
        let base = c.take(base_size)? as usize;
        let mut data = Vec::new();
        for _ in 0..c.take(2)? {
            if version >= 1 {
                c.take(index_size)?;
            }
            let (off, len) = (c.take(offset_size)? as usize, c.take(length_size)? as usize);
            if id == item && method == 0 {
                let at = base.checked_add(off)?;
                data.extend_from_slice(file.get(at..at.checked_add(len)?)?);
            }
        }
        if id == item {
            return (!data.is_empty()).then_some(data);
        }
    }
    None
}

/// The camera's Exif block from a HEIC or AVIF file, in the form the writers expect.
pub fn read_exif(file: &[u8]) -> Option<Vec<u8>> {
    let meta = find(file, b"meta")?.get(4..)?;
    let data = item_data(file, meta, exif_item(meta)?)?;
    let skip = u32::from_be_bytes(data.get(..4)?.try_into().ok()?) as usize;
    data.get(4 + skip..).map(<[u8]>::to_vec)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stream with the three settings units and one picture unit, none of it decodable.
    fn stream(salt: u8) -> Vec<u8> {
        let mut s = Vec::new();
        for unit in [
            vec![0x40, 1, 7],
            vec![0x42, 1, 1, 1, 0x60, 0, 0, 0, 0x90, 0, 0, 0, 0, 0, 120, salt],
            vec![0x44, 1, 9],
            vec![0x26, 1, salt, 200],
        ] {
            s.extend_from_slice(&[0, 0, 0, 1]);
            s.extend_from_slice(&unit);
        }
        s
    }

    const EXIF: &[u8] = b"II*\0\x08\0\0\0\0\0\0\0\0\0";

    #[test]
    fn units_are_found_with_either_start_code() {
        let s = [0, 0, 0, 1, 0x40, 1, 7, 0, 0, 1, 0x42, 1, 9];
        let u = units(&s);
        assert_eq!(u.len(), 2);
        assert_eq!(kind_of(u[0]), NAL_VPS);
        assert_eq!(kind_of(u[1]), NAL_SPS);
        assert_eq!(u[1], &[0x42, 1, 9]);
    }

    #[test]
    fn escape_bytes_are_removed() {
        assert_eq!(unescape(&[1, 0, 0, 3, 1, 0, 0, 3, 0]), [1, 0, 0, 1, 0, 0, 0]);
    }

    #[test]
    fn a_box_records_its_own_length() {
        let b = boxed(b"test", &[1, 2, 3]);
        assert_eq!(&b[..4], &[0, 0, 0, 11]);
        assert_eq!(&b[4..8], b"test");
    }

    #[test]
    fn a_stream_with_no_settings_is_refused() {
        assert!(assemble(&[vec![0, 0, 1, 0x26, 1, 2]], 16, 16, 8, None, 0, None).is_err());
    }

    #[test]
    fn exif_survives_the_trip_through_a_single_picture() {
        let f = assemble(&[stream(1)], 64, 64, 8, None, 0, Some(EXIF)).unwrap();
        assert_eq!(read_exif(&f).as_deref(), Some(EXIF));
    }

    #[test]
    fn exif_survives_the_trip_through_a_grid() {
        let list = vec![stream(1), stream(1), stream(1), stream(1)];
        let f = assemble(&list, 1000, 1000, 8, Some((2, 2)), 0, Some(EXIF)).unwrap();
        assert_eq!(read_exif(&f).as_deref(), Some(EXIF));
    }

    #[test]
    fn a_file_with_no_exif_says_so() {
        let f = assemble(&[stream(1)], 64, 64, 8, None, 0, None).unwrap();
        assert!(read_exif(&f).is_none());
        assert!(read_exif(b"not a heic file").is_none());
    }

    #[test]
    fn tiles_sharing_one_setup_keep_only_one_copy_of_it() {
        let same = assemble(&[stream(1), stream(1)], 1000, 512, 8, Some((1, 2)), 0, None).unwrap();
        let split = assemble(&[stream(1), stream(2)], 1000, 512, 8, Some((1, 2)), 0, None).unwrap();
        let count = |f: &[u8]| f.windows(4).filter(|w| *w == b"hvcC").count();
        assert_eq!(count(&same), 1);
        assert_eq!(count(&split), 2);
    }

    #[test]
    fn the_primary_item_of_a_grid_is_the_grid() {
        let f = assemble(&[stream(1), stream(1)], 1000, 512, 8, Some((1, 2)), 0, None).unwrap();
        let meta = find(&f, b"meta").unwrap().get(4..).unwrap();
        let pitm = find(meta, b"pitm").unwrap();
        assert_eq!(&pitm[4..6], &[0, 1]);
        let data = item_data(&f, meta, 1).unwrap();
        // Version, flags, one row, two columns, then the full picture's size.
        assert_eq!(data, [0, 0, 0, 1, 0x03, 0xE8, 0x02, 0x00]);
    }

    /// A picture that is bright on its right, so a turn shows up in the pixels too.
    fn half_lit(w: usize, h: usize) -> Picture {
        let px = (0..w * h).map(|i| [if i % w > w / 2 { 0.9 } else { 0.1 }; 3]).collect();
        Picture { w, h, px }
    }

    fn read_turned(name: &str, file: Vec<u8>) -> Picture {
        let p = std::env::temp_dir().join(format!("lookroom-{}-{name}.heic", std::process::id()));
        std::fs::write(&p, file).unwrap();
        let back = video::still(&p).unwrap();
        let _ = std::fs::remove_file(p);
        back
    }

    #[test]
    fn both_depths_really_encode_and_decode() {
        if !video::available() {
            return;
        }
        for ten_bit in [false, true] {
            let back = read_turned(if ten_bit { "ten" } else { "eight" }, encode(&half_lit(256, 128), 90, ten_bit, None).unwrap());
            assert_eq!((back.w, back.h), (256, 128));
            assert!((back.px[220 + 64 * 256][0] - 0.9).abs() < 0.04, "{ten_bit}");
            assert!((back.px[30 + 64 * 256][0] - 0.1).abs() < 0.04, "{ten_bit}");
        }
    }

    #[test]
    fn a_large_picture_is_tiled_at_either_depth() {
        if !video::available() {
            return;
        }
        for ten_bit in [false, true] {
            let f = encode(&half_lit(GRID_ABOVE + 100, 600), 60, ten_bit, None).unwrap();
            assert!(f.windows(4).any(|w| w == b"grid"));
            let back = read_turned("wide", f);
            assert_eq!((back.w, back.h), (GRID_ABOVE + 100, 600));
        }
    }

    #[test]
    fn a_portrait_stored_as_a_turn_reads_back_upright() {
        if !video::available() {
            return;
        }
        let pic = half_lit(512, 256);
        let s = hevc(region(&pic, 0, 0, 512, 256), 512, 256, 20, false, "").unwrap();
        let back = read_turned("single", assemble(&[s], 512, 256, 8, None, 1, None).unwrap());
        assert_eq!((back.w, back.h), (256, 512));
        // A quarter turn anticlockwise carries the bright right side to the top.
        assert!(back.px[128 + 40 * 256][0] > 0.7);
        assert!(back.px[128 + 470 * 256][0] < 0.3);
    }

    #[test]
    fn a_tiled_portrait_reads_back_upright() {
        if !video::available() {
            return;
        }
        let pic = half_lit(1000, 512);
        let tiles: Vec<Vec<u8>> = (0..2).map(|c| hevc(region(&pic, c * TILE, 0, TILE, TILE), TILE, TILE, 20, false, "").unwrap()).collect();
        let back = read_turned("grid", assemble(&tiles, 1000, 512, 8, Some((1, 2)), 1, None).unwrap());
        assert_eq!((back.w, back.h), (512, 1000));
        assert!(back.px[256 + 80 * 512][0] > 0.7);
        assert!(back.px[256 + 900 * 512][0] < 0.3);
    }

    #[test]
    fn edges_repeat_so_every_tile_is_full_size() {
        let pic = Picture { w: 2, h: 1, px: vec![[0.0; 3], [1.0; 3]] };
        let b = region(&pic, 1, 0, 3, 2);
        assert_eq!(b.len(), 3 * 2 * 6);
        assert_eq!(&b[..2], &[255, 255]);
        assert_eq!(&b[b.len() - 2..], &[255, 255]);
    }
}
