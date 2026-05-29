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

// Factory + bind + helpers.

#[allow(dead_code)]
fn build_row_factory(
    on_load_more: LoadMoreCallback,
    on_edit_head_message: EditMessageCallback,
    date_format_cell: Rc<std::cell::Cell<DateFormat>>,
) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    let lm = on_load_more.clone();
    let em = on_edit_head_message.clone();
    factory.connect_setup(move |_, list_item| {
        let item = list_item
            .downcast_ref::<gtk::ListItem>()
            .expect("ListItem");
        let outer = build_row_template(lm.clone(), em.clone());
        item.set_child(Some(&outer));
        item.set_activatable(true);
    });

    let dfc_bind = date_format_cell.clone();
    factory.connect_bind(move |_, list_item| {
        let item = list_item
            .downcast_ref::<gtk::ListItem>()
            .expect("ListItem");
        let outer = item
            .child()
            .and_then(|c| c.downcast::<gtk::Box>().ok())
            .expect("row Box");
        let obj = item
            .item()
            .and_then(|o| o.downcast::<CommitObject>().ok())
            .expect("CommitObject");
        bind_row(&outer, &obj, dfc_bind.get());
    });

    factory.connect_unbind(|_, list_item| {
        let item = list_item
            .downcast_ref::<gtk::ListItem>()
            .expect("ListItem");
        if let Some(outer) = item.child().and_then(|c| c.downcast::<gtk::Box>().ok()) {
            // Collapse expanded detail so a recycled row doesn't flash the
            // previous commit's content on first scroll into view.
            if let Some(rev) = find_child_by_name(&outer, "detail-revealer")
                .and_then(|w| w.downcast::<gtk::Revealer>().ok())
            {
                rev.set_reveal_child(false);
            }
            outer.set_widget_name("");
        }
    });

    factory
}

#[allow(dead_code)]
fn build_row_template(
    on_load_more: LoadMoreCallback,
    on_edit_head_message: EditMessageCallback,
) -> gtk::Box {
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    outer.add_css_class(ROW_OUTER_CSS);

    // === Compact summary row ===
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row_box.set_margin_start(8);
    row_box.set_margin_end(8);
    row_box.set_margin_top(6);
    row_box.set_margin_bottom(6);
    row_box.set_widget_name("summary-row");

    let hash_label = gtk::Label::builder()
        .css_classes(["caption", "monospace", "dim-label"])
        .valign(gtk::Align::Start)
        .build();
    hash_label.set_widget_name("hash-label");
    row_box.append(&hash_label);

    let info_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    info_box.set_hexpand(true);

    let msg_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    msg_row.set_widget_name("msg-row");
    let message_label = gtk::Label::builder()
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(60)
        .build();
    message_label.set_widget_name("message-label");
    msg_row.append(&message_label);

    info_box.append(&msg_row);

    let meta_label = gtk::Label::builder()
        .xalign(0.0)
        .css_classes(["caption", "dim-label"])
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    meta_label.set_widget_name("meta-label");
    info_box.append(&meta_label);

    row_box.append(&info_box);

    let signed_icon = gtk::Image::builder()
        .icon_name("channel-secure-symbolic")
        .css_classes(["dim-label"])
        .tooltip_text("Signed commit")
        .valign(gtk::Align::Center)
        .build();
    signed_icon.set_widget_name("signed-icon");
    signed_icon.set_visible(false);
    row_box.append(&signed_icon);

    let edit_btn = gtk::Button::builder()
        .icon_name("document-edit-symbolic")
        .css_classes(["flat", "circular"])
        .tooltip_text("Edit Commit Message")
        .valign(gtk::Align::Center)
        .build();
    edit_btn.set_widget_name("edit-msg-btn");
    edit_btn.set_visible(false);
    {
        let em = on_edit_head_message.clone();
        let outer_weak = outer.downgrade();
        edit_btn.connect_clicked(move |_| {
            let Some(outer) = outer_weak.upgrade() else {
                return;
            };
            let msg_ptr = unsafe { outer.data::<String>("commit-message") };
            if let Some(ptr) = msg_ptr {
                let s: &String = unsafe { ptr.as_ref() };
                em(s.clone());
            }
        });
    }
    row_box.append(&edit_btn);

    outer.append(&row_box);

    // === Expandable detail revealer ===
    let detail_revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .reveal_child(false)
        .build();
    detail_revealer.set_widget_name("detail-revealer");

    let detail_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    detail_box.set_margin_start(20);
    detail_box.set_margin_end(8);
    detail_box.set_margin_top(0);
    detail_box.set_margin_bottom(8);
    detail_box.set_widget_name("detail-box");

    let files_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    files_box.set_widget_name("files-box");
    detail_box.append(&files_box);

    detail_revealer.set_child(Some(&detail_box));
    outer.append(&detail_revealer);

    // === Sentinel row content (created hidden, shown by bind if needed) ===
    let sentinel_label = gtk::Label::builder()
        .label("Load more commits...")
        .css_classes(["dim-label"])
        .margin_top(8)
        .margin_bottom(8)
        .build();
    sentinel_label.set_widget_name("sentinel-label");
    sentinel_label.set_visible(false);
    outer.append(&sentinel_label);

    // Click anywhere on the sentinel row triggers Load more.
    let click = gtk::GestureClick::new();
    let lm = on_load_more.clone();
    let outer_weak = outer.downgrade();
    click.connect_released(move |_, _, _, _| {
        let Some(outer) = outer_weak.upgrade() else {
            return;
        };
        if outer.widget_name() == "sentinel" {
            lm();
        }
    });
    outer.add_controller(click);

    outer
}

