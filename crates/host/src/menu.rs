//! Showing, hiding and resetting the menu, plus driving the `180ms` Optical Condense and `100ms`
//! dismiss animations. Every function here must run on the UI thread.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::{LazyLock, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use slint::winit_030::{EventResult, WinitWindowAccessor, winit};
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::StartMenu;
use crate::app_list::{Activation, AppList};
use crate::apps::{self, App, IconPixels};
use crate::backdrop::{self, MonitorRect};
use crate::config::Config;
use crate::glass::{self, GlassState};
use crate::power::{self, PowerAction};
use crate::trigger::{self, MenuKey, MenuKeyAction, MenuWinKeyTap};
use crate::usage::{self, Usage};

const MATERIALIZE_DURATION: Duration = Duration::from_millis(180);
const DEMATERIALIZE_DURATION: Duration = Duration::from_millis(100);
const ANIMATION_INTERVAL: Duration = Duration::from_millis(8);
/// Toggles arriving this soon after a focus-loss dismiss are ignored. Clicking the taskbar Start
/// button deactivates the menu on mouse-down and posts the toggle on mouse-up, which would
/// otherwise reopen the menu the click just closed.
const REOPEN_GUARD: Duration = Duration::from_millis(400);

/// Launch IDs whose icons were already handed to the menu. Only the background loader and watcher
/// threads touch it, so an app list rebuild renders icons for new apps only.
static RENDERED_ICONS: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Mutex::default);

thread_local! {
    static WIN_KEY_TAP: Cell<MenuWinKeyTap> = Cell::new(MenuWinKeyTap::default());
    static CONTROLLER: RefCell<MenuController> = RefCell::new(MenuController::new());
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Hidden,
    Materializing { started_at: Instant },
    Open,
    Dematerializing { started_at: Instant, from_progress: f32 },
    Clearing { ticks_left: u8 },
}

struct MenuController {
    config: Config,
    capturer: backdrop::Capturer,
    glass_state: Rc<RefCell<GlassState>>,
    monitor_rect: MonitorRect,
    phase: Phase,
    timer: Timer,
    dismissed_by_deactivation_at: Option<Instant>,
    /// Rebuilt app index waiting for the next show, so the list never changes under the cursor.
    pending_apps: Option<Vec<App>>,
}

impl MenuController {
    fn new() -> Self {
        let monitor_rect =
            backdrop::active_monitor_rect().unwrap_or(MonitorRect { x: 0, y: 0, width: 1440, height: 900 });
        Self {
            config: Config::load(),
            capturer: backdrop::Capturer::default(),
            glass_state: Rc::new(RefCell::new(GlassState::default())),
            monitor_rect,
            phase: Phase::Hidden,
            timer: Timer::default(),
            dismissed_by_deactivation_at: None,
            pending_apps: None,
        }
    }
}

