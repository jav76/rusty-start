//! Synthetic desktop backdrop for Linux preview builds.
//!
//! Generates a colorful multi-window desktop wallpaper with crisp edges and grid lines so the
//! dual-Kawase fog ramp and SDF bevel refraction are clearly visible without a Win32 desktop.

use anyhow::Result;

use super::{MonitorRect, MonitorSnapshot};

const PREVIEW_WIDTH: u32 = 1440;
const PREVIEW_HEIGHT: u32 = 900;

#[derive(Default)]
pub struct Capturer {
    cached: Option<MonitorSnapshot>,
}

impl Capturer {
    pub fn capture_active_monitor(&mut self) -> Result<MonitorSnapshot> {
        if let Some(snapshot) = &self.cached {
            return Ok(snapshot.clone());
        }

        let rect = MonitorRect { x: 0, y: 0, width: PREVIEW_WIDTH, height: PREVIEW_HEIGHT };
        let mut rgba_bottom_up: Vec<u8> = Vec::with_capacity((PREVIEW_WIDTH * PREVIEW_HEIGHT * 4) as usize);

        for y in 0..PREVIEW_HEIGHT {
            let v: f32 = y as f32 / PREVIEW_HEIGHT as f32;
            for x in 0..PREVIEW_WIDTH {
                let u: f32 = x as f32 / PREVIEW_WIDTH as f32;

                // Deep aurora gradient background with crisp geometric bands for testing refraction.
                let wave: f32 = ((u * 9.0 + v * 5.0).sin() * 0.5 + 0.5) * 0.35;
                let mut r: f32 = 0.07 + 0.24 * (1.0 - v) + 0.32 * wave * u;
                let mut g: f32 = 0.11 + 0.38 * u * (1.0 - 0.4 * v) + 0.22 * wave;
                let mut b: f32 = 0.24 + 0.48 * v + 0.20 * (1.0 - u);

                // Simulated bright desktop window cards behind the menu to showcase bevel bending.
                let in_left_window: bool = (120..760).contains(&x) && (220..740).contains(&y);
                let in_right_window: bool = (620..1280).contains(&x) && (140..620).contains(&y);
                if in_left_window || in_right_window {
                    r = (r * 0.55 + 0.36).min(1.0);
                    g = (g * 0.55 + 0.40).min(1.0);
                    b = (b * 0.55 + 0.44).min(1.0);
                }

                // Subtle 48px reference grid so optical lens displacement at the card rim is obvious.
                if x % 48 == 0 || y % 48 == 0 {
                    r = (r + 0.10).min(1.0);
                    g = (g + 0.12).min(1.0);
                    b = (b + 0.15).min(1.0);
                }

                // Simulated bottom taskbar strip (bottom 48px in bottom-up coordinates).
                if y < 48 {
                    r *= 0.35;
                    g *= 0.38;
                    b *= 0.45;
                }

                rgba_bottom_up.push((r.clamp(0.0, 1.0) * 255.0) as u8);
                rgba_bottom_up.push((g.clamp(0.0, 1.0) * 255.0) as u8);
                rgba_bottom_up.push((b.clamp(0.0, 1.0) * 255.0) as u8);
                rgba_bottom_up.push(255);
            }
        }

        let snapshot = MonitorSnapshot { rect, rgba_bottom_up };
        self.cached = Some(snapshot.clone());
        Ok(snapshot)
    }
}

