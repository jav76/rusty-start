//! Slint model of the menu rows. With an empty query it lists pinned apps, recent apps and then
//! every app; with a query it lists ranked app matches followed by matching power actions.
//!
//! The model owns the search indexes, usage history and icons, so any UI-thread code holding the
//! `StartMenu` can reach them through `AppList::with`. Every method must run on the UI thread.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;

use slint::{Image, Model, ModelNotify, ModelRc, ModelTracker};

use crate::apps::{App, IconPixels};
use crate::power::PowerAction;
use crate::search::Index;
use crate::usage::{self, Usage};
use crate::{MenuRow, RowKind, StartMenu};

/// Recently launched apps listed between the pinned apps and the full list.
const RECENT_LIMIT: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Pinned,
    Recent,
    AllApps,
    /// Ranked app matches for a query, listed without a header.
    Results,
    System,
}

impl Section {
    fn title(self) -> &'static str {
        match self {
            Self::Pinned => "Pinned",
            Self::Recent => "Recent",
            Self::AllApps => "All apps",
            Self::Results => "Results",
            Self::System => "System",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Header(Section),
    /// `position` indexes `Catalog::apps`. An app can appear in several sections.
    App {
        position: usize,
        section: Section,
    },
    Power(PowerAction),
}

/// What the host should do after a row was chosen.
pub enum Activation {
    Launch(App),
    Power(PowerAction),
}

/// Platform-independent row composition, kept apart from the Slint model so it can be tested.
#[derive(Default)]
struct Catalog {
    apps: Index<App>,
    /// Index position of every app by launch ID.
    positions: HashMap<String, usize>,
    power: Index<PowerAction>,
    usage: Usage,
}

impl Catalog {
    fn new() -> Self {
        Self { power: Index::new(PowerAction::ALL.to_vec()), ..Self::default() }
    }

    fn set_apps(&mut self, apps: Vec<App>) {
        self.apps = Index::new(apps);
        self.positions = self
            .apps
            .all()
            .into_iter()
            .filter_map(|position| Some((self.apps.get(position)?.launch_id.clone(), position)))
            .collect();
    }

    fn rows(&self, query: &str, now: u64) -> Vec<Row> {
        if query.trim().is_empty() { self.browse_rows() } else { self.search_rows(query, now) }
    }

    fn browse_rows(&self) -> Vec<Row> {
        let pinned: Vec<usize> =
            self.usage.pinned().iter().filter_map(|launch_id| self.positions.get(launch_id).copied()).collect();
        let recent: Vec<usize> = self
            .usage
            .recent_first()
            .into_iter()
            .filter(|launch_id| !self.usage.is_pinned(launch_id))
            .filter_map(|launch_id| self.positions.get(launch_id).copied())
            .take(RECENT_LIMIT)
            .collect();

        let mut rows: Vec<Row> = Vec::new();
        push_section(&mut rows, Section::Pinned, &pinned, true);
        push_section(&mut rows, Section::Recent, &recent, true);
        // The full list needs no header when nothing is listed above it.
        let needs_header: bool = !rows.is_empty();
        push_section(&mut rows, Section::AllApps, &self.apps.all(), needs_header);
        rows
    }

    fn search_rows(&self, query: &str, now: u64) -> Vec<Row> {
        let apps: Vec<usize> = self.apps.search(query, |app| self.usage.frecency(&app.launch_id, now));
        let mut rows: Vec<Row> = Vec::new();
        push_section(&mut rows, Section::Results, &apps, false);

        let actions: Vec<usize> = self.power.search(query, |_| 0);
        if !actions.is_empty() {
            rows.push(Row::Header(Section::System));
            rows.extend(actions.into_iter().filter_map(|position| self.power.get(position).copied()).map(Row::Power));
        }
        rows
    }

    fn launch_id(&self, position: usize) -> Option<&str> {
        self.apps.get(position).map(|app| app.launch_id.as_str())
    }
}

fn push_section(rows: &mut Vec<Row>, section: Section, positions: &[usize], with_header: bool) {
    if positions.is_empty() {
        return;
    }
    if with_header {
        rows.push(Row::Header(section));
    }
    rows.extend(positions.iter().map(|&position| Row::App { position, section }));
}

/// Row showing the app at `position` after a pin toggle: the same section if it is still listed,
/// otherwise its first appearance.
fn row_after_toggle(rows: &[Row], position: usize, section: Section) -> Option<usize> {
    let same_app = |row: &Row| matches!(*row, Row::App { position: row_position, .. } if row_position == position);

    rows.iter().position(|row| *row == Row::App { position, section }).or_else(|| rows.iter().position(same_app))
}

struct State {
    catalog: Catalog,
    icons: HashMap<String, Image>,
    query: String,
    rows: Vec<Row>,
    /// Power action waiting for a second selection.
    confirming: Option<PowerAction>,
    /// `None` when usage must not be written, for example because the file failed to load.
    writer: Option<usage::Writer>,
}

pub struct AppList {
    state: RefCell<State>,
    notify: ModelNotify,
}

impl AppList {
    pub fn new(writer: Option<usage::Writer>) -> Self {
        let state = State {
            catalog: Catalog::new(),
            icons: HashMap::new(),
            query: String::new(),
            rows: Vec::new(),
            confirming: None,
            writer,
        };
        Self { state: RefCell::new(state), notify: ModelNotify::default() }
    }

