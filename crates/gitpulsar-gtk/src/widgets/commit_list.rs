//! Virtualized commits list — `gtk::ListView` + `SignalListItemFactory` +
//! `gio::ListStore<CommitObject>`. Mirrors the v1.0.0 Changes-tab pattern.

use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use chrono::TimeZone;
use gtk::gio;

use super::commit_object::CommitObject;
use crate::config::DateFormat;
use gitpulsar_core::models::{CommitInfo, DiffFile};

/// Marker CSS class applied to each row's outer Box. Handlers walk up the
/// widget tree from a button to find the row this way (same trick as the
/// Changes tab's `gp-file-row`).
const ROW_OUTER_CSS: &str = "gp-commit-row";

/// Per-row callback type. Called on row activation with the commit id.
pub type RowActivateCallback = Rc<dyn Fn(&str)>;
/// Called when the user clicks "Load more".
pub type LoadMoreCallback = Rc<dyn Fn()>;
/// Called when the HEAD commit's edit-message pencil is clicked.
pub type EditMessageCallback = Rc<dyn Fn(String)>;
/// Called with `(commit_id, file_path)` when a file inside a commit is clicked.
pub type FileOpenCallback = Rc<dyn Fn(&str, &str)>;

/// Refs returned to window.rs for state management and signal wiring.
pub struct CommitListRefs {
    pub list_view: gtk::ListView,
    pub store: gio::ListStore,
    pub filter: gtk::CustomFilter,
    pub filter_model: gtk::FilterListModel,
    pub selection: gtk::SingleSelection,
}

