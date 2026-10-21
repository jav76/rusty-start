//! Wayland overlay window for KDE Plasma (KWin), and other compositors that honor fullscreen
//! requests.
//!
//! Wayland clients cannot position their windows, so the overlay asks for fullscreen on the output
//! the compositor picks, which is the active one on KWin. Slint destroys the window on every hide
//! under Wayland and rebuilds its winit attributes without the backend's attributes hook, so
//! fullscreen is requested again before every show through Slint's own window properties.
//!
//! The backdrop comes from KWin's `org.kde.KWin.ScreenShot2` interface, which only answers apps
//! whose installed `.desktop` file lists it under `X-KDE-DBUS-Restricted-Interfaces`. Without it,
//! or outside KWin, the menu opens as Plain Glass.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::os::fd::{AsFd, OwnedFd};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

use anyhow::{Context, Result, bail};
use rusty_start_protocol::APP_ID;
use slint::winit_030::WinitWindowAccessor;
use slint::winit_030::winit::platform::startup_notify::WindowAttributesExtStartupNotify;
use slint::winit_030::winit::window::{ActivationToken, Fullscreen};
use zbus::zvariant::{Fd, OwnedValue, Value};

use super::{CaptureUnavailable, MonitorRect, MonitorSnapshot, kwin_image, plasma_shell};
use crate::dbus;

/// Layout size until the compositor reports the fullscreen size.
const PLACEHOLDER_RECT: MonitorRect = MonitorRect { x: 0, y: 0, width: 1920, height: 1080 };

/// The compositor sizes the fullscreen overlay, so the layout follows the window's size.
pub const OVERLAY_FOLLOWS_WINDOW_SIZE: bool = true;

/// Size used for layout until the compositor reports the fullscreen size.
#[expect(clippy::unnecessary_wraps, reason = "signature matches the fallible win32 backend")]
pub fn active_monitor_rect() -> Result<MonitorRect> {
    Ok(PLACEHOLDER_RECT)
}

/// Placeholder so the host's startup code is identical on every platform.
pub struct OverlayOwner;

/// Selects the winit backend and sets the app ID that KWin rules and the `.desktop` file match.
/// Must run on the UI thread before the first Slint component is created.
pub fn install_backend() -> Result<OverlayOwner> {
    // A launcher that started the host (a shortcut, the session's autostart) hands over an
    // activation token, which lets the first window take keyboard focus. Later windows rely on
    // the compositor's focus policy, because Slint rebuilds their attributes without this hook.
    let startup_token: Option<String> = std::env::var("XDG_ACTIVATION_TOKEN").ok().filter(|token| !token.is_empty());

    slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_window_attributes_hook(move |attributes| match &startup_token {
            Some(token) => attributes.with_activation_token(ActivationToken::from_raw(token.clone())),
            None => attributes,
        })
        .select()
        .context("failed to select the winit backend")?;
    slint::set_xdg_app_id(APP_ID).context("failed to set the Wayland app ID")?;

    Ok(OverlayOwner)
}

/// Makes the overlay fullscreen. The compositor picks the size, so `rect` is ignored.
///
/// After a hide there is no window, and toggling Slint's fullscreen property off and on copies the
/// state into the attributes Slint rebuilt for the next one. The session's first window already
/// exists when the menu first opens, because Slint creates it at startup; toggling the property
/// then reaches the compositor as `unset_fullscreen`, so that window is asked through winit.
pub fn place_overlay_window(window: &slint::Window, _rect: MonitorRect) {
    let has_window: bool = window
        .with_winit_window(|winit_window| {
            if winit_window.fullscreen().is_none() {
                winit_window.set_fullscreen(Some(Fullscreen::Borderless(None)));
            }
        })
        .is_some();
    if !has_window {
        window.set_fullscreen(false);
        window.set_fullscreen(true);
    }
}

