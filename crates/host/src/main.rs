// Release builds run without a console window; debug builds keep one for diagnostic output.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod action_flyout;
mod app_list;
mod apps;
mod backdrop;
mod config;
#[cfg(backend_linux)]
mod dbus;
mod glass;
mod instance;
mod logging;
mod menu;
mod power;
mod search;
mod settings;
mod setup;
mod trace;
mod trigger;
mod usage;

use std::process::ExitCode;

use slint::ComponentHandle;

slint::include_modules!();

fn main() -> ExitCode {
    let _log: Option<logging::LogGuard> = logging::init();
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            logging::report_failure(&error);
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let Some(("setup", setup_arguments)) = arguments.split_first().map(|(first, rest)| (first.as_str(), rest)) {
        let report: String = setup::run(setup_arguments)?;
        logging::print_report(&report);
        return Ok(());
    }

    let Some(_instance): Option<instance::InstanceGuard> = instance::acquire()? else {
        log::info!("Rusty Start is already running; toggled its menu instead");
        return Ok(());
    };

    let _overlay_owner: backdrop::OverlayOwner = backdrop::install_backend()?;
    let start_menu: StartMenu = StartMenu::new()?;
    menu::connect_callbacks(&start_menu);
    menu::load_apps_in_background(&start_menu)?;
    let _app_watcher: Option<apps::AppWatcher> = menu::watch_apps(&start_menu)
        .inspect_err(|error| log::warn!("installed apps will not refresh until restart: {error:#}"))
        .ok();

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    let _trigger: trigger::TriggerGuard = trigger::install(move || {
        let queued = weak_menu.upgrade_in_event_loop(|start_menu| menu::toggle(&start_menu));
        if let Err(error) = queued {
            log::error!("failed to queue a menu toggle: {error}");
        }
    })?;

    slint::run_event_loop_until_quit()?;
    Ok(())
}