pub fn build_commit_list_view(
    on_activate: RowActivateCallback,
    on_load_more: LoadMoreCallback,
    on_edit_head_message: EditMessageCallback,
    on_open_file: FileOpenCallback,
    date_format_cell: Rc<std::cell::Cell<DateFormat>>,
    files_limit: Rc<std::cell::Cell<u32>>,
) -> CommitListRefs {
    let store = gio::ListStore::new::<CommitObject>();
    let filter = gtk::CustomFilter::new(|_| true);
    let filter_model = gtk::FilterListModel::new(Some(store.clone()), Some(filter.clone()));
    let selection = gtk::SingleSelection::new(Some(filter_model.clone()));
    selection.set_can_unselect(false);

    let factory = build_row_factory(
        on_load_more.clone(),
        on_edit_head_message.clone(),
        on_open_file,
        date_format_cell,
        files_limit,
    );

    let list_view = gtk::ListView::new(Some(selection.clone()), Some(factory));
    list_view.add_css_class("navigation-sidebar");
    list_view.set_vexpand(true);
    // Single click activates a row (toggles its detail revealer), matching the
    // ListBox behaviour this list replaced. Without it `ListView` only
    // activates on double-click and clicking a commit appears to do nothing.
    list_view.set_single_click_activate(true);

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

/// Mark commits as signed from a background lookup's results.
///
/// Returns the ids whose flag actually changed, so the caller rebinds only
/// those rows.
pub fn apply_signature_flags(store: &gio::ListStore, flags: &[(String, bool)]) -> Vec<String> {
    let mut changed = Vec::new();
    for i in 0..store.n_items() {
        let Some(obj) = store.item(i).and_then(|o| o.downcast::<CommitObject>().ok()) else {
            continue;
        };
        if obj.is_load_more_sentinel() {
            continue;
        }
        let id = obj.id();
        if flags
            .iter()
            .any(|(flagged, signed)| *signed && *flagged == id)
            && !obj.is_signed()
        {
            obj.set_is_signed(true);
            changed.push(id);
        }
    }
    changed
}

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

fn build_row_factory(
    on_load_more: LoadMoreCallback,
    on_edit_head_message: EditMessageCallback,
    on_open_file: FileOpenCallback,
    date_format_cell: Rc<std::cell::Cell<DateFormat>>,
    files_limit: Rc<std::cell::Cell<u32>>,
) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    let lm = on_load_more.clone();
    let em = on_edit_head_message.clone();
    let of = on_open_file.clone();
    let fl = files_limit.clone();
    factory.connect_setup(move |_, list_item| {
        let item = list_item
            .downcast_ref::<gtk::ListItem>()
            .expect("ListItem");
        let outer = build_row_template(lm.clone(), em.clone(), of.clone(), fl.clone());
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

fn build_row_template(
    on_load_more: LoadMoreCallback,
    on_edit_head_message: EditMessageCallback,
    on_open_file: FileOpenCallback,
    files_limit: Rc<std::cell::Cell<u32>>,
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

    // Click anywhere on the sentinel row triggers Load more. Rows are recycled,
    // so the same controller sits on ordinary commit rows too — there it must
    // stay out of the way: a claimed sequence would swallow the click before
    // `ListView` can activate the row.
    let click = gtk::GestureClick::new();
    let lm = on_load_more.clone();
    let outer_weak = outer.downgrade();
    let outer_weak_press = outer.downgrade();
    click.connect_pressed(move |gesture, _, _, _| {
        let is_sentinel = outer_weak_press
            .upgrade()
            .is_some_and(|outer| outer.widget_name() == "sentinel");
        if !is_sentinel {
            gesture.set_state(gtk::EventSequenceState::Denied);
        }
    });
    click.connect_released(move |_, _, _, _| {
        let Some(outer) = outer_weak.upgrade() else {
            return;
        };
        if outer.widget_name() == "sentinel" {
            lm();
        }
    });
    outer.add_controller(click);

    // Cache the handles bind needs — otherwise every bind walks the row's
    // widget tree once per widget it touches.
    unsafe {
        outer.set_data(
            ROW_WIDGETS_KEY,
            RowWidgets {
                summary_row: row_box,
                msg_row,
                hash_label,
                message_label,
                meta_label,
                signed_icon,
                edit_btn,
                detail_revealer,
                files_box,
                sentinel_label,
                on_open_file,
                files_limit,
            },
        );
    }

    outer
}

/// Widget handles cached on each row template at construction time.
struct RowWidgets {
    summary_row: gtk::Box,
    msg_row: gtk::Box,
    hash_label: gtk::Label,
    message_label: gtk::Label,
    meta_label: gtk::Label,
    signed_icon: gtk::Image,
    edit_btn: gtk::Button,
    detail_revealer: gtk::Revealer,
    files_box: gtk::Box,
    sentinel_label: gtk::Label,
    on_open_file: FileOpenCallback,
    files_limit: Rc<std::cell::Cell<u32>>,
}

/// Key under which [`RowWidgets`] is attached to a row's outer Box.
const ROW_WIDGETS_KEY: &str = "gp-commit-row-widgets";

/// Read back the handles stashed by `build_row_template`.
fn row_widgets(outer: &gtk::Box) -> Option<&RowWidgets> {
    // Safety: set once in `build_row_template`, never replaced, and owned by
    // the widget for its whole life. Only ever read through a shared reference
    // whose lifetime this signature ties to `outer`.
    unsafe { outer.data::<RowWidgets>(ROW_WIDGETS_KEY).map(|p| p.as_ref()) }
}

/// Find the realized row widget currently showing `commit_id`, if any.
///
/// Rows are only realized while in (or near) the viewport, so this returns
/// `None` for commits scrolled out of view — callers must keep the state on the
/// [`CommitObject`] as well, which `bind_row` reapplies when the row scrolls
/// back in.
pub fn find_row_outer(list_view: &gtk::ListView, commit_id: &str) -> Option<gtk::Box> {
    // Iterative walk: the row tree is shallow but unbounded in principle.
    let mut stack: Vec<gtk::Widget> = list_view.first_child().into_iter().collect();
    while let Some(widget) = stack.pop() {
        if let Ok(b) = widget.clone().downcast::<gtk::Box>() {
            if b.has_css_class(ROW_OUTER_CSS) && b.widget_name() == commit_id {
                return Some(b);
            }
        }
        if let Some(sibling) = widget.next_sibling() {
            stack.push(sibling);
        }
        if let Some(child) = widget.first_child() {
            stack.push(child);
        }
    }
    None
}

/// Re-apply a [`CommitObject`]'s state onto its realized row.
///
/// `GtkListView` will not re-bind a row when the object at that position is
/// unchanged, so emitting `items_changed` after mutating a `CommitObject` is a
/// no-op. Callers that change row state push it through here instead.
pub fn rebind_row(outer: &gtk::Box, obj: &CommitObject, date_format: DateFormat) {
    bind_row(outer, obj, date_format);
}

fn bind_row(outer: &gtk::Box, obj: &CommitObject, date_format: DateFormat) {
    let Some(widgets) = row_widgets(outer) else {
        return;
    };

    if obj.is_load_more_sentinel() {
        outer.set_widget_name("sentinel");
        widgets.summary_row.set_visible(false);
        widgets.detail_revealer.set_visible(false);
        widgets.sentinel_label.set_visible(true);
        return;
    }

    outer.set_widget_name(&obj.id());
    // Stash full message for the edit-button closure to read.
    unsafe {
        outer.set_data::<String>("commit-message", obj.message());
    }

    widgets.summary_row.set_visible(true);
    widgets.detail_revealer.set_visible(true);
    widgets.sentinel_label.set_visible(false);

    widgets.hash_label.set_label(&obj.short_id());
    widgets.message_label.set_label(&obj.summary());
    let when = format_relative_time(obj.time_unix(), date_format);
    widgets
        .meta_label
        .set_label(&format!("{} {}", obj.author().name, when));
    widgets.signed_icon.set_visible(obj.is_signed());
    widgets.edit_btn.set_visible(obj.is_head());

    // Tag badges live inside msg-row after the message label. Clear and re-add.
    let msg_row = &widgets.msg_row;
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

    // Detail visibility + file list — populated lazily; window.rs sets
    // files via CommitObject and re-binds by toggling expanded.
    widgets.detail_revealer.set_reveal_child(obj.expanded());
    if obj.expanded() {
        // Render whatever's currently in obj.files() — window.rs handles the
        // async fetch and re-binds via store.items_changed(idx, 1, 1).
        populate_files_into_outer(outer, obj);
    }
}

fn populate_files_into_outer(outer: &gtk::Box, obj: &CommitObject) {
    let Some(files_box) = row_widgets(outer).map(|w| w.files_box.clone()) else {
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

    let Some((on_open, limit)) =
        row_widgets(outer).map(|w| (w.on_open_file.clone(), w.files_limit.get()))
    else {
        return;
    };

    // The `commit_files_limit` preference stopped being honoured during the
    // ListView migration; a commit touching hundreds of files built hundreds of
    // widgets inside one row.
    let shown = files.len().min(limit.max(1) as usize);
    let commit_id = obj.id();
    for f in &files[..shown] {
        files_box.append(&build_file_row(f, &commit_id, on_open.clone()));
    }
    if shown < files.len() {
        let more = gtk::Label::builder()
            .label(format!("… and {} more files", files.len() - shown))
            .css_classes(["caption", "dim-label"])
            .xalign(0.0)
            .margin_start(4)
            .margin_top(2)
            .build();
        files_box.append(&more);
    }
}

/// One file inside an expanded commit. Clicking it opens the diff dialog —
/// the diff used to expand inline here, inside a `ListView` row, which
/// scrolled unpredictably and often refused to collapse again.
fn build_file_row(file: &DiffFile, commit_id: &str, on_open: FileOpenCallback) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let file_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    file_row.set_margin_start(4);

    // Status icon
    let (icon_name, icon_class) = diff_file_icon(file);
    let icon = gtk::Image::builder()
        .icon_name(icon_name)
        .css_classes([icon_class])
        .pixel_size(14)
        .build();
    file_row.append(&icon);

    // File path
    let path_label = gtk::Label::builder()
        .label(&file.path)
        .css_classes(["caption"])
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::Start)
        .hexpand(true)
        .build();
    path_label.set_widget_name(&file.path);
    file_row.append(&path_label);

    // Stats
    let stats_text = format!("+{} -{}", file.stats.insertions, file.stats.deletions);
    let stats_label = gtk::Label::builder()
        .label(&stats_text)
        .css_classes(["caption", "dim-label", "monospace"])
        .build();
    file_row.append(&stats_label);

    let header_btn = gtk::Button::builder()
        .child(&file_row)
        .css_classes(["flat"])
        .tooltip_text("Show diff")
        .build();
    let path = file.path.clone();
    let commit_id = commit_id.to_string();
    header_btn.connect_clicked(move |_| on_open(&commit_id, &path));
    container.append(&header_btn);

    container
}

/// Icon name and CSS class for a diff file status.
fn diff_file_icon(file: &DiffFile) -> (&'static str, &'static str) {
    let has_additions = file.stats.insertions > 0;
    let has_deletions = file.stats.deletions > 0;

    if has_additions && !has_deletions {
        ("list-add-symbolic", "success")
    } else if has_deletions && !has_additions {
        ("list-remove-symbolic", "error")
    } else {
        ("document-edit-symbolic", "accent")
    }
}

fn find_child_by_name(widget: &gtk::Box, name: &str) -> Option<gtk::Widget> {
    let mut child = widget.first_child();
    while let Some(c) = child {
        if c.widget_name() == name {
            return Some(c);
        }
        // Recurse into boxes
        if let Ok(inner_box) = c.clone().downcast::<gtk::Box>() {
            if let Some(found) = find_child_by_name(&inner_box, name) {
                return Some(found);
            }
        }
        // Recurse into revealers
        if let Ok(revealer) = c.clone().downcast::<gtk::Revealer>() {
            if let Some(rev_child) = revealer.child() {
                if rev_child.widget_name() == name {
                    return Some(rev_child);
                }
                if let Ok(inner_box) = rev_child.downcast::<gtk::Box>() {
                    if let Some(found) = find_child_by_name(&inner_box, name) {
                        return Some(found);
                    }
                }
            }
        }
        child = c.next_sibling();
    }
    None
}

fn format_relative_time(time_unix: i64, date_format: DateFormat) -> String {
    let time = chrono::Utc
        .timestamp_opt(time_unix, 0)
        .single()
        .unwrap_or_default();
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(time);

    if duration.num_minutes() < 1 {
        "just now".to_string()
    } else if duration.num_hours() < 1 {
        format!("{} min ago", duration.num_minutes())
    } else if duration.num_days() < 1 {
        format!("{}h ago", duration.num_hours())
    } else if duration.num_weeks() < 1 {
        format!("{}d ago", duration.num_days())
    } else if duration.num_weeks() < 5 {
        format!("{}w ago", duration.num_weeks())
    } else {
        date_format.format_date(&time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serial_test::serial;

    use crate::test_support;

    fn sample_commit() -> CommitInfo {
        use gitpulsar_core::models::Signature;
        let who = Signature {
            name: "Test".into(),
            email: "t@e.com".into(),
        };
        CommitInfo {
            id: "5a6607e2a7e4da9d8dc393d87d7e54a67817d24f".into(),
            short_id: "5a6607e".into(),
            summary: "a commit".into(),
            message: "a commit\n\nbody".into(),
            author: who.clone(),
            committer: who,
            time: chrono::Utc
                .timestamp_opt(1_700_000_000, 0)
                .single()
                .unwrap_or_default(),
            parent_ids: vec![],
            is_signed: false,
        }
    }

    fn build_for_test() -> CommitListRefs {
        build_commit_list_view(
            Rc::new(|_| {}),
            Rc::new(|| {}),
            Rc::new(|_| {}),
            Rc::new(|_, _| {}),
            Rc::new(std::cell::Cell::new(DateFormat::Iso)),
            Rc::new(std::cell::Cell::new(10u32)),
        )
    }

    /// Regression guard: the ListBox this list replaced activated rows on a
    /// single click. `gtk::ListView` defaults to double-click activation, so
    /// the property must be set explicitly or clicking a commit does nothing.
    #[test]
    #[serial]
    fn rows_activate_on_single_click() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }

        let single_click =
            test_support::on_gtk_thread(|| build_for_test().list_view.is_single_click_activate());
        assert!(single_click);
    }

    /// The signature lookup runs off the UI thread and comes back as a list of
    /// ids; it must land only on the commits it names.
    #[test]
    #[serial]
    fn signature_flags_land_on_matching_commits() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }

        let (changed, signed, items) = test_support::on_gtk_thread(|| {
            let store = gio::ListStore::new::<CommitObject>();
            let obj = CommitObject::from_info(&sample_commit(), vec![], true, false);
            store.append(&obj);
            store.append(&CommitObject::load_more_sentinel());

            let changed = apply_signature_flags(
                &store,
                &[(obj.id(), true), ("no-such-commit".to_string(), true)],
            );
            (changed, obj.is_signed(), store.n_items())
        });

        assert_eq!(changed.len(), 1, "only the matching commit is reported");
        assert!(signed);
        assert_eq!(items, 2, "unknown ids must not touch the store");
    }

    fn sample_diff_file(path: &str) -> DiffFile {
        DiffFile {
            path: path.to_string(),
            hunks: vec![],
            stats: gitpulsar_core::models::DiffStats {
                insertions: 1,
                deletions: 0,
            },
        }
    }

    fn count_children(container: &gtk::Box) -> usize {
        let mut n = 0;
        let mut child = container.first_child();
        while let Some(c) = child {
            n += 1;
            child = c.next_sibling();
        }
        n
    }

    /// Clicking a file inside a commit must report which file, so the window can
    /// open the diff dialog on it.
    #[test]
    #[serial]
    fn file_row_click_reports_its_path() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }

        let seen = test_support::on_gtk_thread(|| {
            let seen: Rc<std::cell::RefCell<Vec<(String, String)>>> = Rc::default();
            let sink = seen.clone();
            let cb: FileOpenCallback = Rc::new(move |commit, path| {
                sink.borrow_mut().push((commit.to_string(), path.to_string()));
            });

            let row = build_file_row(&sample_diff_file("src/main.rs"), "abc123", cb);
            let btn = row
                .first_child()
                .and_then(|c| c.downcast::<gtk::Button>().ok())
                .expect("row's child is the clickable header");
            btn.emit_clicked();

            let out = seen.borrow().clone();
            out
        });

        assert_eq!(
            seen,
            vec![("abc123".to_string(), "src/main.rs".to_string())]
        );
    }

    /// `commit_files_limit` is a live preference that the ListView migration
    /// stopped honouring — a commit row rendered every file it touched.
    #[test]
    #[serial]
    fn file_list_respects_the_configured_cap() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }

        let children = test_support::on_gtk_thread(|| {
            let outer = build_row_template(
                Rc::new(|| {}),
                Rc::new(|_| {}),
                Rc::new(|_, _| {}),
                Rc::new(std::cell::Cell::new(2u32)),
            );
            let obj = CommitObject::from_info(&sample_commit(), vec![], true, false);
            obj.set_files(vec![
                sample_diff_file("a.rs"),
                sample_diff_file("b.rs"),
                sample_diff_file("c.rs"),
            ]);
            obj.set_files_loaded(true);
            obj.set_expanded(true);
            rebind_row(&outer, &obj, DateFormat::Iso);

            let files_box = row_widgets(&outer)
                .map(|w| w.files_box.clone())
                .expect("files box");
            count_children(&files_box)
        });

        assert_eq!(children, 3, "two file rows plus the overflow note");
    }

    /// Expanding a commit mutates its `CommitObject` in place, which
    /// `GtkListView` does not notice — the window pushes the change onto the row
    /// via `rebind_row`. Guard that this actually reveals the detail pane.
    #[test]
    #[serial]
    fn rebind_reveals_expanded_detail() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }

        let (revealed_when_expanded, revealed_when_collapsed) = test_support::on_gtk_thread(|| {
            let row = build_row_template(
                Rc::new(|| {}),
                Rc::new(|_| {}),
                Rc::new(|_, _| {}),
                Rc::new(std::cell::Cell::new(10u32)),
            );
            let obj = CommitObject::from_info(&sample_commit(), vec![], true, false);

            obj.set_expanded(true);
            rebind_row(&row, &obj, DateFormat::Iso);
            let expanded = row_widgets(&row)
                .map(|w| w.detail_revealer.reveals_child())
                .unwrap_or(false);

            obj.set_expanded(false);
            rebind_row(&row, &obj, DateFormat::Iso);
            let collapsed = row_widgets(&row)
                .map(|w| w.detail_revealer.reveals_child())
                .unwrap_or(true);

            (expanded, collapsed)
        });

        assert!(revealed_when_expanded, "expanded commit must reveal detail");
        assert!(!revealed_when_collapsed, "collapsing must hide detail again");
    }
}
