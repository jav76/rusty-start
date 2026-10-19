//! Showing, hiding and resetting the menu. Every function here must run on the UI thread.

use std::rc::Rc;
use std::thread;

use anyhow::Context;
use slint::{ComponentHandle, ModelRc};

use crate::StartMenu;
use crate::app_list::AppList;
use crate::apps::{self, App};

pub fn connect_callbacks(start_menu: &StartMenu) {
    start_menu.set_apps(ModelRc::from(Rc::new(AppList::default())));

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.on_dismiss(move || {
        if let Some(start_menu) = weak_menu.upgrade() {
            hide(&start_menu);
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
    if start_menu.window().is_visible() {
        hide(start_menu);
    } else {
        show(start_menu);
    }
}

fn show(start_menu: &StartMenu) {
    if let Err(error) = start_menu.show() {
        eprintln!("failed to show the menu: {error}");
    }
}

/// Hides the window without destroying it, so the next show reuses the native window and its
/// render surface. The search is reset first so the next open never flashes stale results.
fn hide(start_menu: &StartMenu) {
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
        hide(start_menu);
    }
}
