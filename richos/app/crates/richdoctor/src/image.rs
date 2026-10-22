//! The menu bar item's picture: the round 38 v3 template image's pixels. (A screenshot's final
//! bytes are made in the core, `richdoctor_core::image_profile::finalize`.)
use std::io::Cursor;

/// The pixels of a PNG as RGBA.
pub fn rgba(png_bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let mut decoder = png::Decoder::new(Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..frame.buffer_size()];
    let px: Vec<u8> = match frame.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => data.chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some((px, frame.width, frame.height))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_menu_bar_template_image_decodes_at_its_two_scale_size() {
        let (px, w, h) = super::rgba(include_bytes!("../icons/menubar@2x.png")).unwrap();
        assert_eq!((w, h), (36, 36));
        assert_eq!(px.len(), 36 * 36 * 4);
        assert!(px.chunks(4).any(|p| p[3] > 0), "the glyph is drawn");
    }
}
