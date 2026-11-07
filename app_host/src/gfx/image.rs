//! Bounded PNG/JPEG decoding to the RGBA8 `TextureData` takes. Sizes are checked from the
//! header before any pixel buffer is allocated.

use runtime::scene3d::texture::MAX_TEXTURE_SIDE;

#[derive(Debug)]
pub(crate) struct Decoded {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub(crate) fn check(width: u32, height: u32) -> Result<(), String> {
    if !(1..=MAX_TEXTURE_SIDE).contains(&width) || !(1..=MAX_TEXTURE_SIDE).contains(&height) {
        return Err(format!(
            "image is {width}x{height}; textures are at most {MAX_TEXTURE_SIDE}x{MAX_TEXTURE_SIDE}"
        ));
    }
    Ok(())
}

/// Width and height from the header alone, for budgeting before decoding.
pub(crate) fn size(bytes: &[u8]) -> Result<(u32, u32), String> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .map_err(|e| format!("png: {e}"))?;
        let info = reader.info();
        Ok((info.width, info.height))
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        let mut d = zune_jpeg::JpegDecoder::new(bytes);
        d.decode_headers().map_err(|e| format!("jpeg: {e:?}"))?;
        let (w, h) = d.dimensions().ok_or("jpeg: no dimensions")?;
        Ok((w as u32, h as u32))
    } else {
        Err("image is neither PNG nor JPEG".into())
    }
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    let (width, height) = size(bytes)?;
    check(width, height)?;
    let (rgba, channels) = if bytes.starts_with(b"\x89PNG") {
        let mut d = png::Decoder::new(std::io::Cursor::new(bytes));
        d.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = d.read_info().map_err(|e| format!("png: {e}"))?;
        let mut buf = vec![0; reader.output_buffer_size().ok_or("png: image too large")?];
        let frame = reader.next_frame(&mut buf).map_err(|e| format!("png: {e}"))?;
        buf.truncate(frame.buffer_size());
        let channels = match frame.color_type {
            png::ColorType::Grayscale => 1,
            png::ColorType::GrayscaleAlpha => 2,
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            png::ColorType::Indexed => return Err("png: palette was not expanded".into()),
        };
        (buf, channels)
    } else {
        use zune_jpeg::zune_core::{colorspace::ColorSpace, options::DecoderOptions};
        let options = DecoderOptions::default()
            .jpeg_set_out_colorspace(ColorSpace::RGBA)
            .set_max_width(MAX_TEXTURE_SIDE as usize)
            .set_max_height(MAX_TEXTURE_SIDE as usize);
        let mut d = zune_jpeg::JpegDecoder::new_with_options(bytes, options);
        let buf = d.decode().map_err(|e| format!("jpeg: {e:?}"))?;
        // Greyscale files decode to one channel whatever was asked for.
        let channels = d.get_output_colorspace().map_or(4, |c| c.num_components());
        (buf, channels)
    };
    let pixels = width as usize * height as usize;
    if rgba.len() != pixels * channels {
        return Err("image data is shorter than its header".into());
    }
    let rgba = match channels {
        4 => rgba,
        3 => rgba.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        2 => rgba.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        1 => rgba.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        n => return Err(format!("image has {n} channels")),
    };
    Ok(Decoded { width, height, rgba })
}