pub fn connect_callbacks(start_menu: &StartMenu) {
    let writer: Option<usage::Writer> = usage::Writer::spawn()
        .inspect_err(|error| eprintln!("pins and launch history will not be saved: {error:#}"))
        .ok();
    start_menu.set_rows(ModelRc::from(Rc::new(AppList::new(writer))));

    let power_labels: Vec<SharedString> = PowerAction::ALL.iter().map(|action| action.label().into()).collect();
    start_menu.set_power_labels(ModelRc::from(Rc::new(VecModel::from(power_labels))));

    CONTROLLER.with(|cell| {
        let controller = cell.borrow();
        let rect = controller.monitor_rect;
        backdrop::place_overlay_window(start_menu.window(), rect);
        apply_layout(start_menu, rect, controller.config, &controller.glass_state);
        set_progress(start_menu, &controller.glass_state, 0.0);
        glass::install_rendering_notifier(start_menu, Rc::clone(&controller.glass_state));
    });

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.window().on_winit_window_event(move |_, event| {
        let Some(start_menu) = weak_menu.upgrade() else {
            return EventResult::Propagate;
        };
        match event {
            winit::event::WindowEvent::Focused(false) => {
                WIN_KEY_TAP.with(|cell| cell.set(MenuWinKeyTap::default()));
                dismiss_on_deactivation(&start_menu);
            }
            winit::event::WindowEvent::KeyboardInput { event, .. } if trigger::MENU_HANDLES_WIN_KEY => {
                handle_win_key(&start_menu, event);
            }
            winit::event::WindowEvent::Resized(size) if backdrop::OVERLAY_FOLLOWS_WINDOW_SIZE => {
                follow_window_size(&start_menu, size.width, size.height);
            }
            _ => {}
        }
        EventResult::Propagate
    });

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.on_dismiss(move || {
        if let Some(start_menu) = weak_menu.upgrade() {
            dismiss(&start_menu);
        }
    });

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.on_search(move |query| {
        if let Some(start_menu) = weak_menu.upgrade() {
            AppList::with(&start_menu, |app_list| app_list.filter(&query));
        }
    });

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.on_launch(move |row| {
        if let Some(start_menu) = weak_menu.upgrade() {
            launch(&start_menu, row);
        }
    });

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.on_toggle_pin(move |row| {
        if let Some(start_menu) = weak_menu.upgrade() {
            toggle_pin(&start_menu, row);
        }
    });

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.on_power(move |index| {
        if let Some(start_menu) = weak_menu.upgrade()
            && let Some(&action) = usize::try_from(index).ok().and_then(|index| PowerAction::ALL.get(index))
        {
            perform_power_action(&start_menu, action);
        }
    });

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.on_cycle_anchor(move || {
        if let Some(start_menu) = weak_menu.upgrade() {
            cycle_anchor(&start_menu);
        }
    });
}

/// Lays the menu out for the size the compositor gave the fullscreen overlay.
fn follow_window_size(start_menu: &StartMenu, width: u32, height: u32) {
    if width == 0 || height == 0 {
        return;
    }

    CONTROLLER.with(|cell| {
        let mut controller = cell.borrow_mut();
        let rect = MonitorRect { x: 0, y: 0, width, height };
        if controller.monitor_rect == rect {
            return;
        }
        controller.monitor_rect = rect;
        apply_layout(start_menu, rect, controller.config, &controller.glass_state);
    });
    start_menu.window().request_redraw();
}

/// While the menu has keyboard focus the trigger's keyboard hook never sees its keys, so the window
/// handles the Windows key itself (see `MenuWinKeyTap`).
fn handle_win_key(start_menu: &StartMenu, event: &winit::event::KeyEvent) {
    let Some(key) = menu_key(event) else {
        return;
    };

    match WIN_KEY_TAP.with(|cell| {
        let mut tap: MenuWinKeyTap = cell.get();
        let action: MenuKeyAction = tap.on_key(key);
        cell.set(tap);
        action
    }) {
        MenuKeyAction::Mask => trigger::suppress_native_start_menu(),
        MenuKeyAction::Toggle => dismiss(start_menu),
        MenuKeyAction::None => {}
    }
}

fn menu_key(event: &winit::event::KeyEvent) -> Option<MenuKey> {
    use winit::keyboard::{Key, NamedKey, PhysicalKey};

    let is_win_key: bool = event.logical_key == Key::Named(NamedKey::Super);
    match (is_win_key, event.state.is_pressed()) {
        (true, true) => Some(MenuKey::WinPressed),
        (true, false) => Some(MenuKey::WinReleased),
        // The mask key has no scan code, so it arrives unidentified and must not count as a chord.
        (false, true) if !matches!(event.physical_key, PhysicalKey::Unidentified(_)) => Some(MenuKey::OtherPressed),
        (false, _) => None,
    }
}

/// Loads the usage history and app index on a background thread and hands them to the menu, then
/// renders the app icons and hands those over too.
pub fn load_apps_in_background(start_menu: &StartMenu) -> anyhow::Result<()> {
    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();

    thread::Builder::new()
        .name("rusty-start-app-index".into())
        .spawn(move || {
            let started_at: Instant = Instant::now();
            let usage: anyhow::Result<Usage> = usage::load();
            let apps: Vec<App> = apps::load()
                .inspect_err(|error| eprintln!("failed to load the app list: {error:#}"))
                .unwrap_or_default();
            eprintln!("loaded the app index ({} apps) in {} ms", apps.len(), started_at.elapsed().as_millis());
            let icon_apps: Vec<App> = apps.clone();

            let delivered =
                weak_menu.upgrade_in_event_loop(move |start_menu| set_initial_data(&start_menu, apps, usage));
            if let Err(error) = delivered {
                eprintln!("failed to deliver the app list: {error}");
                return;
            }

            deliver_icons(&weak_menu, &icon_apps);
        })
        .context("failed to spawn the app index thread")?;

    Ok(())
}

