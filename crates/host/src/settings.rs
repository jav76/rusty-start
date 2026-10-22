//! The Settings Page: Card Anchor, Veil Mode, blur strength, the Meta key switch and the Setup
//! Audit. Every function here runs on the UI thread. Setup audits and changes touch the desktop, so
//! they run on the `rusty-start-setup` thread and hand their results back with
//! `upgrade_in_event_loop`.

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::config::{CardAnchor, Config, MAX_VEIL_BLUR, VeilMode};
use crate::setup::{self, ItemState, MetaKeyState, Options, SetupItem, SetupSummary};
use crate::{MetaKeyStatus, SetupRow, SetupState, SetupStatus, StartMenu, menu, trigger};

const META_KEY_LABEL: &str =
    if setup::OWNS_META_KEY { "Meta key opens Rusty Start" } else { "Windows key opens Rusty Start" };

/// Set while a setup task runs, so a second request cannot race the first.
static SETUP_BUSY: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug)]
enum SetupTask {
    Audit,
    Install,
    Undo,
    SetMetaKey(bool),
}

/// Results of a setup task, gathered on the setup thread. Slint models are not `Send`, so the
/// page's rows are built from this on the UI thread.
struct SetupReport {
    audit: anyhow::Result<Vec<SetupItem>>,
    runtime: Vec<SetupItem>,
    meta_key: Option<anyhow::Result<MetaKeyState>>,
    message: String,
}

pub fn connect_callbacks(start_menu: &StartMenu) {
    start_menu.set_meta_key_label(META_KEY_LABEL.into());

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.on_open_settings(move || {
        if let Some(start_menu) = weak_menu.upgrade() {
            run_setup_task(&start_menu, SetupTask::Audit);
        }
    });

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.on_choose_anchor(move |index| {
        if let Some(start_menu) = weak_menu.upgrade()
            && let Some(anchor) = usize::try_from(index).ok().and_then(CardAnchor::from_index)
        {
            menu::update_config(&start_menu, |config| config.card_anchor = anchor);
        }
    });

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.on_choose_veil_mode(move |index| {
        if let Some(start_menu) = weak_menu.upgrade()
            && let Some(mode) = usize::try_from(index).ok().and_then(VeilMode::from_index)
        {
            menu::update_config(&start_menu, |config| config.veil_mode = mode);
        }
    });

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.on_set_veil_blur(move |value| {
        if let Some(start_menu) = weak_menu.upgrade() {
            let blur: u8 = value.round().clamp(0.0, f32::from(MAX_VEIL_BLUR)) as u8;
            menu::update_config(&start_menu, |config| config.veil_blur = blur);
        }
    });

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.on_set_meta_key(move |enabled| {
        if let Some(start_menu) = weak_menu.upgrade() {
            set_meta_key(&start_menu, enabled);
        }
    });

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.on_install_setup(move || {
        if let Some(start_menu) = weak_menu.upgrade() {
            run_setup_task(&start_menu, SetupTask::Install);
        }
    });

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    start_menu.on_undo_setup(move || {
        if let Some(start_menu) = weak_menu.upgrade() {
            run_setup_task(&start_menu, SetupTask::Undo);
        }
    });
}

/// Shows `config` on the Settings Page.
pub fn show_config(start_menu: &StartMenu, config: Config) {
    start_menu.set_anchor_index(config.card_anchor.index() as i32);
    start_menu.set_veil_mode(config.veil_mode.index() as i32);
    start_menu.set_veil_blur(f32::from(config.veil_blur));
    if !setup::OWNS_META_KEY {
        start_menu.set_meta_key(MetaKeyStatus { enabled: config.use_win_key, available: true, detail: "".into() });
    }
}

fn set_meta_key(start_menu: &StartMenu, enabled: bool) {
    if setup::OWNS_META_KEY {
        run_setup_task(start_menu, SetupTask::SetMetaKey(enabled));
    } else {
        menu::update_config(start_menu, |config| config.use_win_key = enabled);
        trigger::set_win_key(enabled);
    }
}

