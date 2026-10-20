// Release builds run without a console window; debug builds keep one for diagnostic output.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_list;
mod apps;
mod backdrop;
mod config;
mod glass;
mod instance;
mod menu;
mod power;
mod search;
mod taskbar;
mod trigger;
mod usage;

use slint::ComponentHandle;

slint::include_modules!();

fn main() -> anyhow::Result<()> {
    let Some(_instance): Option<instance::InstanceGuard> = instance::acquire()? else {
        eprintln!("Rusty Start is already running; toggled its menu instead");
        return Ok(());
    };

    let _overlay_owner: backdrop::OverlayOwner = backdrop::install_backend()?;
    let start_menu: StartMenu = StartMenu::new()?;
    menu::connect_callbacks(&start_menu);
    menu::load_apps_in_background(&start_menu)?;
    let _app_watcher: Option<apps::AppWatcher> = menu::watch_apps(&start_menu)
        .inspect_err(|error| eprintln!("installed apps will not refresh until restart: {error:#}"))
        .ok();

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    let _trigger: trigger::TriggerGuard = trigger::install(move || {
        let queued = weak_menu.upgrade_in_event_loop(|start_menu| menu::toggle(&start_menu));
        if let Err(error) = queued {
            eprintln!("failed to queue a menu toggle: {error}");
        }
    })?;

    slint::run_event_loop_until_quit()?;
    Ok(())
}
