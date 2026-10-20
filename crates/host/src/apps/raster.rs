//! Pixel helpers for icons decoded from files: premultiplying alpha and fitting an image into the
//! square icon tile.

/// Converts straight-alpha RGBA to premultiplied RGBA in place.
pub fn premultiply(rgba: &mut [u8]) {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha: u16 = u16::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
}

/// Scales premultiplied RGBA of `width`x`height` to fit an `edge`x`edge` tile, keeping the aspect
/// ratio and centering it on transparent pixels. Each target pixel averages the source pixels it
/// covers, so shrinking stays smooth; enlarging repeats pixels.
pub fn fit_square(rgba: &[u8], width: u32, height: u32, edge: u32) -> Vec<u8> {
    let mut tile: Vec<u8> = vec![0; (edge * edge * 4) as usize];
    if width == 0 || height == 0 || edge == 0 || rgba.len() < (width * height * 4) as usize {
        return tile;
    }

    let longest: u32 = width.max(height);
    let fitted_width: u32 = (width * edge / longest).max(1);
    let fitted_height: u32 = (height * edge / longest).max(1);
    let offset_x: u32 = (edge - fitted_width) / 2;
    let offset_y: u32 = (edge - fitted_height) / 2;

    for target_y in 0..fitted_height {
        let source_top: u32 = target_y * height / fitted_height;
        let source_bottom: u32 = ((target_y + 1) * height / fitted_height).max(source_top + 1);
        for target_x in 0..fitted_width {
            let source_left: u32 = target_x * width / fitted_width;
            let source_right: u32 = ((target_x + 1) * width / fitted_width).max(source_left + 1);

            let mut sums: [u32; 4] = [0; 4];
            for source_y in source_top..source_bottom {
                for source_x in source_left..source_right {
                    let start: usize = ((source_y * width + source_x) * 4) as usize;
                    for (sum, &value) in sums.iter_mut().zip(&rgba[start..start + 4]) {
                        *sum += u32::from(value);
                    }
                }
            }

            let count: u32 = (source_bottom - source_top) * (source_right - source_left);
            let start: usize = (((target_y + offset_y) * edge + target_x + offset_x) * 4) as usize;
            for (target, sum) in tile[start..start + 4].iter_mut().zip(sums) {
                *target = ((sum + count / 2) / count) as u8;
            }
        }
    }
    tile
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiplies_color_by_alpha() {
        let mut pixels: [u8; 8] = [255, 128, 0, 128, 10, 20, 30, 255];
        premultiply(&mut pixels);

        assert_eq!(pixels, [128, 64, 0, 128, 10, 20, 30, 255]);
    }

    #[test]
    fn shrinking_averages_the_covered_pixels() {
        // A 2x2 checkerboard of opaque white and transparent black.
        let source: [u8; 16] = [255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255];

        assert_eq!(fit_square(&source, 2, 2, 1), [128, 128, 128, 128]);
    }

    #[test]
    fn wide_images_are_centered_vertically() {
        let source: Vec<u8> = vec![255; 4 * 2 * 4];
        let tile: Vec<u8> = fit_square(&source, 4, 2, 4);
        let alpha_at = |x: usize, y: usize| tile[(y * 4 + x) * 4 + 3];

        assert_eq!((alpha_at(0, 0), alpha_at(0, 1), alpha_at(3, 2), alpha_at(0, 3)), (0, 255, 255, 0));
    }

    #[test]
    fn invalid_input_yields_a_transparent_tile() {
        assert_eq!(fit_square(&[], 4, 4, 2), [0; 16]);
    }
}
