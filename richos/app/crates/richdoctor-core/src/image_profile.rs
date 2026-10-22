//! **THE SCREENSHOT FORMAT PROFILE** (PRD §7: "The client encoder and Worker verifier share one
//! versioned format profile and golden fixtures").
//!
//! An approved screenshot is a PNG or a JPEG whose structure holds only:
//!
//!   - PNG: `IHDR PLTE IDAT IEND` plus the rendering hints `tRNS sRGB gAMA pHYs`. No text
//!     (`tEXt zTXt iTXt`), EXIF (`eXIf`), ICC (`iCCP`), time (`tIME`) or any other chunk.
//!   - JPEG: the structural markers (SOI, SOFn, DHT, DQT, DRI, SOS and its entropy-coded data
//!     with RSTn, EOI) plus an optional JFIF APP0. No COM, no other APPn (EXIF is APP1, ICC is
//!     APP2, Adobe is APP14), no unknown marker.
//!
//! [`check`] reads only the structure (chunk and segment headers, and the dimensions in IHDR or
//! SOFn), never the pixels, exactly as the relay's verifier does. The limits (PRD §6): 10
//! images, 5 MiB each, 25 MiB together, 40 megapixels before any decode.
use serde::{Deserialize, Serialize};

pub const PROFILE_VERSION: u32 = 1;
pub const MAX_IMAGES: usize = 10;
pub const MAX_IMAGE_BYTES: u64 = 5 << 20;
pub const MAX_TOTAL_BYTES: u64 = 25 << 20;
pub const MAX_PIXELS: u64 = 40_000_000;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const PNG_ALLOWED: [&[u8; 4]; 8] = [b"IHDR", b"PLTE", b"IDAT", b"IEND", b"tRNS", b"sRGB", b"gAMA", b"pHYs"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/jpeg")]
    Jpeg,
}

impl Kind {
    pub fn media_type(self) -> &'static str {
        match self {
            Kind::Png => "image/png",
            Kind::Jpeg => "image/jpeg",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Kind::Png => "png",
            Kind::Jpeg => "jpg",
        }
    }
}

/// What the structure says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub kind: Kind,
    pub width: u32,
    pub height: u32,
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

fn png(bytes: &[u8]) -> Result<Shape, String> {
    let mut at = PNG_SIGNATURE.len();
    let mut shape = None;
    let mut ended = false;
    let mut first = true;
    while at < bytes.len() {
        if ended {
            return Err("data after IEND".into());
        }
        if at + 12 > bytes.len() {
            return Err("a truncated chunk".into());
        }
        let len = be32(&bytes[at..]) as usize;
        let ty: [u8; 4] = bytes[at + 4..at + 8].try_into().unwrap();
        let end = at.checked_add(12).and_then(|x| x.checked_add(len)).ok_or("a chunk length overflows")?;
        if end > bytes.len() {
            return Err("a chunk runs past the end".into());
        }
        if first && &ty != b"IHDR" {
            return Err("the first chunk is not IHDR".into());
        }
        first = false;
        if !PNG_ALLOWED.iter().any(|a| **a == ty) {
            return Err(format!("a {} chunk is not allowed", String::from_utf8_lossy(&ty)));
        }
        if &ty == b"IHDR" {
            if len != 13 || shape.is_some() {
                return Err("a bad IHDR".into());
            }
            shape = Some(Shape { kind: Kind::Png, width: be32(&bytes[at + 8..]), height: be32(&bytes[at + 12..]) });
        }
        if &ty == b"IEND" {
            ended = true;
        }
        at = end;
    }
    if !ended {
        return Err("no IEND".into());
    }
    shape.ok_or_else(|| "no IHDR".into())
}