fn run_setup_task(start_menu: &StartMenu, task: SetupTask) {
    if SETUP_BUSY.swap(true, Ordering::AcqRel) {
        return;
    }
    let mut status: SetupStatus = start_menu.get_setup();
    status.busy = true;
    start_menu.set_setup(status.clone());

    let weak_menu: slint::Weak<StartMenu> = start_menu.as_weak();
    let spawned = thread::Builder::new().name("rusty-start-setup".into()).spawn(move || {
        let report: SetupReport = perform(task);
        let delivered = weak_menu.upgrade_in_event_loop(move |start_menu| {
            SETUP_BUSY.store(false, Ordering::Release);
            show_report(&start_menu, report);
        });
        if let Err(error) = delivered {
            SETUP_BUSY.store(false, Ordering::Release);
            eprintln!("failed to deliver the setup audit: {error}");
        }
    });

    if let Err(error) = spawned {
        SETUP_BUSY.store(false, Ordering::Release);
        status.busy = false;
        status.message = format!("failed to start the setup check: {error}").into();
        start_menu.set_setup(status);
    }
}

/// Runs `task`, then audits setup so the page shows its result. Runs on the setup thread.
fn perform(task: SetupTask) -> SetupReport {
    let options = Options { dev: false, from_host: true };
    let mut changes: Vec<String> = Vec::new();
    let result: anyhow::Result<()> = match task {
        SetupTask::Audit => Ok(()),
        SetupTask::Install => setup::apply(options, &mut changes),
        SetupTask::Undo => setup::undo(options, &mut changes),
        SetupTask::SetMetaKey(enabled) => setup::set_meta_key(enabled, &mut changes),
    };
    let message: String = match (task, result) {
        (_, Err(error)) => format!("{error:#}"),
        (SetupTask::Install, Ok(())) => "Setup installed.".to_owned(),
        (SetupTask::Undo, Ok(())) => "Setup undone. Rusty Start keeps running until you stop it.".to_owned(),
        (SetupTask::Audit | SetupTask::SetMetaKey(_), Ok(())) => String::new(),
    };

    SetupReport {
        audit: setup::audit(),
        runtime: trigger::status_items(),
        meta_key: setup::OWNS_META_KEY.then(setup::meta_key),
        message,
    }
}

fn show_report(start_menu: &StartMenu, report: SetupReport) {
    let (items, message): (Vec<SetupItem>, String) = match report.audit {
        Ok(items) => (items, report.message),
        Err(error) if report.message.is_empty() => (Vec::new(), format!("failed to check setup: {error:#}")),
        Err(_) => (Vec::new(), report.message),
    };
    let summary: SetupSummary = setup::summarize(&items);
    let rows: Vec<SetupRow> = items.iter().chain(&report.runtime).map(setup_row).collect();

    start_menu.set_setup(SetupStatus {
        summary: summary.label().into(),
        rows: ModelRc::from(Rc::new(VecModel::from(rows))),
        busy: false,
        message: message.into(),
        can_install: matches!(summary, SetupSummary::NotInstalled | SetupSummary::PartlyInstalled),
        can_undo: setup::can_undo(&items),
    });

    if let Some(meta_key) = report.meta_key {
        start_menu.set_meta_key(match meta_key {
            Ok(state) => {
                MetaKeyStatus { enabled: state.enabled, available: state.available, detail: state.detail.into() }
            }
            Err(error) => MetaKeyStatus { enabled: false, available: false, detail: format!("{error:#}").into() },
        });
    }
}

fn setup_row(item: &SetupItem) -> SetupRow {
    let state: SetupState = match item.state {
        ItemState::Installed => SetupState::Installed,
        ItemState::Missing => SetupState::Missing,
        ItemState::Conflict => SetupState::Conflict,
        ItemState::Unavailable => SetupState::Unavailable,
    };
    SetupRow { label: item.label.into(), state, detail: item.detail.as_str().into() }
}
