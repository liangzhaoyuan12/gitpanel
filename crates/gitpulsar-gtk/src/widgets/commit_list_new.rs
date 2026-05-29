//! Virtualized commits list — `gtk::ListView` + `SignalListItemFactory` +
//! `gio::ListStore<CommitObject>`. Mirrors the v1.0.0 Changes-tab pattern.
//!
//! This module is being introduced alongside the legacy `commit_list` module
//! and is wired in piecewise. Exported helpers carry `#[allow(dead_code)]`
//! until `window.rs` is migrated to consume them in Task 6+.

use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gio;

use super::commit_object::CommitObject;
use crate::config::DateFormat;
use gitpulsar_core::models::{CommitInfo, DiffFile};

/// Marker CSS class applied to each row's outer Box. Handlers walk up the
/// widget tree from a button to find the row this way (same trick as the
/// Changes tab's `gp-file-row`).
#[allow(dead_code)]
const ROW_OUTER_CSS: &str = "gp-commit-row";

/// Per-row callback type. Called on row activation with the commit id.
#[allow(dead_code)]
pub type RowActivateCallback = Rc<dyn Fn(&str)>;
/// Called when the user clicks "Load more".
#[allow(dead_code)]
pub type LoadMoreCallback = Rc<dyn Fn()>;
/// Called when the HEAD commit's edit-message pencil is clicked.
#[allow(dead_code)]
pub type EditMessageCallback = Rc<dyn Fn(String)>;

/// Refs returned to window.rs for state management and signal wiring.
#[allow(dead_code)]
pub struct CommitListRefs {
    pub list_view: gtk::ListView,
    pub store: gio::ListStore,
    pub filter: gtk::CustomFilter,
    pub filter_model: gtk::FilterListModel,
    pub selection: gtk::SingleSelection,
}

#[allow(dead_code)]
pub fn build_commit_list_view(
    on_activate: RowActivateCallback,
    on_load_more: LoadMoreCallback,
    on_edit_head_message: EditMessageCallback,
    date_format_cell: Rc<std::cell::Cell<DateFormat>>,
) -> CommitListRefs {
    let store = gio::ListStore::new::<CommitObject>();
    let filter = gtk::CustomFilter::new(|_| true);
    let filter_model = gtk::FilterListModel::new(Some(store.clone()), Some(filter.clone()));
    let selection = gtk::SingleSelection::new(Some(filter_model.clone()));
    selection.set_can_unselect(false);

    let factory = build_row_factory(
        on_load_more.clone(),
        on_edit_head_message.clone(),
        date_format_cell,
    );

    let list_view = gtk::ListView::new(Some(selection.clone()), Some(factory));
    list_view.add_css_class("navigation-sidebar");
    list_view.set_vexpand(true);

    let on_activate_inner = on_activate.clone();
    list_view.connect_activate(move |lv, position| {
        let Some(model) = lv.model() else {
            return;
        };
        let Some(item) = model.item(position) else {
            return;
        };
        if let Ok(obj) = item.downcast::<CommitObject>() {
            if obj.is_load_more_sentinel() {
                return; // sentinel activation handled by the dedicated button
            }
            on_activate_inner(&obj.id());
        }
    });

    CommitListRefs {
        list_view,
        store,
        filter,
        filter_model,
        selection,
    }
}

#[allow(dead_code)]
pub fn populate_commit_store(
    store: &gio::ListStore,
    commits: &[CommitInfo],
    tags_map: &HashMap<String, Vec<String>>,
    ahead: usize,
    has_more: bool,
) {
    let mut items: Vec<CommitObject> = Vec::with_capacity(commits.len() + 1);
    for (idx, c) in commits.iter().enumerate() {
        let tags = tags_map.get(&c.id).cloned().unwrap_or_default();
        items.push(CommitObject::from_info(c, tags, idx == 0, idx < ahead));
    }
    if has_more {
        items.push(CommitObject::load_more_sentinel());
    }
    store.splice(0, store.n_items(), &items);
}

#[allow(dead_code)]
pub fn append_commits_to_store(
    store: &gio::ListStore,
    new_commits: &[CommitInfo],
    tags_map: &HashMap<String, Vec<String>>,
    ahead: usize,
    offset: usize,
    has_more: bool,
) {
    remove_load_more_sentinel(store);
    for (i, c) in new_commits.iter().enumerate() {
        let global_idx = offset + i;
        let tags = tags_map.get(&c.id).cloned().unwrap_or_default();
        let obj = CommitObject::from_info(c, tags, global_idx == 0, global_idx < ahead);
        store.append(&obj);
    }
    if has_more {
        store.append(&CommitObject::load_more_sentinel());
    }
}

#[allow(dead_code)]
pub fn set_commit_filter(filter: &gtk::CustomFilter, query: &str) {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        filter.set_filter_func(|_| true);
        return;
    }
    filter.set_filter_func(move |obj| {
        let Some(c) = obj.downcast_ref::<CommitObject>() else {
            return false;
        };
        if c.is_load_more_sentinel() {
            return true;
        }
        c.summary().to_lowercase().contains(&q)
            || c.short_id().to_lowercase().contains(&q)
            || c.id().to_lowercase().contains(&q)
    });
}

#[allow(dead_code)]
fn remove_load_more_sentinel(store: &gio::ListStore) {
    let n = store.n_items();
    if n == 0 {
        return;
    }
    if let Some(last) = store
        .item(n - 1)
        .and_then(|o| o.downcast::<CommitObject>().ok())
    {
        if last.is_load_more_sentinel() {
            store.remove(n - 1);
        }
    }
}

// Factory + bind + helpers — filled in by subsequent tasks.

#[allow(dead_code)]
fn build_row_factory(
    on_load_more: LoadMoreCallback,
    on_edit_head_message: EditMessageCallback,
    date_format_cell: Rc<std::cell::Cell<DateFormat>>,
) -> gtk::SignalListItemFactory {
    // Filled in Task 4.
    let _ = (on_load_more, on_edit_head_message, date_format_cell);
    gtk::SignalListItemFactory::new()
}

#[allow(dead_code)]
fn build_file_row(_f: &DiffFile) -> gtk::Box {
    // Filled in Task 5.
    gtk::Box::new(gtk::Orientation::Horizontal, 0)
}

#[allow(dead_code)]
fn find_child_by_name(_w: &gtk::Box, _name: &str) -> Option<gtk::Widget> {
    // Filled in Task 5.
    None
}

#[allow(dead_code)]
fn format_relative_time(time_unix: i64, date_format: DateFormat) -> String {
    // Filled in Task 5; placeholder so the module compiles for now.
    let _ = (time_unix, date_format);
    String::new()
}