    /// Runs `action` with the list attached to `start_menu`, or returns `None` if none is attached.
    pub fn with<R>(start_menu: &StartMenu, action: impl FnOnce(&AppList) -> R) -> Option<R> {
        let model: ModelRc<MenuRow> = start_menu.get_rows();
        model.as_any().downcast_ref::<AppList>().map(action)
    }

    pub fn set_apps(&self, apps: Vec<App>) {
        self.state.borrow_mut().catalog.set_apps(apps);
        self.refresh();
    }

    pub fn set_usage(&self, usage: Usage) {
        self.state.borrow_mut().catalog.usage = usage;
        self.refresh();
    }

    /// Keeps pins and launches in memory only, so an unreadable usage file is never overwritten.
    pub fn disable_saving(&self) {
        self.state.borrow_mut().writer = None;
    }

    pub fn set_icons(&self, icons: Vec<(String, IconPixels)>) {
        {
            let mut state = self.state.borrow_mut();
            for (launch_id, pixels) in icons {
                state.icons.insert(launch_id, Image::from_rgba8_premultiplied(pixels));
            }
        }
        self.notify.reset();
    }

    pub fn filter(&self, query: &str) {
        {
            let mut state = self.state.borrow_mut();
            query.clone_into(&mut state.query);
            state.confirming = None;
        }
        self.refresh();
    }

    /// Chooses `row`. Apps are recorded in the launch history. Power actions that need
    /// confirmation return `None` the first time and show a prompt instead.
    pub fn activate(&self, row: usize) -> Option<Activation> {
        let mut state = self.state.borrow_mut();

        match *state.rows.get(row)? {
            Row::Header(_) => None,
            Row::App { position, .. } => {
                let app: App = state.catalog.apps.get(position)?.clone();
                state.catalog.usage.record_launch(&app.launch_id, usage::unix_now());
                save(&state);
                Some(Activation::Launch(app))
            }
            Row::Power(action) if action.needs_confirmation() && state.confirming != Some(action) => {
                state.confirming = Some(action);
                drop(state);
                self.notify.reset();
                None
            }
            Row::Power(action) => {
                state.confirming = None;
                Some(Activation::Power(action))
            }
        }
    }

    /// Pins or unpins the app in `row`. Returns the row to select afterwards, so keyboard focus
    /// follows the app when the rows shift.
    pub fn toggle_pin(&self, row: usize) -> Option<usize> {
        let (position, section): (usize, Section) = {
            let mut state = self.state.borrow_mut();
            let Row::App { position, section } = *state.rows.get(row)? else {
                return None;
            };
            let launch_id: String = state.catalog.launch_id(position)?.to_owned();
            state.catalog.usage.toggle_pin(&launch_id);
            save(&state);
            (position, section)
        };

        self.refresh();
        row_after_toggle(&self.state.borrow().rows, position, section)
    }

    fn refresh(&self) {
        {
            let mut state = self.state.borrow_mut();
            let rows: Vec<Row> = state.catalog.rows(&state.query, usage::unix_now());
            state.rows = rows;
        }
        self.notify.reset();
    }
}

fn save(state: &State) {
    if let Some(writer) = &state.writer {
        writer.save(&state.catalog.usage);
    }
}

impl Model for AppList {
    type Data = MenuRow;

    fn row_count(&self) -> usize {
        self.state.borrow().rows.len()
    }

