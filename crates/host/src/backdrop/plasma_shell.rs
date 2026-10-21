//! Gives each overlay window the `appletpopup` role of KDE's `org_kde_plasma_shell` Wayland
//! protocol, the role of Plasma's own launcher popups.
//!
//! KWin's Scale effect grows every new normal window from a smaller size. The overlay is a new
//! window on every open, so the Fog Veil looked like a window opening instead of covering the
//! screen at once. Applet popups are left to the Fading Popups effect, which only fades them in.
//! The role must reach KWin before the window's first frame. Compositors without the protocol
//! keep the default role.
//!
//! Everything here runs on the UI thread, on winit's own Wayland connection.

use std::cell::RefCell;
use std::mem::ManuallyDrop;

use anyhow::{Context, Result};
use slint::winit_030::winit::raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use slint::winit_030::winit::window::Window;
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop};
use wayland_protocols_plasma::plasma_shell::client::org_kde_plasma_shell::OrgKdePlasmaShell;
use wayland_protocols_plasma::plasma_shell::client::org_kde_plasma_surface::{OrgKdePlasmaSurface, Role};

/// First `org_kde_plasma_shell` version with the `appletpopup` role.
const APPLET_POPUP_VERSION: u32 = 8;

enum Probe {
    NotProbed,
    Unavailable,
    /// Never dropped: the `wl_display` belongs to winit's event loop, which may already be gone
    /// when thread-locals are destroyed at exit.
    Available(ManuallyDrop<PlasmaShell>),
}

thread_local! {
    static PLASMA_SHELL: RefCell<Probe> = const { RefCell::new(Probe::NotProbed) };
}

/// Sets the role on `window`'s surface. Must run before the window's first frame is committed.
pub fn set_applet_popup_role(window: &Window) {
    PLASMA_SHELL.with(|cell| {
        let mut probe = cell.borrow_mut();
        if matches!(*probe, Probe::NotProbed) {
            *probe = match PlasmaShell::connect(window) {
                Ok(Some(shell)) => Probe::Available(ManuallyDrop::new(shell)),
                Ok(None) => Probe::Unavailable,
                Err(error) => {
                    eprintln!("failed to connect to the Plasma shell protocol: {error:#}");
                    Probe::Unavailable
                }
            };
        }
        if let Probe::Available(shell) = &mut *probe
            && let Err(error) = shell.set_role(window)
        {
            eprintln!("failed to give the menu window the applet popup role: {error:#}");
        }
    });
}

struct PlasmaShell {
    connection: Connection,
    queue: EventQueue<Events>,
    shell: OrgKdePlasmaShell,
    /// Role object of the current overlay window. Destroying it would reset the role, so it lives
    /// until the next window replaces it.
    surface: Option<OrgKdePlasmaSurface>,
}

impl PlasmaShell {
    /// Returns `None` outside Wayland or when the compositor does not offer the protocol.
    fn connect(window: &Window) -> Result<Option<Self>> {
        let RawDisplayHandle::Wayland(display) =
            window.display_handle().context("failed to get the display handle")?.as_raw()
        else {
            return Ok(None);
        };
        // SAFETY: the `wl_display` belongs to winit's event loop, which runs until the host exits,
        // and this backend is never dropped (see `Probe::Available`).
        let backend: Backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let connection: Connection = Connection::from_backend(backend);
        let (globals, queue) =
            registry_queue_init::<Events>(&connection).context("failed to list the Wayland globals")?;
        let Ok(shell) =
            globals.bind::<OrgKdePlasmaShell, _, _>(&queue.handle(), APPLET_POPUP_VERSION..=APPLET_POPUP_VERSION, ())
        else {
            return Ok(None);
        };
        Ok(Some(Self { connection, queue, shell, surface: None }))
    }

    fn set_role(&mut self, window: &Window) -> Result<()> {
        let RawWindowHandle::Wayland(handle) =
            window.window_handle().context("failed to get the window handle")?.as_raw()
        else {
            return Ok(());
        };
        // SAFETY: winit created this `wl_surface` on the display this connection wraps and keeps it
        // alive while `window` exists. The id is only used within this call.
        let surface_id: ObjectId =
            unsafe { ObjectId::from_ptr(WlSurface::interface(), handle.surface.as_ptr().cast()) }
                .context("the window's surface is not a wl_surface")?;
        let surface: WlSurface =
            WlSurface::from_id(&self.connection, surface_id).context("failed to wrap the window's surface")?;

        if let Some(previous) = self.surface.take() {
            previous.destroy();
        }
        let plasma_surface: OrgKdePlasmaSurface = self.shell.get_surface(&surface, &self.queue.handle(), ());
        plasma_surface.set_role(Role::Appletpopup.into());
        self.surface = Some(plasma_surface);

        // Registry events (outputs coming and going) queue up here otherwise.
        self.queue.dispatch_pending(&mut Events).context("failed to drain the Plasma shell events")?;
        self.connection.flush().context("failed to send the role to the compositor")?;
        Ok(())
    }
}

/// Event sink for the private queue. The overlay needs none of these events.
struct Events;

impl Dispatch<WlRegistry, GlobalListContents> for Events {
    fn event(
        _state: &mut Self,
        _registry: &WlRegistry,
        _event: wl_registry::Event,
        _contents: &GlobalListContents,
        _connection: &Connection,
        _queue: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(Events: OrgKdePlasmaShell);
delegate_noop!(Events: ignore OrgKdePlasmaSurface);
