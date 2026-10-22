//! **THE SHARED SCREENSHOT FIXTURES** (PRD §7: "The client encoder and Worker verifier share one
//! versioned format profile and golden fixtures").
//!
//! Writes RichDoctor's side of the golden fixtures into `<dir>` (the relay reads them from
//! `richos/mobile/service/report/fixtures/client/`):
//!
//!   - `accept-*.png`, `accept-*.jpg`: what RichDoctor's finalize step actually produces, which
//!     the relay must accept unchanged;
//!   - `reject-*.png`, `reject-*.jpg`: originals carrying metadata outside the profile (a PNG text
//!     chunk, an EXIF APP1 and a comment in a JPEG), which the relay must refuse.
//!
//! The JPEG source is made by macOS's own `sips` from the PNG, then given an EXIF segment and a
//! comment, so the fixtures are real files and not hand-built structure.
//!
//! ```sh
//! cargo run --offline --manifest-path richos/app/crates/richdoctor-core/Cargo.toml --example profile_fixtures -- richos/mobile/service/report/fixtures/client
//! ```
use richdoctor_core::image_profile::{check, finalize};
use std::path::Path;

fn gradient_png_with_text() -> Vec<u8> {
    let (w, h) = (96u32, 48u32);
    let mut px = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            px.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, 128]);
        }
    }
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        e.add_text_chunk("Author".into(), "Fixture Person".into()).unwrap();
        let mut writer = e.write_header().unwrap();
        writer.write_image_data(&px).unwrap();
    }
    out
}

/// A JPEG with an EXIF APP1 and a comment inserted after SOI.
fn with_metadata(jpeg: &[u8]) -> Vec<u8> {
    let mut out = vec![0xFF, 0xD8];
    let exif = b"Exif\0\0MM\0*\0\0\0\x08\0\0";
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(exif);
    let com = b"made by Fixture Person";
    out.extend_from_slice(&[0xFF, 0xFE]);
    out.extend_from_slice(&((com.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(com);
    out.extend_from_slice(&jpeg[2..]);
    out
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: profile_fixtures <dir>"));
    std::fs::create_dir_all(&dir).unwrap();
    let write = |name: &str, bytes: &[u8]| {
        std::fs::write(dir.join(name), bytes).unwrap();
        println!("{name}: {} bytes, {}", bytes.len(), match check(bytes) { Ok(s) => format!("accepted {}x{}", s.width, s.height), Err(e) => format!("refused ({e})") });
    };
    let original = gradient_png_with_text();
    assert!(check(&original).is_err());
    write("reject-text-chunk.png", &original);
    let (png_final, _) = finalize(&original).unwrap();
    write("accept-final.png", &png_final);

    let scratch = std::env::temp_dir().join(format!("rd-fixtures-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let src = scratch.join("src.png");
    std::fs::write(&src, &png_final).unwrap();
    let jpg = scratch.join("src.jpg");
    let ok = std::process::Command::new("/usr/bin/sips").args(["-s", "format", "jpeg", "-s", "formatOptions", "90"]).arg(&src).arg("--out").arg(&jpg).output().unwrap();
    assert!(ok.status.success(), "{}", String::from_utf8_lossy(&ok.stderr));
    let sips_jpeg = std::fs::read(&jpg).unwrap();
    let tagged = with_metadata(&sips_jpeg);
    assert!(check(&tagged).is_err());
    write("reject-exif-and-comment.jpg", &tagged);
    let (jpeg_final, _) = finalize(&tagged).unwrap();
    write("accept-final.jpg", &jpeg_final);
    std::fs::remove_dir_all(&scratch).ok();
    let _ = Path::new(".");
}