fn jpeg(bytes: &[u8]) -> Result<Shape, String> {
    let mut at = 2;
    let mut shape = None;
    loop {
        if at + 2 > bytes.len() {
            return Err("a truncated segment".into());
        }
        if bytes[at] != 0xFF {
            return Err("a segment does not start with a marker".into());
        }
        let m = bytes[at + 1];
        match m {
            0xD9 => return if at + 2 == bytes.len() { shape.ok_or_else(|| "no frame header".into()) } else { Err("data after EOI".into()) },
            0xFF => {
                at += 1;
                continue;
            }
            _ => {}
        }
        if at + 4 > bytes.len() {
            return Err("a truncated segment".into());
        }
        let len = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
        if len < 2 || at + 2 + len > bytes.len() {
            return Err("a segment runs past the end".into());
        }
        let seg = &bytes[at + 4..at + 2 + len];
        match m {
            // SOF0..SOF15 except DHT (C4), JPG (C8) and DAC (CC).
            0xC0..=0xCF if m != 0xC4 && m != 0xC8 && m != 0xCC => {
                if seg.len() < 5 || shape.is_some() {
                    return Err("a bad frame header".into());
                }
                let h = u16::from_be_bytes([seg[1], seg[2]]) as u32;
                let w = u16::from_be_bytes([seg[3], seg[4]]) as u32;
                shape = Some(Shape { kind: Kind::Jpeg, width: w, height: h });
            }
            // DHT, DQT, DRI and DNL.
            0xC4 | 0xDB | 0xDD | 0xDC => {}
            0xE0 => {
                if !seg.starts_with(b"JFIF\0") {
                    return Err("an APP0 segment that is not JFIF".into());
                }
            }
            0xDA => {
                // The scan: skip its entropy-coded data, which may hold restart markers and
                // stuffed 0xFF00 bytes, to the next real marker.
                at += 2 + len;
                loop {
                    if at + 1 >= bytes.len() {
                        return Err("a scan runs past the end".into());
                    }
                    if bytes[at] == 0xFF && bytes[at + 1] != 0x00 && !(0xD0..=0xD7).contains(&bytes[at + 1]) {
                        break;
                    }
                    at += 1;
                }
                continue;
            }
            0xFE => return Err("a comment segment is not allowed".into()),
            0xE1..=0xEF => return Err(format!("an APP{} segment is not allowed", m - 0xE0)),
            _ => return Err(format!("a 0x{m:02X} segment is not allowed")),
        }
        at += 2 + len;
    }
}

/// **CHECK ONE IMAGE AGAINST THE PROFILE AND THE LIMITS.** Structure only, no pixel decode.
pub fn check(bytes: &[u8]) -> Result<Shape, String> {
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("larger than 5 MiB".into());
    }
    structure(bytes)
}

/// The profile's structure and dimensions, without the size limit.
fn structure(bytes: &[u8]) -> Result<Shape, String> {
    let shape = if bytes.starts_with(PNG_SIGNATURE) {
        png(bytes)?
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        jpeg(bytes)?
    } else {
        return Err("not a PNG or JPEG image".into());
    };
    if shape.width == 0 || shape.height == 0 {
        return Err("an empty image".into());
    }
    if shape.width as u64 * shape.height as u64 > MAX_PIXELS {
        return Err("more than 40 megapixels".into());
    }
    Ok(shape)
}

/// The dimensions a PNG or JPEG declares, read from its header before anything is decoded (the
/// 40-megapixel refusal happens before any pixel buffer exists).
pub fn declared(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.starts_with(PNG_SIGNATURE) && bytes.len() >= 24 && &bytes[12..16] == b"IHDR" {
        return Some((be32(&bytes[16..]), be32(&bytes[20..])));
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        let mut at = 2;
        while at + 4 <= bytes.len() && bytes[at] == 0xFF {
            let m = bytes[at + 1];
            let len = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
            if (0xC0..=0xCF).contains(&m) && m != 0xC4 && m != 0xC8 && m != 0xCC && at + 9 <= bytes.len() {
                return Some((u16::from_be_bytes([bytes[at + 7], bytes[at + 8]]) as u32, u16::from_be_bytes([bytes[at + 5], bytes[at + 6]]) as u32));
            }
            if m == 0xDA {
                return None;
            }
            at += 2 + len;
        }
    }
    None
}