    fn row_data(&self, row: usize) -> Option<MenuRow> {
        let state = self.state.borrow();

        Some(match *state.rows.get(row)? {
            Row::Header(section) => {
                MenuRow { kind: RowKind::Header, text: section.title().into(), ..MenuRow::default() }
            }
            Row::App { position, .. } => {
                let app: &App = state.catalog.apps.get(position)?;
                MenuRow {
                    kind: RowKind::App,
                    text: app.name.as_str().into(),
                    icon: state.icons.get(&app.launch_id).cloned().unwrap_or_default(),
                    pinned: state.catalog.usage.is_pinned(&app.launch_id),
                    confirming: false,
                }
            }
            Row::Power(action) => {
                let confirming: bool = state.confirming == Some(action);
                let text: &str = if confirming { action.confirmation_prompt() } else { action.label() };
                MenuRow { kind: RowKind::System, text: text.into(), confirming, ..MenuRow::default() }
            }
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn catalog(names: &[&str]) -> Catalog {
        let mut catalog = Catalog::new();
        catalog
            .set_apps(names.iter().map(|name| App { name: name.to_string(), launch_id: name.to_string() }).collect());
        catalog
    }

    /// Renders rows as short strings, for example `"# Pinned"`, `"Paint"` and `"! Restart"`.
    fn describe(catalog: &Catalog, rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| match *row {
                Row::Header(section) => format!("# {}", section.title()),
                Row::App { position, .. } => catalog.apps.get(position).unwrap().name.clone(),
                Row::Power(action) => format!("! {}", action.label()),
            })
            .collect()
    }

    fn browse(catalog: &Catalog) -> Vec<String> {
        describe(catalog, &catalog.rows("", NOW))
    }

    #[test]
    fn without_history_lists_every_app_without_headers() {
        let catalog = catalog(&["Paint", "Calculator"]);

        assert_eq!(browse(&catalog), ["Calculator", "Paint"]);
    }

    #[test]
    fn lists_pinned_then_recent_then_all_apps() {
        let mut catalog = catalog(&["Calculator", "Edge", "Paint", "Terminal"]);
        catalog.usage.toggle_pin("Terminal");
        catalog.usage.toggle_pin("Calculator");
        catalog.usage.record_launch("Paint", NOW);

        assert_eq!(
            browse(&catalog),
            [
                "# Pinned",
                "Terminal",
                "Calculator",
                "# Recent",
                "Paint",
                "# All apps",
                "Calculator",
                "Edge",
                "Paint",
                "Terminal"
            ]
        );
    }

    #[test]
    fn recent_skips_pinned_and_uninstalled_apps() {
        let mut catalog = catalog(&["Edge", "Paint"]);
        catalog.usage.toggle_pin("Paint");
        catalog.usage.record_launch("Paint", NOW);
        catalog.usage.record_launch("Removed App", NOW);

        assert_eq!(browse(&catalog), ["# Pinned", "Paint", "# All apps", "Edge", "Paint"]);
    }

    #[test]
    fn recent_is_capped() {
        let names: Vec<String> = (0..8).map(|index| format!("App {index}")).collect();
        let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut catalog = catalog(&name_refs);
        for (offset, name) in names.iter().enumerate() {
            catalog.usage.record_launch(name, NOW - offset as u64);
        }

        let rows: Vec<Row> = catalog.rows("", NOW);
        let recent: usize = rows.iter().filter(|row| matches!(row, Row::App { section: Section::Recent, .. })).count();

        assert_eq!(recent, RECENT_LIMIT);
    }

    #[test]
    fn search_lists_apps_then_matching_power_actions() {
        let catalog = catalog(&["ShareX", "Notepad"]);

        assert_eq!(describe(&catalog, &catalog.rows("sh", NOW)), ["ShareX", "# System", "! Shut down"]);
    }

    #[test]
    fn search_ranks_frequent_apps_first_within_a_match_quality() {
        let mut catalog = catalog(&["Microsoft Edge", "Microsoft Edge Dev"]);
        catalog.usage.record_launch("Microsoft Edge Dev", NOW);

        assert_eq!(describe(&catalog, &catalog.rows("edge", NOW)), ["Microsoft Edge Dev", "Microsoft Edge"]);
    }

    #[test]
    fn selection_stays_in_the_section_after_pinning_from_all_apps() {
        let mut catalog = catalog(&["Edge", "Paint"]);
        catalog.usage.toggle_pin("Paint");
        let rows: Vec<Row> = catalog.rows("", NOW);
        let paint: usize = catalog.positions["Paint"];

        assert_eq!(row_after_toggle(&rows, paint, Section::AllApps), Some(4));
    }

    #[test]
    fn selection_falls_back_to_the_first_appearance() {
        let mut catalog = catalog(&["Edge", "Paint"]);
        catalog.usage.toggle_pin("Paint");
        let rows: Vec<Row> = catalog.rows("", NOW);
        let paint: usize = catalog.positions["Paint"];

        assert_eq!(row_after_toggle(&rows, paint, Section::Recent), Some(1));
    }
}