/// Requests fullscreen again when the compositor reports the overlay out of fullscreen. Returns
/// whether the overlay is fullscreen, so callers skip laying out for a transient windowed size.
pub fn keep_overlay_fullscreen(window: &slint::Window) -> bool {
    window
        .with_winit_window(|winit_window| {
            if winit_window.fullscreen().is_some() {
                return true;
            }
            winit_window.set_fullscreen(Some(Fullscreen::Borderless(None)));
            false
        })
        .unwrap_or(false)
}

pub fn configure_overlay_window() {}

/// Gives a new overlay window the Plasma applet popup role, so KWin does not animate it like an
/// opening window. Runs when Slint sets up rendering for the window, before its first frame.
pub fn prepare_overlay_surface(window: &slint::Window) {
    window.with_winit_window(plasma_shell::set_applet_popup_role);
}

/// The compositor decides focus on Wayland. The menu learns the outcome from focus events.
pub fn focus_overlay_window() -> bool {
    true
}

/// Captures the active screen through KWin on the `rusty-start-capture` thread, so the menu can
/// show its window while the capture runs.
#[derive(Default)]
pub struct Capturer {
    worker: Option<CaptureWorker>,
    /// Sequence number of the capture `poll_capture` waits for. Results of older captures, which
    /// arrive after their open already went Plain Glass, are discarded.
    awaited: Option<u64>,
    next_sequence: u64,
    /// Set after KWin refused or lacked the interface. Capture is not retried this session.
    unavailable: bool,
    failed_early: Option<anyhow::Error>,
}

struct CaptureWorker {
    requests: Sender<u64>,
    results: Receiver<(u64, Result<MonitorSnapshot, CaptureError>)>,
}

#[derive(Debug)]
enum CaptureError {
    /// Retrying cannot succeed this session: no KWin, or this executable is not authorized.
    Unavailable(String),
    Failed(anyhow::Error),
}

impl Capturer {
    /// Starts capturing the active screen. The result arrives through `poll_capture`.
    pub fn start_capture(&mut self) {
        if self.unavailable {
            self.failed_early = Some(anyhow::Error::new(CaptureUnavailable));
            return;
        }
        let worker: &CaptureWorker = match &mut self.worker {
            Some(worker) => worker,
            empty @ None => match CaptureWorker::spawn() {
                Ok(worker) => empty.insert(worker),
                Err(error) => {
                    self.failed_early = Some(error);
                    return;
                }
            },
        };

        let sequence: u64 = self.next_sequence;
        self.next_sequence += 1;
        if worker.requests.send(sequence).is_err() {
            self.worker = None;
            self.failed_early = Some(anyhow::anyhow!("the capture thread stopped"));
            return;
        }
        self.awaited = Some(sequence);
    }

    /// Returns the result of the last `start_capture` once it is ready.
    pub fn poll_capture(&mut self) -> Option<Result<MonitorSnapshot>> {
        if let Some(error) = self.failed_early.take() {
            return Some(Err(error));
        }
        let awaited: u64 = self.awaited?;
        loop {
            let (sequence, result) = match self.worker.as_ref()?.results.try_recv() {
                Ok(received) => received,
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.awaited = None;
                    return Some(Err(anyhow::anyhow!("the capture thread stopped")));
                }
            };
            if sequence != awaited {
                continue;
            }
            self.awaited = None;
            return Some(result.map_err(|error| match error {
                CaptureError::Unavailable(reason) => {
                    eprintln!("the menu opens as Plain Glass this session: {reason}");
                    self.unavailable = true;
                    anyhow::Error::new(CaptureUnavailable)
                }
                CaptureError::Failed(error) => error,
            }));
        }
    }
}

