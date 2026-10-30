//! The App Action Flyout: what else can be done with an app row. Rust decides which actions an app
//! offers and the flyout only renders their labels, reporting a choice by index. Every function
//! here must run on the UI thread.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::app_list::AppList;
use crate::apps::{self, App, AppAction};
use crate::{StartMenu, menu};

#[derive(Clone, Debug, PartialEq, Eq)]
enum FlyoutAction {
    /// Pins the app, or unpins it when `pinned`.
    TogglePin {
        pinned: bool,
    },
    App(AppAction),
}

impl FlyoutAction {
    fn label(&self) -> &str {
        match self {
            Self::TogglePin { pinned: false } => "Pin",
            Self::TogglePin { pinned: true } => "Unpin",
            Self::App(action) => action.label(),
        }
    }
}

/// The flyout on screen. Choices arrive as indexes into `actions`.
struct OpenFlyout {
    row: usize,
    app: App,
    actions: Vec<FlyoutAction>,
}

thread_local! {
    static OPEN_FLYOUT: RefCell<Option<OpenFlyout>> = const { RefCell::new(None) };
}

pub fn connect_callbacks(start_menu: &StartMenu) {
    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.on_open_actions(move |row| {
        if let Some(start_menu) = weak_menu.upgrade()
            && let Ok(row) = usize::try_from(row)
        {
            open(&start_menu, row);
        }
    });

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.on_choose_action(move |index| {
        if let Some(start_menu) = weak_menu.upgrade()
            && let Ok(index) = usize::try_from(index)
        {
            choose(&start_menu, index);
        }
    });

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.on_run_as_administrator(move |row| {
        if let Some(start_menu) = weak_menu.upgrade()
            && let Ok(row) = usize::try_from(row)
        {
            run_as_administrator(&start_menu, row);
        }
    });
}

/// Opens the flyout for the app in `row`. Headers and power actions have none.
fn open(start_menu: &StartMenu, row: usize) {
    let Some((app, pinned)) = AppList::with(start_menu, |app_list| app_list.app(row)).flatten() else {
        return;
    };
    let actions: Vec<FlyoutAction> = flyout_actions(pinned, apps::actions(&app));
    let labels: Vec<SharedString> = actions.iter().map(|action| action.label().into()).collect();

    start_menu.set_action_labels(ModelRc::from(Rc::new(VecModel::from(labels))));
    OPEN_FLYOUT.with(|cell| *cell.borrow_mut() = Some(OpenFlyout { row, app, actions }));
    start_menu.invoke_show_actions(row as i32);
}

fn flyout_actions(pinned: bool, app_actions: Vec<AppAction>) -> Vec<FlyoutAction> {
    std::iter::once(FlyoutAction::TogglePin { pinned }).chain(app_actions.into_iter().map(FlyoutAction::App)).collect()
}

fn choose(start_menu: &StartMenu, index: usize) {
    let Some(flyout) = OPEN_FLYOUT.with(|cell| cell.borrow_mut().take()) else {
        return;
    };
    match flyout.actions.get(index) {
        Some(FlyoutAction::TogglePin { .. }) => menu::toggle_pin(start_menu, flyout.row),
        Some(FlyoutAction::App(action)) => perform(start_menu, &flyout.app, action),
        None => {}
    }
}

/// Ctrl+Shift+Enter. Apps that cannot run as administrator are left alone rather than launched
/// without elevation.
fn run_as_administrator(start_menu: &StartMenu, row: usize) {
    let Some((app, _)) = AppList::with(start_menu, |app_list| app_list.app(row)).flatten() else {
        return;
    };
    if apps::actions(&app).contains(&AppAction::RunAsAdministrator) {
        perform(start_menu, &app, &AppAction::RunAsAdministrator);
    } else {
        eprintln!("{} cannot run as administrator", app.name);
    }
}

fn perform(start_menu: &StartMenu, app: &App, action: &AppAction) {
    if action.launches_app() {
        AppList::with(start_menu, |app_list| app_list.record_launch(&app.launch_id));
    }
    apps::perform_action(app, action);
    menu::clear_and_hide(start_menu);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_comes_first_and_names_what_it_will_do() {
        let unpinned: Vec<FlyoutAction> = flyout_actions(false, vec![AppAction::Uninstall]);
        let pinned: Vec<FlyoutAction> = flyout_actions(true, Vec::new());

        assert_eq!(unpinned.iter().map(FlyoutAction::label).collect::<Vec<_>>(), ["Pin", "Uninstall"]);
        assert_eq!(pinned.iter().map(FlyoutAction::label).collect::<Vec<_>>(), ["Unpin"]);
    }
}
