// Release builds run without a console window; debug builds keep one for diagnostic output.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_list;
mod apps;
mod backdrop;
mod config;
mod glass;
mod menu;
mod search;
mod trigger;

use slint::ComponentHandle;

slint::include_modules!();

fn main() -> anyhow::Result<()> {
    let start_menu: StartMenu = StartMenu::new()?;
    menu::connect_callbacks(&start_menu);
    menu::load_apps_in_background(&start_menu)?;

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