impl CaptureWorker {
    fn spawn() -> Result<Self> {
        let (request_sender, request_receiver) = mpsc::channel::<u64>();
        let (result_sender, result_receiver) = mpsc::channel();
        thread::Builder::new()
            .name("rusty-start-capture".into())
            .spawn(move || {
                let trace: bool = std::env::var_os("RUSTY_START_TRACE_OPEN").is_some();
                for sequence in request_receiver {
                    let started_at = std::time::Instant::now();
                    let result = capture_active_screen();
                    if trace && let Ok(snapshot) = &result {
                        let (width, height) = (snapshot.rect.width, snapshot.rect.height);
                        eprintln!("captured {width}x{height} in {} ms", started_at.elapsed().as_millis());
                    }
                    if result_sender.send((sequence, result)).is_err() {
                        return;
                    }
                }
            })
            .context("failed to spawn the capture thread")?;
        Ok(Self { requests: request_sender, results: result_receiver })
    }
}

fn capture_active_screen() -> Result<MonitorSnapshot, CaptureError> {
    let connection = dbus::session().map_err(CaptureError::Failed)?;
    let (pipe_reader, pipe_writer): (OwnedFd, OwnedFd) = rustix::pipe::pipe_with(rustix::pipe::PipeFlags::CLOEXEC)
        .map_err(|error| {
            CaptureError::Failed(anyhow::Error::from(error).context("failed to create the screenshot pipe"))
        })?;
    let options: HashMap<&str, Value<'_>> =
        HashMap::from([("native-resolution", Value::from(true)), ("include-cursor", Value::from(false))]);

    let reply = connection.call_method(
        Some("org.kde.KWin"),
        "/org/kde/KWin/ScreenShot2",
        Some("org.kde.KWin.ScreenShot2"),
        "CaptureActiveScreen",
        &(options, Fd::from(pipe_writer.as_fd())),
    );
    // KWin writes through its own copy of the pipe; dropping ours lets the read end see EOF.
    drop(pipe_writer);
    let reply = reply.map_err(classify_call_error)?;

    let results: HashMap<String, OwnedValue> = reply
        .body()
        .deserialize()
        .map_err(|error| CaptureError::Failed(anyhow::Error::from(error).context("unexpected ScreenShot2 reply")))?;
    read_snapshot(&results, pipe_reader).map_err(CaptureError::Failed)
}

fn classify_call_error(error: zbus::Error) -> CaptureError {
    let name: Option<String> = match &error {
        zbus::Error::MethodError(name, _, _) => Some(name.to_string()),
        zbus::Error::FDO(fdo) => Some(format!("{fdo}")),
        _ => None,
    };
    match name.as_deref() {
        Some(name) if name.contains("NoAuthorized") => CaptureError::Unavailable(format!(
            "KWin refused the screenshot because this executable has no authorized desktop entry ({name})"
        )),
        Some(name)
            if name.contains("ServiceUnknown") || name.contains("UnknownMethod") || name.contains("UnknownObject") =>
        {
            CaptureError::Unavailable(format!("KWin's ScreenShot2 interface is not available ({name})"))
        }
        _ => CaptureError::Failed(anyhow::Error::from(error).context("KWin ScreenShot2 call failed")),
    }
}

fn read_snapshot(results: &HashMap<String, OwnedValue>, pipe_reader: OwnedFd) -> Result<MonitorSnapshot> {
    let number = |key: &str| -> Result<u32> {
        let value: &OwnedValue = results.get(key).with_context(|| format!("the reply has no {key}"))?;
        u32::try_from(value).with_context(|| format!("{key} is not a u32"))
    };
    let kind: String = results.get("type").and_then(|value| String::try_from(value.clone()).ok()).unwrap_or_default();
    if kind != "raw" {
        bail!("unsupported screenshot type {kind:?}");
    }
    let (width, height, stride, format) = (number("width")?, number("height")?, number("stride")?, number("format")?);

    let mut data: Vec<u8> = Vec::with_capacity(stride as usize * height as usize);
    File::from(pipe_reader).read_to_end(&mut data).context("failed to read the screenshot")?;
    let rgba_bottom_up: Vec<u8> = kwin_image::to_rgba_bottom_up(&data, width, height, stride, format)?;

    Ok(MonitorSnapshot { rect: MonitorRect { x: 0, y: 0, width, height }, rgba_bottom_up })
}
