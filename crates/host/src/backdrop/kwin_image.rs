//! Converts the raw image KWin's ScreenShot2 interface writes to its pipe into the bottom-up RGBA
//! layout the glass renderer uploads.

use anyhow::{Result, bail};

/// Byte order of one pixel in memory for the `QImage::Format` values KWin sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ByteOrder {
    /// `Format_RGB32`, `Format_ARGB32` and `Format_ARGB32_Premultiplied` on little-endian hosts.
    Bgra,
    /// `Format_RGBX8888`, `Format_RGBA8888` and `Format_RGBA8888_Premultiplied`.
    Rgba,
}

fn byte_order(qimage_format: u32) -> Option<ByteOrder> {
    match qimage_format {
        4..=6 if cfg!(target_endian = "little") => Some(ByteOrder::Bgra),
        16..=18 => Some(ByteOrder::Rgba),
        _ => None,
    }
}

/// Converts a top-down image with `stride` bytes per row into bottom-up RGBA. The desktop is
/// opaque, so alpha is set to 255 as on Windows.
pub fn to_rgba_bottom_up(data: &[u8], width: u32, height: u32, stride: u32, qimage_format: u32) -> Result<Vec<u8>> {
    let Some(order) = byte_order(qimage_format) else {
        bail!("unsupported screenshot pixel format {qimage_format}");
    };
    let row_bytes: usize = width as usize * 4;
    let stride: usize = stride as usize;
    if width == 0 || height == 0 || stride < row_bytes || data.len() < stride * (height as usize - 1) + row_bytes {
        bail!("the screenshot is smaller than its {width}x{height} header");
    }

    let mut rgba: Vec<u8> = Vec::with_capacity(row_bytes * height as usize);
    for row in data.chunks(stride).take(height as usize).rev() {
        for pixel in row[..row_bytes].as_chunks::<4>().0 {
            let [first, second, third, _] = *pixel;
            match order {
                ByteOrder::Bgra => rgba.extend_from_slice(&[third, second, first, 255]),
                ByteOrder::Rgba => rgba.extend_from_slice(&[first, second, third, 255]),
            }
        }
    }
    Ok(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORMAT_ARGB32_PREMULTIPLIED: u32 = 6;
    const FORMAT_RGBA8888: u32 = 17;

    #[test]
    fn flips_rows_and_swaps_bgra_to_opaque_rgba() {
        // 1x2 image: top pixel blue, bottom pixel red, in BGRA byte order.
        let data: [u8; 8] = [255, 0, 0, 255, 0, 0, 255, 128];
        let rgba = to_rgba_bottom_up(&data, 1, 2, 4, FORMAT_ARGB32_PREMULTIPLIED).expect("valid image");

        assert_eq!(rgba, [255, 0, 0, 255, 0, 0, 255, 255]);
    }

    #[test]
    fn skips_row_padding() {
        // 1x2 RGBA image with a 4-byte pad after each row; the last row may omit its padding.
        let data: [u8; 12] = [1, 2, 3, 4, 9, 9, 9, 9, 5, 6, 7, 8];
        let rgba = to_rgba_bottom_up(&data, 1, 2, 8, FORMAT_RGBA8888).expect("valid image");

        assert_eq!(rgba, [5, 6, 7, 255, 1, 2, 3, 255]);
    }

    #[test]
    fn rejects_short_data_and_unknown_formats() {
        assert!(to_rgba_bottom_up(&[0; 7], 1, 2, 4, FORMAT_RGBA8888).is_err());
        assert!(to_rgba_bottom_up(&[0; 8], 1, 2, 4, 3).is_err());
        assert!(to_rgba_bottom_up(&[], 0, 0, 0, FORMAT_RGBA8888).is_err());
    }
}