/// **THE FINAL BYTES OF AN APPROVED SCREENSHOT** (PRD §6: "Strip metadata and flatten the
/// redacted result into a new image"; the window has already burned the person's covers into
/// the pixels). A PNG is decoded and written again from its pixels alone (IHDR, IDAT, IEND), so
/// no chunk of the original survives. A JPEG keeps its frame, tables and scan and loses every
/// segment outside the profile (EXIF, ICC, comments, any APPn but JFIF). The result passes
/// [`check`], or the reason it cannot is returned for the person.
pub fn finalize(input: &[u8]) -> Result<(Vec<u8>, Shape), String> {
    let (out, shape) = encode(input)?;
    check(&out).map_err(|e| format!("it cannot be sent as it is ({e})"))?;
    Ok((out, shape))
}

/// [`finalize`] without the size limit: the new bytes and their shape, so the window can see an
/// image that encodes above 5 MiB and reduce it (the size is checked again when it is kept).
pub fn encode(input: &[u8]) -> Result<(Vec<u8>, Shape), String> {
    let (w, h) = declared(input).ok_or("it is not a PNG or JPEG image RichDoctor can read")?;
    if w as u64 * h as u64 > MAX_PIXELS {
        return Err("it is larger than 40 megapixels".into());
    }
    let out = if input.starts_with(PNG_SIGNATURE) { repng(input)? } else { rejpeg(input)? };
    let shape = structure(&out).map_err(|e| format!("it cannot be sent as it is ({e})"))?;
    Ok((out, shape))
}

fn repng(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(input));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    decoder.set_ignore_text_chunk(true);
    let mut reader = decoder.read_info().map_err(|e| format!("it could not be read ({e})"))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).map_err(|e| format!("it could not be read ({e})"))?;
    let data = &buf[..frame.buffer_size()];
    let (color, pixels): (png::ColorType, Vec<u8>) = match frame.color_type {
        png::ColorType::Rgba => (png::ColorType::Rgba, data.to_vec()),
        png::ColorType::Rgb => (png::ColorType::Rgb, data.to_vec()),
        png::ColorType::GrayscaleAlpha => (png::ColorType::Rgba, data.chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect()),
        png::ColorType::Grayscale => (png::ColorType::Rgb, data.iter().flat_map(|&g| [g, g, g]).collect()),
        png::ColorType::Indexed => return Err("it uses a palette RichDoctor does not read".into()),
    };
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, frame.width, frame.height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Best);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&pixels).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

fn rejpeg(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = vec![0xFF, 0xD8];
    let mut at = 2;
    let mut jfif = false;
    loop {
        if at + 2 > input.len() || input[at] != 0xFF {
            return Err("it is damaged".into());
        }
        let m = input[at + 1];
        if m == 0xD9 {
            out.extend_from_slice(&[0xFF, 0xD9]);
            return Ok(out);
        }
        if m == 0xFF {
            at += 1;
            continue;
        }
        if at + 4 > input.len() {
            return Err("it is damaged".into());
        }
        let len = u16::from_be_bytes([input[at + 2], input[at + 3]]) as usize;
        let end = at + 2 + len;
        if len < 2 || end > input.len() {
            return Err("it is damaged".into());
        }
        let keep = match m {
            0xE0 if !jfif && input[at + 4..end].starts_with(b"JFIF\0") => {
                jfif = true;
                true
            }
            0xE0..=0xEF | 0xFE => false,
            _ => true,
        };
        if m == 0xDA {
            // The scan and its entropy-coded data, to the next real marker, kept as it is.
            let mut scan = end;
            while scan + 1 < input.len() && !(input[scan] == 0xFF && input[scan + 1] != 0x00 && !(0xD0..=0xD7).contains(&input[scan + 1])) {
                scan += 1;
            }
            out.extend_from_slice(&input[at..scan]);
            at = scan;
            continue;
        }
        if keep {
            out.extend_from_slice(&input[at..end]);
        }
        at = end;
    }
}