fn set_initial_data(start_menu: &StartMenu, apps: Vec<App>, usage: anyhow::Result<Usage>) {
    AppList::with(start_menu, |app_list| {
        match usage {
            Ok(usage) => app_list.set_usage(usage),
            Err(error) => {
                eprintln!("pins and launch history will not be saved this session: {error:#}");
                app_list.disable_saving();
            }
        }
        app_list.set_apps(apps);
    });
}

/// Renders icons not rendered before for `apps` on the calling thread and hands them to the menu.
/// Icons are keyed by launch ID, so they can be applied right away even while a rebuilt list waits
/// for the next show.
fn deliver_icons(weak_menu: &slint::Weak<StartMenu>, apps: &[App]) {
    let missing: Vec<App> = {
        let rendered = RENDERED_ICONS.lock().unwrap_or_else(PoisonError::into_inner);
        apps.iter().filter(|app| !rendered.contains(&app.launch_id)).cloned().collect()
    };
    if missing.is_empty() {
        return;
    }

    let started_at: Instant = Instant::now();
    let icons: Vec<(String, IconPixels)> = match apps::load_icons(&missing) {
        Ok(icons) => {
            eprintln!(
                "rendered {} of {} app icons in {} ms",
                icons.len(),
                missing.len(),
                started_at.elapsed().as_millis()
            );
            icons
        }
        Err(error) => {
            eprintln!("failed to load app icons: {error:#}");
            return;
        }
    };
    RENDERED_ICONS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .extend(icons.iter().map(|(launch_id, _)| launch_id.clone()));

    let delivered = weak_menu.upgrade_in_event_loop(move |start_menu| {
        AppList::with(&start_menu, |app_list| app_list.set_icons(icons));
    });
    if let Err(error) = delivered {
        eprintln!("failed to deliver the app icons: {error}");
    }
}

/// Rebuilds the app index off the UI thread after installs and uninstalls. The new list is applied
/// on the next show.
pub fn watch_apps(start_menu: &StartMenu) -> anyhow::Result<apps::AppWatcher> {
    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();

    apps::watch(move |apps| {
        eprintln!("rebuilt the app index ({} apps); it applies on the next show", apps.len());
        let icon_apps: Vec<App> = apps.clone();
        if let Err(error) = weak_menu.upgrade_in_event_loop(move |_| queue_apps(apps)) {
            eprintln!("failed to deliver the rebuilt app list: {error}");
            return;
        }
        deliver_icons(&weak_menu, &icon_apps);
    })
}

/// Stores a rebuilt app index until the next show.
fn queue_apps(apps: Vec<App>) {
    CONTROLLER.with(|cell| cell.borrow_mut().pending_apps = Some(apps));
}

pub fn toggle(start_menu: &StartMenu) {
    let (should_dismiss, recently_deactivated): (bool, bool) = CONTROLLER.with(|cell| {
        let controller = cell.borrow();
        let should_dismiss = start_menu.window().is_visible()
            && !matches!(controller.phase, Phase::Dematerializing { .. } | Phase::Clearing { .. } | Phase::Hidden);
        let recently_deactivated =
            controller.dismissed_by_deactivation_at.is_some_and(|at| at.elapsed() < REOPEN_GUARD);
        (should_dismiss, recently_deactivated)
    });

    if should_dismiss {
        dismiss(start_menu);
    } else if !recently_deactivated {
        show(start_menu);
    }
}

