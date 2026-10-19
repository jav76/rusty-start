//! Showing, hiding and resetting the menu, plus driving the `180ms` Optical Condense and `100ms`
//! dismiss animations. Every function here must run on the UI thread.

use std::cell::RefCell;
use std::rc::Rc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use slint::{ComponentHandle, ModelRc, PhysicalPosition, PhysicalSize, Timer, TimerMode};

use crate::StartMenu;
use crate::app_list::AppList;
use crate::apps::{self, App};
use crate::backdrop::{self, MonitorRect};
use crate::config::Config;
use crate::glass::{self, GlassState};

const MATERIALIZE_DURATION: Duration = Duration::from_millis(180);
const DEMATERIALIZE_DURATION: Duration = Duration::from_millis(100);
const ANIMATION_INTERVAL: Duration = Duration::from_millis(8);

thread_local! {
    static CONTROLLER: RefCell<MenuController> = RefCell::new(MenuController::new());
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Hidden,
    Materializing { started_at: Instant },
    Open,
    Dematerializing { started_at: Instant, from_progress: f32 },
}

struct MenuController {
    config: Config,
    capturer: backdrop::Capturer,
    glass_state: Rc<RefCell<GlassState>>,
    monitor_rect: MonitorRect,
    phase: Phase,
    timer: Timer,
}

impl MenuController {
    fn new() -> Self {
        Self {
            config: Config::load(),
            capturer: backdrop::Capturer::default(),
            glass_state: Rc::new(RefCell::new(GlassState::default())),
            monitor_rect: MonitorRect { x: 0, y: 0, width: 1440, height: 900 },
            phase: Phase::Hidden,
            timer: Timer::default(),
        }
    }
}

pub fn connect_callbacks(start_menu: &StartMenu) {
    start_menu.set_apps(ModelRc::from(Rc::new(AppList::default())));

    CONTROLLER.with(|cell| {
        let controller = cell.borrow();
        glass::install_rendering_notifier(start_menu, Rc::clone(&controller.glass_state));
        apply_layout(start_menu, controller.monitor_rect, controller.config, &controller.glass_state);
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
    start_menu.on_cycle_anchor(move || {
        if let Some(start_menu) = weak_menu.upgrade() {
            cycle_anchor(&start_menu);
        }
    });
}

/// Builds the app index on a background thread and hands it to the menu when ready.
pub fn load_apps_in_background(start_menu: &StartMenu) -> anyhow::Result<()> {
    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();

    thread::Builder::new()
        .name("rusty-start-app-index".into())
        .spawn(move || match apps::load() {
            Ok(apps) => {
                if let Err(error) = weak_menu.upgrade_in_event_loop(move |start_menu| set_apps(&start_menu, apps)) {
                    eprintln!("failed to deliver the app list: {error}");
                }
            }
            Err(error) => eprintln!("failed to load the app list: {error:#}"),
        })
        .context("failed to spawn the app index thread")?;

    Ok(())
}

fn set_apps(start_menu: &StartMenu, apps: Vec<App>) {
    let query = start_menu.get_query();
    AppList::with(start_menu, |app_list| app_list.replace_apps(apps, &query));
}

pub fn toggle(start_menu: &StartMenu) {
    let should_dismiss: bool = CONTROLLER.with(|cell| {
        let controller = cell.borrow();
        start_menu.window().is_visible() && !matches!(controller.phase, Phase::Dematerializing { .. } | Phase::Hidden)
    });

    if should_dismiss {
        dismiss(start_menu);
    } else {
        show(start_menu);
    }
}

fn show(start_menu: &StartMenu) {
    CONTROLLER.with(|cell| {
        let mut controller = cell.borrow_mut();
        controller.timer.stop();

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

        let rect: MonitorRect = controller.monitor_rect;
        start_menu.window().set_position(PhysicalPosition::new(rect.x, rect.y));
        start_menu.window().set_size(PhysicalSize::new(rect.width, rect.height));
        apply_layout(start_menu, rect, controller.config, &controller.glass_state);

        set_progress(start_menu, &controller.glass_state, 0.0);
        start_menu.invoke_reset_search();
        controller.phase = Phase::Materializing { started_at: Instant::now() };
    });

    if let Err(error) = start_menu.show() {
        eprintln!("failed to show the menu: {error}");
        return;
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
        if !start_menu.window().is_visible() || matches!(controller.phase, Phase::Hidden) {
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

/// Hides the window immediately without destroying it, so the next show reuses the native window,
/// OpenGL context and Dual-Kawase blur FBOs. The search is reset first so the next open never
/// flashes stale results.
fn hide(start_menu: &StartMenu) {
    CONTROLLER.with(|cell| {
        let mut controller = cell.borrow_mut();
        controller.timer.stop();
        controller.phase = Phase::Hidden;
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

    if let Some(app) = AppList::with(start_menu, |app_list| app_list.app(row)).flatten() {
        apps::launch(&app);
        // Hide immediately on app launch so focus transfers to the target app with zero delay.
        hide(start_menu);
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

    glass_state.borrow_mut().update_geometry(rect.width, rect.height, scale, config);
}

fn set_progress(start_menu: &StartMenu, glass_state: &Rc<RefCell<GlassState>>, progress: f32) {
    let clamped: f32 = progress.clamp(0.0, 1.0);
    start_menu.set_progress(clamped);
    glass_state.borrow_mut().progress = clamped;
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
                    controller.timer.stop();
                    should_hide = true;
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