/// One approved image as the manifest lists it (the relay contract's field names).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub attachment_id: String,
    pub order: u32,
    pub filename: String,
    pub media_type: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub sha256: String,
}

impl Entry {
    /// The entry for image number `order` (1-based) with these final bytes.
    pub fn of(order: u32, bytes: &[u8]) -> Result<Entry, String> {
        let s = check(bytes)?;
        Ok(Entry {
            attachment_id: order.to_string(),
            order,
            filename: format!("screenshot-{order}.{}", s.kind.extension()),
            media_type: s.kind.media_type().into(),
            width: s.width,
            height: s.height,
            bytes: bytes.len() as u64,
            sha256: crate::util::sha256_hex(bytes),
        })
    }
}

/// The set's limits: count and total.
pub fn check_set(entries: &[Entry]) -> Result<(), String> {
    if entries.len() > MAX_IMAGES {
        return Err(format!("more than {MAX_IMAGES} screenshots"));
    }
    if entries.iter().map(|e| e.bytes).sum::<u64>() > MAX_TOTAL_BYTES {
        return Err("more than 25 MiB of screenshots together".into());
    }
    Ok(())
}

/// The manifest as the content hash covers it: a JSON array of objects with sorted keys and no
/// spaces, `[]` when there are no images.
pub fn canonical(entries: &[Entry]) -> String {
    let values: Vec<serde_json::Value> = entries.iter().map(|e| serde_json::to_value(e).expect("serializes")).collect();
    // serde_json's Map is a BTreeMap without the preserve_order feature: keys come out sorted.
    serde_json::to_string(&values).expect("serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_with(extra: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        fn chunk(out: &mut Vec<u8>, ty: &[u8; 4], data: &[u8]) {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            out.extend_from_slice(ty);
            out.extend_from_slice(data);
            out.extend_from_slice(&[0, 0, 0, 0]);
        }
        let mut out = PNG_SIGNATURE.to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&3u32.to_be_bytes());
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        chunk(&mut out, b"IHDR", &ihdr);
        for (ty, data) in extra {
            chunk(&mut out, ty, data);
        }
        chunk(&mut out, b"IDAT", b"x");
        chunk(&mut out, b"IEND", b"");
        out
    }

    #[test]
    fn png_structure_is_read_without_decoding_and_only_the_profile_passes() {
        let s = check(&png_with(&[(b"sRGB", b"\0"), (b"pHYs", &[0; 9])])).unwrap();
        assert_eq!((s.kind, s.width, s.height), (Kind::Png, 3, 2));
        for (ty, name) in [(b"tEXt", "tEXt"), (b"eXIf", "eXIf"), (b"iCCP", "iCCP"), (b"tIME", "tIME"), (b"zTXt", "zTXt")] {
            let e = check(&png_with(&[(ty, b"Author\0Jane")])).unwrap_err();
            assert!(e.contains(name), "{e}");
        }
        let mut cut = png_with(&[]);
        cut.truncate(cut.len() - 5);
        assert!(check(&cut).is_err());
    }

    fn jpeg_with(extra: &[(u8, &[u8])]) -> Vec<u8> {
        let mut out = vec![0xFF, 0xD8];
        let seg = |out: &mut Vec<u8>, m: u8, data: &[u8]| {
            out.extend_from_slice(&[0xFF, m]);
            out.extend_from_slice(&((data.len() + 2) as u16).to_be_bytes());
            out.extend_from_slice(data);
        };
        seg(&mut out, 0xE0, b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        for (m, d) in extra {
            seg(&mut out, *m, d);
        }
        seg(&mut out, 0xDB, &[0; 65]);
        seg(&mut out, 0xC0, &[8, 0, 2, 0, 3, 1, 1, 0x11, 0]);
        seg(&mut out, 0xC4, &[0; 17]);
        seg(&mut out, 0xDA, &[1, 1, 0, 0, 63, 0]);
        out.extend_from_slice(&[0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD0, 0x56]);
        out.extend_from_slice(&[0xFF, 0xD9]);
        out
    }

    #[test]
    fn jpeg_structure_is_read_and_only_jfif_app0_is_allowed() {
        let s = check(&jpeg_with(&[])).unwrap();
        assert_eq!((s.kind, s.width, s.height), (Kind::Jpeg, 3, 2));
        assert!(check(&jpeg_with(&[(0xE1, b"Exif\0\0data")])).unwrap_err().contains("APP1"));
        assert!(check(&jpeg_with(&[(0xE2, b"ICC_PROFILE\0")])).unwrap_err().contains("APP2"));
        assert!(check(&jpeg_with(&[(0xFE, b"made by Jane")])).unwrap_err().contains("comment"));
    }

    fn real_png_with_metadata() -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, 2, 1);
            e.set_color(png::ColorType::Rgb);
            e.set_depth(png::BitDepth::Eight);
            e.add_text_chunk("Author".into(), "Jane Doe secret-author".into()).unwrap();
            let mut w = e.write_header().unwrap();
            w.write_image_data(&[255, 0, 0, 0, 0, 0]).unwrap();
        }
        out
    }

    #[test]
    fn finalizing_keeps_the_pixels_and_drops_every_metadata_chunk_and_segment() {
        let input = real_png_with_metadata();
        assert!(check(&input).is_err(), "the original is outside the profile");
        let (out, shape) = finalize(&input).unwrap();
        assert_eq!((shape.width, shape.height), (2, 1));
        assert!(!out.windows(6).any(|w| w == b"secret"), "no metadata survives");
        assert!(check(&out).is_ok());
        let mut d = png::Decoder::new(std::io::Cursor::new(&out)).read_info().unwrap();
        let mut px = vec![0; d.output_buffer_size()];
        d.next_frame(&mut px).unwrap();
        assert_eq!(&px[..6], &[255, 0, 0, 0, 0, 0], "the pixels are the approved ones");
        let jpeg = jpeg_with(&[(0xE1, b"Exif\0\0GPS 51.5N"), (0xFE, b"made by Jane")]);
        let (out, shape) = finalize(&jpeg).unwrap();
        assert_eq!((shape.kind, shape.width, shape.height), (Kind::Jpeg, 3, 2));
        assert!(!out.windows(4).any(|w| w == b"Exif" || w == b"Jane"));
        assert!(check(&out).is_ok());
        assert_eq!(finalize(&out).unwrap().0, out, "a final image finalizes to itself");
        let mut huge = png_with(&[]);
        huge[16..20].copy_from_slice(&10_000u32.to_be_bytes());
        huge[20..24].copy_from_slice(&10_000u32.to_be_bytes());
        assert!(finalize(&huge).unwrap_err().contains("40 megapixels"), "refused before any decode");
    }

    #[test]
    fn the_limits_and_the_manifest() {
        assert!(check(&vec![0u8; (MAX_IMAGE_BYTES + 1) as usize]).unwrap_err().contains("5 MiB"));
        assert!(check(b"GIF89a").is_err());
        let e = Entry::of(1, &png_with(&[])).unwrap();
        assert_eq!(e.filename, "screenshot-1.png");
        let c = canonical(&[e]);
        assert!(c.starts_with("[{\"attachment_id\":\"1\",\"bytes\":"), "{c}");
        assert_eq!(canonical(&[]), "[]");
        let many: Vec<Entry> = (1..=11).map(|i| Entry::of(i, &png_with(&[])).unwrap()).collect();
        assert!(check_set(&many).is_err());
    }
}