#[allow(dead_code)]
fn bind_row(outer: &gtk::Box, obj: &CommitObject, date_format: DateFormat) {
    let summary_row = find_child_by_name(outer, "summary-row");
    let detail_revealer = find_child_by_name(outer, "detail-revealer");
    let sentinel_label = find_child_by_name(outer, "sentinel-label");

    if obj.is_load_more_sentinel() {
        outer.set_widget_name("sentinel");
        if let Some(w) = summary_row.as_ref() {
            w.set_visible(false);
        }
        if let Some(w) = detail_revealer.as_ref() {
            w.set_visible(false);
        }
        if let Some(w) = sentinel_label.as_ref() {
            w.set_visible(true);
        }
        return;
    }

    outer.set_widget_name(&obj.id());
    // Stash full message for the edit-button closure to read.
    unsafe {
        outer.set_data::<String>("commit-message", obj.message());
    }

    if let Some(w) = summary_row.as_ref() {
        w.set_visible(true);
    }
    if let Some(w) = detail_revealer.as_ref() {
        w.set_visible(true);
    }
    if let Some(w) = sentinel_label.as_ref() {
        w.set_visible(false);
    }

    if let Some(hash) = find_child_by_name(outer, "hash-label")
        .and_then(|w| w.downcast::<gtk::Label>().ok())
    {
        hash.set_label(&obj.short_id());
    }
    if let Some(msg) = find_child_by_name(outer, "message-label")
        .and_then(|w| w.downcast::<gtk::Label>().ok())
    {
        msg.set_label(&obj.summary());
    }
    if let Some(meta) = find_child_by_name(outer, "meta-label")
        .and_then(|w| w.downcast::<gtk::Label>().ok())
    {
        let when = format_relative_time(obj.time_unix(), date_format);
        meta.set_label(&format!("{} {}", obj.author().name, when));
    }
    if let Some(signed) = find_child_by_name(outer, "signed-icon") {
        signed.set_visible(obj.is_signed());
    }
    if let Some(edit) = find_child_by_name(outer, "edit-msg-btn") {
        edit.set_visible(obj.is_head());
    }

    // Tag badges live inside msg-row after the message label. Clear and re-add.
    if let Some(msg_row) =
        find_child_by_name(outer, "msg-row").and_then(|w| w.downcast::<gtk::Box>().ok())
    {
        // Remove all children after message-label.
        let mut child = msg_row.first_child();
        let mut skip_first = true;
        while let Some(c) = child {
            let next = c.next_sibling();
            if skip_first {
                skip_first = false;
            } else {
                msg_row.remove(&c);
            }
            child = next;
        }
        for tag_name in obj.tags() {
            let label = gtk::Label::builder()
                .label(&tag_name)
                .css_classes(["caption"])
                .valign(gtk::Align::Center)
                .build();
            let frame = gtk::Frame::new(None);
            frame.set_child(Some(&label));
            frame.add_css_class("accent");
            frame.set_margin_start(2);
            msg_row.append(&frame);
        }
    }

    // Detail visibility + file list — populated lazily; window.rs sets
    // files via CommitObject and re-binds by toggling expanded.
    if let Some(rev) = detail_revealer.and_then(|w| w.downcast::<gtk::Revealer>().ok()) {
        rev.set_reveal_child(obj.expanded());
        if obj.expanded() {
            // Render whatever's currently in obj.files() — window.rs handles the
            // async fetch and re-binds via store.items_changed(idx, 1, 1).
            populate_files_into_outer(outer, obj);
        }
    }
}

#[allow(dead_code)]
fn populate_files_into_outer(outer: &gtk::Box, obj: &CommitObject) {
    let Some(files_box) = find_child_by_name(outer, "files-box")
        .and_then(|w| w.downcast::<gtk::Box>().ok())
    else {
        return;
    };
    while let Some(c) = files_box.first_child() {
        files_box.remove(&c);
    }
    if !obj.files_loaded() {
        let spinner = gtk::Spinner::builder()
            .spinning(true)
            .halign(gtk::Align::Center)
            .margin_top(4)
            .margin_bottom(4)
            .build();
        files_box.append(&spinner);
        return;
    }
    let files = obj.files();
    if files.is_empty() {
        let empty = gtk::Label::builder()
            .label("No file changes")
            .css_classes(["dim-label"])
            .xalign(0.0)
            .build();
        files_box.append(&empty);
        return;
    }
    for f in &files {
        files_box.append(&build_file_row(f));
    }
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
