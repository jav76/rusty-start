//! Win32 desktop snapshot capture using a reusable GDI DIB section.
//!
//! Captures the full monitor (`rcMonitor`, including the taskbar) containing the mouse cursor right
//! before the Start menu window is shown, so the custom GPU shader can progressively fog the
//! background and refract desktop pixels through the Liquid-Glass Card without `IDXGIOutputDuplication`
//! exclusive-lock conflicts.

use std::ffi::c_void;
use std::ptr;
use std::slice;

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDC, GetMonitorInfoW, HBITMAP, HDC, HGDIOBJ, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MonitorFromPoint, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use super::{MonitorRect, MonitorSnapshot};

/// Reuses a GDI memory DC and 32-bpp DIB section across menu toggles whenever the monitor size
/// stays the same.
#[derive(Default)]
pub struct Capturer {
    surface: Option<DibSurface>,
}

impl Capturer {
    pub fn capture_active_monitor(&mut self) -> Result<MonitorSnapshot> {
        let rect: MonitorRect = active_monitor_rect()?;

        let needs_recreate: bool = self
            .surface
            .as_ref()
            .is_none_or(|surface| surface.width != rect.width || surface.height != rect.height);

        if needs_recreate {
            self.surface = Some(DibSurface::create(rect.width, rect.height)?);
        }

        let surface: &DibSurface = self.surface.as_ref().expect("surface initialized above");
        let screen_dc = ScreenDc::acquire()?;

        // SAFETY: `surface.memory_dc` has `surface.bitmap` selected into it with dimensions
        // `(rect.width, rect.height)`, and `screen_dc.dc` is a valid virtual-screen DC.
        unsafe {
            BitBlt(
                surface.memory_dc,
                0,
                0,
                rect.width as i32,
                rect.height as i32,
                Some(screen_dc.dc),
                rect.x,
                rect.y,
                SRCCOPY,
            )
        }
        .context("failed to BitBlt the active monitor desktop")?;

        let byte_len: usize = (rect.width as usize) * (rect.height as usize) * 4;
        // SAFETY: `CreateDIBSection` allocated at least `width * height * 4` bytes at `surface.bits`,
        // and the `BitBlt` above has completed writing to the buffer on this thread.
        let mut rgba_bottom_up: Vec<u8> = unsafe { slice::from_raw_parts(surface.bits, byte_len) }.to_vec();
        for pixel in rgba_bottom_up.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }

        Ok(MonitorSnapshot { rect, rgba_bottom_up })
    }
}

fn active_monitor_rect() -> Result<MonitorRect> {
    let mut cursor = POINT::default();
    // SAFETY: `cursor` is a valid writable `POINT`. If `GetCursorPos` fails (for example on a locked
    // desktop), `(0, 0)` still resolves to the primary monitor via `MONITOR_DEFAULTTONEAREST`.
    let _ = unsafe { GetCursorPos(&mut cursor) };

    // SAFETY: `MonitorFromPoint` has no memory-safety preconditions.
    let monitor: HMONITOR = unsafe { MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_invalid() {
        bail!("MonitorFromPoint returned an invalid monitor handle");
    }

    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    // SAFETY: `info` is initialized with `cbSize` set to `size_of::<MONITORINFO>()`.
    unsafe { GetMonitorInfoW(monitor, &mut info) }.ok().context("failed to query monitor bounds")?;

    let rc: RECT = info.rcMonitor;
    let width: u32 = u32::try_from(rc.right - rc.left).context("monitor width was not positive")?;
    let height: u32 = u32::try_from(rc.bottom - rc.top).context("monitor height was not positive")?;
    if width == 0 || height == 0 {
        bail!("monitor dimensions were zero ({width}x{height})");
    }

    Ok(MonitorRect { x: rc.left, y: rc.top, width, height })
}

struct ScreenDc {
    dc: HDC,
}

impl ScreenDc {
    fn acquire() -> Result<Self> {
        // SAFETY: `None` requests the device context for the entire virtual screen.
        let dc: HDC = unsafe { GetDC(None) };
        if dc.is_invalid() {
            bail!("GetDC(None) failed to acquire the virtual screen device context");
        }

        Ok(Self { dc })
    }
}

impl Drop for ScreenDc {
    fn drop(&mut self) {
        // SAFETY: `self.dc` was acquired from `GetDC(None)` and has not been released yet.
        let _ = unsafe { ReleaseDC(None, self.dc) };
    }
}

struct DibSurface {
    memory_dc: HDC,
    bitmap: HBITMAP,
    previous_object: HGDIOBJ,
    bits: *mut u8,
    width: u32,
    height: u32,
}

impl DibSurface {
    fn create(width: u32, height: u32) -> Result<Self> {
        // SAFETY: `None` creates a memory DC compatible with the application's current screen.
        let memory_dc: HDC = unsafe { CreateCompatibleDC(None) };
        if memory_dc.is_invalid() {
            bail!("CreateCompatibleDC failed");
        }

        // A positive `biHeight` creates a bottom-up DIB whose row order matches OpenGL's bottom-left
        // texture origin, avoiding a vertical flip during upload or sampling.
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: height as i32,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut raw_bits: *mut c_void = ptr::null_mut();
        // SAFETY: `memory_dc` is valid, `info` is a valid `BITMAPINFO`, and `raw_bits` receives the
        // GDI-managed pixel buffer pointer.
        let bitmap: HBITMAP =
            match unsafe { CreateDIBSection(Some(memory_dc), &info, DIB_RGB_COLORS, &mut raw_bits, None, 0) } {
                Ok(bitmap) if !bitmap.is_invalid() && !raw_bits.is_null() => bitmap,
                Ok(_) => {
                    // SAFETY: `memory_dc` was created above and must be cleaned up on error.
                    let _ = unsafe { DeleteDC(memory_dc) };
                    bail!("CreateDIBSection returned a null bitmap or pixel buffer");
                }
                Err(error) => {
                    // SAFETY: `memory_dc` was created above and must be cleaned up on error.
                    let _ = unsafe { DeleteDC(memory_dc) };
                    return Err(error).context("CreateDIBSection failed");
                }
            };

        // SAFETY: both `memory_dc` and `bitmap` are live GDI handles owned by this struct.
        let previous_object: HGDIOBJ = unsafe { SelectObject(memory_dc, HGDIOBJ::from(bitmap)) };
        if previous_object.is_invalid() {
            // SAFETY: cleans up the newly created GDI objects on failure.
            unsafe {
                let _ = DeleteObject(HGDIOBJ::from(bitmap));
                let _ = DeleteDC(memory_dc);
            }
            bail!("SelectObject failed to bind the DIB section");
        }

        Ok(Self { memory_dc, bitmap, previous_object, bits: raw_bits.cast::<u8>(), width, height })
    }
}

impl Drop for DibSurface {
    fn drop(&mut self) {
        // SAFETY: restores the original object before deleting the DIB section and memory DC.
        unsafe {
            let _ = SelectObject(self.memory_dc, self.previous_object);
            let _ = DeleteObject(HGDIOBJ::from(self.bitmap));
            let _ = DeleteDC(self.memory_dc);
        }
    }
}
