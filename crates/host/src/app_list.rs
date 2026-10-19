//! Slint model of the apps matching the current query. The model owns the search index, so any
//! UI-thread code holding the `StartMenu` can reach it through `AppList::with`.

use std::any::Any;
use std::cell::RefCell;

use slint::{Model, ModelNotify, ModelRc, ModelTracker, StandardListViewItem};

use crate::StartMenu;
use crate::apps::App;
use crate::search::Index;

#[derive(Default)]
pub struct AppList {
    index: RefCell<Index>,
    /// Index positions of the matching apps, in display order.
    matches: RefCell<Vec<usize>>,
    notify: ModelNotify,
}

impl AppList {
    /// Runs `action` with the list attached to `start_menu`, or returns `None` if none is attached.
    pub fn with<R>(start_menu: &StartMenu, action: impl FnOnce(&AppList) -> R) -> Option<R> {
        let model: ModelRc<StandardListViewItem> = start_menu.get_apps();
        model.as_any().downcast_ref::<AppList>().map(action)
    }

    pub fn replace_apps(&self, apps: Vec<App>, query: &str) {
        *self.index.borrow_mut() = Index::new(apps);
        self.filter(query);
    }

    pub fn filter(&self, query: &str) {
        *self.matches.borrow_mut() = self.index.borrow().search(query);
        self.notify.reset();
    }

    pub fn app(&self, row: usize) -> Option<App> {
        let position: usize = *self.matches.borrow().get(row)?;
        self.index.borrow().app(position).cloned()
    }
}

impl Model for AppList {
    type Data = StandardListViewItem;

    fn row_count(&self) -> usize {
        self.matches.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<StandardListViewItem> {
        let position: usize = *self.matches.borrow().get(row)?;
        let index = self.index.borrow();
        Some(StandardListViewItem::from(index.app(position)?.name.as_str()))
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