fn show(start_menu: &StartMenu) {
    CONTROLLER.with(|cell| {
        let mut controller = cell.borrow_mut();
        controller.timer.stop();
        controller.glass_state.borrow_mut().shown_at = Some(Instant::now());

        // Capture a fresh desktop snapshot only while the Start menu window is hidden so we never
        // capture our own Fog Veil.
        if !start_menu.window().is_visible() {
            match controller.capturer.capture_active_monitor() {
                Ok(snapshot) => {
                    controller.monitor_rect = snapshot.rect;
                    controller.glass_state.borrow_mut().pending_snapshot = Some(snapshot);
                }
                Err(error) => eprintln!("failed to capture the active monitor backdrop: {error:#}"),
            }
        }

        if let Some(apps) = controller.pending_apps.take() {
            AppList::with(start_menu, |app_list| app_list.set_apps(apps));
        }

        let rect: MonitorRect = controller.monitor_rect;
        backdrop::configure_overlay_window();
        backdrop::place_overlay_window(start_menu.window(), rect);
        apply_layout(start_menu, rect, controller.config, &controller.glass_state);

        set_progress(start_menu, &controller.glass_state, 0.0);
        start_menu.invoke_reset_search();
        controller.phase = Phase::Materializing { started_at: Instant::now() };
    });

    if let Err(error) = start_menu.show() {
        eprintln!("failed to show the menu: {error}");
        return;
    }

    if !backdrop::focus_overlay_window() {
        eprintln!("the menu could not take the foreground, so it has no keyboard focus");
    }

    // Ensure keyboard focus is active at t = 0ms even before the 180ms condense finishes.
    start_menu.invoke_focus_search();
    start_menu.window().request_redraw();
    start_animation_timer(start_menu);
}

/// Runs the `100ms` reverse condense animation before hiding the window.
fn dismiss(start_menu: &StartMenu) {
    let should_start: bool = CONTROLLER.with(|cell| {
        let mut controller = cell.borrow_mut();
        if !start_menu.window().is_visible()
            || matches!(controller.phase, Phase::Dematerializing { .. } | Phase::Clearing { .. } | Phase::Hidden)
        {
            return false;
        }

        let current_progress: f32 = controller.glass_state.borrow().progress.clamp(0.05, 1.0);
        controller.phase = Phase::Dematerializing { started_at: Instant::now(), from_progress: current_progress };
        true
    });

    if should_start {
        start_animation_timer(start_menu);
    }
}

/// Dismisses the menu when another window takes the foreground (Alt+Tab, a click elsewhere, a
/// launched app). Launching already moves the menu to `Clearing`, so `dismiss` ignores it then.
fn dismiss_on_deactivation(start_menu: &StartMenu) {
    let is_open: bool =
        CONTROLLER.with(|cell| matches!(cell.borrow().phase, Phase::Materializing { .. } | Phase::Open));
    if !is_open {
        return;
    }

    CONTROLLER.with(|cell| cell.borrow_mut().dismissed_by_deactivation_at = Some(Instant::now()));
    dismiss(start_menu);
}

/// Hides the window without destroying it, so the next show reuses the native window, OpenGL
/// context and Dual-Kawase blur FBOs. The search is reset and progress is zeroed so the next open
/// never flashes stale results.
fn hide(start_menu: &StartMenu) {
    CONTROLLER.with(|cell| {
        let mut controller = cell.borrow_mut();
        controller.timer.stop();
        controller.phase = Phase::Hidden;
        set_progress(start_menu, &controller.glass_state, 0.0);
    });

    start_menu.invoke_reset_search();

    if let Err(error) = start_menu.hide() {
        eprintln!("failed to hide the menu: {error}");
    }
}

fn launch(start_menu: &StartMenu, row: i32) {
    let Ok(row) = usize::try_from(row) else {
        return;
    };

    match AppList::with(start_menu, |app_list| app_list.activate(row)).flatten() {
        Some(Activation::Launch(app)) => {
            apps::launch(&app);
            clear_and_hide(start_menu);
        }
        Some(Activation::Power(action)) => perform_power_action(start_menu, action),
        None => {}
    }
}

fn perform_power_action(start_menu: &StartMenu, action: PowerAction) {
    power::perform(action);
    clear_and_hide(start_menu);
}

/// Clears the swapchain to 0.0 opacity before hiding so the next open never flashes the previous
/// frame.
fn clear_and_hide(start_menu: &StartMenu) {
    CONTROLLER.with(|cell| {
        let mut controller = cell.borrow_mut();
        set_progress(start_menu, &controller.glass_state, 0.0);
        controller.phase = Phase::Clearing { ticks_left: 2 };
    });
    start_menu.window().request_redraw();
    start_animation_timer(start_menu);
}

fn toggle_pin(start_menu: &StartMenu, row: i32) {
    let Ok(row) = usize::try_from(row) else {
        return;
    };

    if let Some(selected) = AppList::with(start_menu, |app_list| app_list.toggle_pin(row)).flatten() {
        start_menu.invoke_select_row(selected as i32);
    }
}

fn cycle_anchor(start_menu: &StartMenu) {
    CONTROLLER.with(|cell| {
        let mut controller = cell.borrow_mut();
        controller.config.card_anchor = controller.config.card_anchor.next();
        apply_layout(start_menu, controller.monitor_rect, controller.config, &controller.glass_state);

        if let Err(error) = controller.config.save() {
            eprintln!("failed to save config.toml: {error:#}");
        }
    });

    start_menu.window().request_redraw();
}

fn apply_layout(start_menu: &StartMenu, rect: MonitorRect, config: Config, glass_state: &Rc<RefCell<GlassState>>) {
    let scale: f32 = start_menu.window().scale_factor().max(0.5);
    let logical_w: f32 = rect.width as f32 / scale;
    let logical_h: f32 = rect.height as f32 / scale;
    let (card_x, card_y) = config.card_anchor.card_origin(logical_w, logical_h, config.edge_margin);

    start_menu.set_monitor_width(logical_w);
    start_menu.set_monitor_height(logical_h);
    start_menu.set_card_x(card_x);
    start_menu.set_card_y(card_y);
    start_menu.set_anchor_label(config.card_anchor.label().into());

    glass_state.borrow_mut().update_geometry(rect, scale, config);
}

fn set_progress(start_menu: &StartMenu, glass_state: &Rc<RefCell<GlassState>>, progress: f32) {
    let clamped: f32 = progress.clamp(0.0, 1.0);
    start_menu.set_progress(clamped);
    let mut borrowed = glass_state.borrow_mut();
    borrowed.progress = clamped;
    if clamped > 0.0001 {
        borrowed.zero_frame_presented = false;
    }
}

fn start_animation_timer(start_menu: &StartMenu) {
    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();

    CONTROLLER.with(|cell| {
        let controller = cell.borrow();
        controller.timer.start(TimerMode::Repeated, ANIMATION_INTERVAL, move || {
            let Some(menu) = weak_menu.upgrade() else {
                return;
            };
            tick_animation(&menu);
        });
    });
}

fn tick_animation(start_menu: &StartMenu) {
    let mut should_hide = false;

    CONTROLLER.with(|cell| {
        let mut controller = cell.borrow_mut();
        match controller.phase {
            Phase::Materializing { started_at } => {
                let raw_t: f32 =
                    (started_at.elapsed().as_secs_f32() / MATERIALIZE_DURATION.as_secs_f32()).clamp(0.0, 1.0);
                set_progress(start_menu, &controller.glass_state, raw_t);
                start_menu.window().request_redraw();

                if raw_t >= 1.0 {
                    controller.phase = Phase::Open;
                    controller.timer.stop();
                }
            }
            Phase::Dematerializing { started_at, from_progress } => {
                let raw_t: f32 =
                    (started_at.elapsed().as_secs_f32() / DEMATERIALIZE_DURATION.as_secs_f32()).clamp(0.0, 1.0);
                let eased: f32 = glass::cubic_out(raw_t);
                let progress: f32 = from_progress * (1.0 - eased);
                set_progress(start_menu, &controller.glass_state, progress);
                start_menu.window().request_redraw();

                if raw_t >= 1.0 {
                    if controller.glass_state.borrow().zero_frame_presented {
                        controller.timer.stop();
                        should_hide = true;
                    } else {
                        controller.phase = Phase::Clearing { ticks_left: 2 };
                    }
                }
            }
            Phase::Clearing { ticks_left } => {
                if controller.glass_state.borrow().zero_frame_presented || ticks_left <= 1 {
                    controller.timer.stop();
                    should_hide = true;
                } else {
                    controller.phase = Phase::Clearing { ticks_left: ticks_left - 1 };
                    start_menu.window().request_redraw();
                }
            }
            Phase::Open | Phase::Hidden => {
                controller.timer.stop();
            }
        }
    });

    if should_hide {
        hide(start_menu);
    }
}
