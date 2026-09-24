use std::rc::Rc;
use std::collections::HashSet;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::model::{DiffFile, DiffLineKind, FileStatusKind, RepoStatus};

use super::changed_file_object::ChangedFileObject;
use super::syntax;
use crate::i18n::{self, Key};

/// Per-row button callback: `(file_path, button_name)`.
pub type RowButtonCallback = Rc<dyn Fn(&str, &str)>;

/// CSS class marker applied to each row's outer Box so the button click
/// handlers (which are wired once during factory setup) can walk up the widget
/// tree to find the currently-bound row and read its file path.
const ROW_OUTER_CSS: &str = "gp-file-row";

/// Refs returned to window.rs for connecting signals.
pub struct ChangesViewRefs {
    pub list_view: gtk::ListView,
    pub store: gio::ListStore,
    /// Kept so the window can attach a per-row render callback after setup.
    pub factory: gtk::SignalListItemFactory,
    pub stage_all_btn: gtk::Button,
    pub unstage_all_btn: gtk::Button,
    pub trash_all_btn: gtk::Button,
    /// Optional extras (commit-prefix template + co-author trailer).
    /// Hidden on narrow widths to make room for the commit button.
    pub template_btn: gtk::MenuButton,
    pub coauthor_btn: gtk::MenuButton,
}

/// Build the changes tab content.
/// Top: compact action bar (Stage All | Unstage All | Commit msg | Commit btn)
/// Below: scrollable list of file accordion rows.
pub fn build_changes_view(
    commit_entry: &gtk::TextView,
    commit_button: &gtk::Button,
    amend_check: &gtk::CheckButton,
    allow_empty_check: &gtk::CheckButton,
) -> (gtk::Box, ChangesViewRefs) {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    container.set_vexpand(true);

    // === Compact commit bar at top ===
    let commit_bar = gtk::Box::new(gtk::Orientation::Vertical, 4);
    commit_bar.set_margin_start(8);
    commit_bar.set_margin_end(8);
    commit_bar.set_margin_top(8);
    commit_bar.set_margin_bottom(4);

    // Commit message entry
    commit_entry.set_wrap_mode(gtk::WrapMode::Word);
    commit_entry.set_top_margin(6);
    commit_entry.set_bottom_margin(6);
    commit_entry.set_left_margin(8);
    commit_entry.set_right_margin(8);
    commit_entry.set_height_request(56);
    commit_entry.add_css_class("card");

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(commit_entry));

    let placeholder_label = gtk::Label::builder()
        .label(i18n::t(Key::commit_message_placeholder))
        .css_classes(["dim-label"])
        .xalign(0.0)
        .yalign(0.0)
        .margin_start(12)
        .margin_top(8)
        .can_focus(false)
        .can_target(false)
        .valign(gtk::Align::Start)
        .halign(gtk::Align::Start)
        .build();
    overlay.add_overlay(&placeholder_label);

    let pl = placeholder_label.clone();
    commit_entry.buffer().connect_changed(move |buf| {
        pl.set_visible(buf.char_count() == 0);
    });

    commit_bar.append(&overlay);

    // Action row: Stage All | Unstage All | [spacer] | Amend | Commit
    let action_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);

    let stage_all_btn = gtk::Button::builder()
        .label(i18n::t(Key::stage_all))
        .css_classes(["flat", "caption"])
        .build();
    action_row.append(&stage_all_btn);

    let unstage_all_btn = gtk::Button::builder()
        .label(i18n::t(Key::unstage_all))
        .css_classes(["flat", "caption"])
        .build();
    action_row.append(&unstage_all_btn);

    let trash_all_btn = gtk::Button::builder()
        .label(i18n::t(Key::trash_all))
        .css_classes(["flat", "caption"])
        .tooltip_text(i18n::t(Key::trash_all_tooltip))
        .build();
    action_row.append(&trash_all_btn);

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    action_row.append(&spacer);

    // Conventional commit prefix button
    let commit_entry_for_template = commit_entry.clone();
    let template_btn = super::commit_templates::build_template_button(move |prefix| {
        super::commit_templates::insert_prefix(&commit_entry_for_template.buffer(), prefix);
    });
    action_row.append(&template_btn);

    // Co-Author trailer button
    let commit_entry_for_coauthor = commit_entry.clone();
    let coauthor_btn = super::commit_templates::build_coauthor_button(move |name, email| {
        super::commit_templates::append_coauthor(&commit_entry_for_coauthor.buffer(), name, email);
    });
    action_row.append(&coauthor_btn);

    action_row.append(amend_check);

    allow_empty_check.set_tooltip_text(Some("Allow commit when no files are staged"));
    action_row.append(allow_empty_check);

    commit_button.set_label(i18n::t(Key::commit_btn));
    commit_button.add_css_class("suggested-action");
    commit_button.add_css_class("pill");
    action_row.append(commit_button);

    commit_bar.append(&action_row);
    container.append(&commit_bar);

    container.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // === Scrollable area with single unified file list ===
    let lists_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let header = gtk::Label::builder()
        .label(i18n::t(Key::changes))
        .css_classes(["heading"])
        .xalign(0.0)
        .margin_start(8)
        .margin_top(6)
        .margin_bottom(2)
        .build();
    header.set_widget_name("changes-header");
    lists_box.append(&header);

    // Virtualized list: only viewport rows are realized. Custom GObject model
    // holds per-entry state (path/status/staged + expanded flag) so it survives
    // factory recycling.
    let store = gio::ListStore::new::<ChangedFileObject>();

    // Empty-state label shown when the store has no items.
    let placeholder = gtk::Label::builder()
        .label(i18n::t(Key::no_changes))
        .css_classes(["dim-label"])
        .margin_top(12)
        .margin_bottom(12)
        .visible(false)
        .build();
    placeholder.set_widget_name("changes-placeholder");
    {
        let placeholder = placeholder.clone();
        store.connect_items_changed(move |s, _, _, _| {
            placeholder.set_visible(s.n_items() == 0);
        });
    }
    placeholder.set_visible(true);
    lists_box.append(&placeholder);

    let selection = gtk::NoSelection::new(Some(store.clone()));
    let factory = build_row_factory();
    let list_view = gtk::ListView::builder()
        .model(&selection)
        .factory(&factory)
        // Single click activates a row (toggles its diff revealer), matching
        // the previous ListBox behaviour.
        .single_click_activate(true)
        .css_classes(["navigation-sidebar"])
        .build();

    attach_row_hover_controller(&list_view);
    lists_box.append(&list_view);

    let file_scrolled = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    file_scrolled.set_child(Some(&lists_box));
    container.append(&file_scrolled);

    let refs = ChangesViewRefs {
        list_view,
        store,
        factory,
        stage_all_btn,
        unstage_all_btn,
        trash_all_btn,
        template_btn,
        coauthor_btn,
    };

    (container, refs)
}

/// An entry representing a changed file for the accordion list.
pub struct ChangedFileEntry {
    pub path: String,
    pub status: FileStatusKind,
    pub is_staged: bool,
}

/// Collect all changed files from RepoStatus into a flat list.
pub fn collect_changed_files(status: &RepoStatus) -> Vec<ChangedFileEntry> {
    let mut files = Vec::new();

    for f in &status.unstaged {
        files.push(ChangedFileEntry {
            path: f.path.clone(),
            status: f.status,
            is_staged: false,
        });
    }

    for p in &status.untracked {
        files.push(ChangedFileEntry {
            path: p.clone(),
            status: FileStatusKind::New,
            is_staged: false,
        });
    }

    for f in &status.staged {
        files.push(ChangedFileEntry {
            path: f.path.clone(),
            status: f.status,
            is_staged: true,
        });
    }

    files
}

/// Wire `connect_clicked` for all hunk-action buttons in `hunk_box` (stage/unstage hunks
/// and stage-selected-lines). Must be called after `populate_hunk_actions`.
/// Callback signature: `(button_name)`.
pub fn wire_hunk_button_signals<F>(hunk_box: &gtk::Box, callback: F)
where
    F: Fn(&str) + 'static + Clone,
{
    walk_buttons(hunk_box.upcast_ref(), &mut |btn| {
        let name = btn.widget_name().to_string();
        if name.starts_with("stage-hunk-")
            || name.starts_with("unstage-hunk-")
            || name == "stage-selected-lines"
            || name == "unstage-selected-lines"
        {
            let cb = callback.clone();
            btn.connect_clicked(move |b| cb(&b.widget_name()));
        }
    });
}

fn walk_buttons(parent: &gtk::Widget, cb: &mut dyn FnMut(&gtk::Button)) {
    let mut child = parent.first_child();
    while let Some(c) = child {
        if let Ok(btn) = c.clone().downcast::<gtk::Button>() {
            cb(&btn);
        } else {
            walk_buttons(&c, cb);
        }
        child = c.next_sibling();
    }
}

/// Replace the contents of the file-row store with the given entries. Performs
/// a single `splice` so only viewport items are realized; preserves nothing
/// from the previous state.
///
/// The factory wires its own per-row click handlers, so `on_button` is stored
/// on the factory via [`set_row_button_callback`] and not consumed here.
pub fn populate_file_lists(
    store: &gio::ListStore,
    files: &[ChangedFileEntry],
    list_view: &gtk::ListView,
) {
    let unstaged_count = files.iter().filter(|f| !f.is_staged).count();
    let staged_count = files.iter().filter(|f| f.is_staged).count();
    let label = if staged_count == 0 && unstaged_count == 0 {
        i18n::t(Key::changes).to_string()
    } else {
        crate::i18n::FmtKey::commit_files_staged_unstaged(staged_count, unstaged_count).render()
    };
    update_header_label(list_view, "changes-header", &label);

    // Carry expansion across the rebuild: `splice` replaces every object, and
    // a fresh object would silently collapse rows the user had open — the
    // "refresh loses my place" bug class (see GOAL 5.1).
    let mut expanded: HashSet<String> = HashSet::new();
    for i in 0..store.n_items() {
        if let Some(o) = store.item(i).and_then(|x| x.downcast::<ChangedFileObject>().ok()) {
            if o.expanded() {
                expanded.insert(o.path());
            }
        }
    }

    // Sort staged first, then unstaged, preserving original order within each group.
    let mut ordered: Vec<&ChangedFileEntry> = files.iter().filter(|f| f.is_staged).collect();
    ordered.extend(files.iter().filter(|f| !f.is_staged));

    let objects: Vec<glib::Object> = ordered
        .into_iter()
        .map(|f| {
            let o = ChangedFileObject::new(f.path.clone(), f.status, f.is_staged);
            if expanded.contains(&f.path) {
                o.set_expanded(true);
            }
            o.upcast()
        })
        .collect();
    store.splice(0, store.n_items(), &objects);
}

/// Attach the row-button callback to the ListView so the factory's setup
/// closure (registered on the buttons once) can fire it with the bound path.
pub fn set_row_button_callback(list_view: &gtk::ListView, on_button: RowButtonCallback) {
    unsafe {
        list_view.set_data::<RowButtonCallback>(ROW_CB_KEY, on_button);
    }
}

pub const ROW_CB_KEY: &str = "gp-row-button-cb";

/// Per-row render callback: re-renders a row's diff pane when the row is
/// (re)bound already-expanded. `populate_file_lists` splices the whole store
/// on every refresh and `connect_unbind` clears the pane, so without this an
/// open diff would come back empty after any background refresh.
pub type RowRenderCallback = Rc<dyn Fn(&gtk::Box, &str)>;

const ROW_RENDER_KEY: &str = "gp-row-render-cb";

/// Store the render callback on the *factory* (not the ListView): the factory
/// closure needs it at bind time, before the row has any parent to walk up.
pub fn set_row_render_callback(factory: &gtk::SignalListItemFactory, on_render: RowRenderCallback) {
    unsafe {
        factory.set_data::<RowRenderCallback>(ROW_RENDER_KEY, on_render);
    }
}

fn list_view_for_widget(widget: &gtk::Widget) -> Option<gtk::ListView> {
    let mut cur = widget.parent();
    while let Some(w) = cur {
        if let Ok(lv) = w.clone().downcast::<gtk::ListView>() {
            return Some(lv);
        }
        cur = w.parent();
    }
    None
}

/// Walk up from a button to find the row outer Box (marked with `ROW_OUTER_CSS`).
fn outer_box_from_button(btn: &gtk::Button) -> Option<gtk::Box> {
    let mut cur = btn.parent();
    while let Some(w) = cur {
        if let Ok(b) = w.clone().downcast::<gtk::Box>() {
            if b.has_css_class(ROW_OUTER_CSS) {
                return Some(b);
            }
        }
        cur = w.parent();
    }
    None
}

fn invoke_row_button(btn: &gtk::Button, action: &str) {
    let Some(outer) = outer_box_from_button(btn) else { return };
    let path = outer.widget_name().to_string();
    if path.is_empty() {
        return;
    }
    let Some(list_view) = list_view_for_widget(outer.upcast_ref()) else { return };
    let cb = unsafe { list_view.data::<RowButtonCallback>(ROW_CB_KEY) };
    if let Some(ptr) = cb {
        let cb_ref: &RowButtonCallback = unsafe { ptr.as_ref() };
        cb_ref(&path, action);
    }
}

/// Build the factory that creates a row template once and re-binds it to each
/// `ChangedFileObject` as it scrolls into view.
fn build_row_factory() -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    factory.connect_setup(|_, list_item| {
        let item = list_item.downcast_ref::<gtk::ListItem>().expect("ListItem");
        let outer = build_row_template();
        item.set_child(Some(&outer));
        item.set_activatable(true);
        // Bind expand state both ways via property: ListItem activation flips
        // a Cell on the outer Box that toggle_file_diff_outer reads.
    });

    factory.connect_bind(|factory, list_item| {
        let item = list_item.downcast_ref::<gtk::ListItem>().expect("ListItem");
        let outer = item
            .child()
            .and_then(|c| c.downcast::<gtk::Box>().ok())
            .expect("row Box");
        let obj = item
            .item()
            .and_then(|o| o.downcast::<ChangedFileObject>().ok())
            .expect("ChangedFileObject");
        bind_row_widgets(&outer, &obj);

        // A row bound already-expanded is one restored across a refresh whose
        // pane `connect_unbind` just cleared — re-render it from fresh caches.
        if obj.expanded() {
            let ptr = unsafe { factory.data::<RowRenderCallback>(ROW_RENDER_KEY) };
            if let Some(ptr) = ptr {
                let cb = unsafe { ptr.as_ref() };
                cb(&outer, &obj.path());
            }
        }
    });

    factory.connect_unbind(|_, list_item| {
        let item = list_item.downcast_ref::<gtk::ListItem>().expect("ListItem");
        if let Some(outer) = item.child().and_then(|c| c.downcast::<gtk::Box>().ok()) {
            // Collapse the diff revealer + clear lazy state so a recycled row
            // doesn't show stale content for the next item it's bound to.
            if let Some(rev) = find_child_by_name(&outer, "diff-box").and_then(|w| w.downcast::<gtk::Revealer>().ok()) {
                rev.set_reveal_child(false);
                if let Some(child) = rev.child().and_then(|c| c.downcast::<gtk::Box>().ok()) {
                    if let Some(tv) = find_child_by_name(&child, "diff-textview") {
                        child.remove(&tv);
                    }
                    if let Some(ha) = find_child_by_name(&child, "hunk-actions-box").and_then(|w| w.downcast::<gtk::Box>().ok()) {
                        while let Some(c) = ha.first_child() {
                            ha.remove(&c);
                        }
                    }
                }
            }
            outer.set_widget_name("");
        }
    });

    factory
}

/// Build an empty row widget tree without binding any file data. Called once
/// per recycled row by the factory's setup closure.
fn build_row_template() -> gtk::Box {
    let outer_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    outer_box.add_css_class(ROW_OUTER_CSS);

    // === Header row ===
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    header.set_margin_start(8);
    header.set_margin_end(4);
    header.set_margin_top(4);
    header.set_margin_bottom(4);

    let expand_icon = gtk::Image::builder()
        .icon_name("pan-end-symbolic")
        .css_classes(["dim-label"])
        .build();
    expand_icon.set_widget_name("expand-icon");
    header.append(&expand_icon);

    let status_icon = gtk::Image::builder()
        .icon_name("text-x-generic-symbolic")
        .pixel_size(14)
        .build();
    status_icon.set_widget_name("status-icon");
    header.append(&status_icon);

    let path_label = gtk::Label::builder()
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::Start)
        .css_classes(["caption"])
        .build();
    path_label.set_widget_name("path-label");
    header.append(&path_label);

    // Action buttons (shown on hover via ListView-level controller)
    let btn_box = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    btn_box.set_widget_name("row-actions");
    btn_box.set_visible(false);

    let action_btn = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .width_request(36)
        .height_request(36)
        .build();
    action_btn.set_widget_name("action-btn");
    action_btn.connect_clicked(|b| invoke_row_button(b, &b.widget_name()));
    btn_box.append(&action_btn);

    let discard_btn = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .css_classes(["flat", "circular"])
        .tooltip_text(i18n::t(Key::discard))
        .valign(gtk::Align::Center)
        .build();
    discard_btn.set_widget_name("discard-file");
    discard_btn.connect_clicked(|b| invoke_row_button(b, "discard-file"));
    btn_box.append(&discard_btn);

    let blame_btn = gtk::Button::builder()
        .icon_name("view-list-symbolic")
        .css_classes(["flat", "circular"])
        .tooltip_text(i18n::t(Key::blame))
        .valign(gtk::Align::Center)
        .build();
    blame_btn.set_widget_name("blame-file");
    blame_btn.connect_clicked(|b| invoke_row_button(b, "blame-file"));
    btn_box.append(&blame_btn);

    let history_btn = gtk::Button::builder()
        .icon_name("document-open-recent-symbolic")
        .css_classes(["flat", "circular"])
        .tooltip_text(i18n::t(Key::file_history))
        .valign(gtk::Align::Center)
        .build();
    history_btn.set_widget_name("history-file");
    history_btn.connect_clicked(|b| invoke_row_button(b, "history-file"));
    btn_box.append(&history_btn);

    header.append(&btn_box);
    outer_box.append(&header);

    // Diff revealer (collapsed by default)
    let diff_revealer = gtk::Revealer::builder()
        .reveal_child(false)
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .transition_duration(200)
        .build();
    diff_revealer.set_widget_name("diff-box");

    let diff_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    diff_box.set_margin_start(16);
    diff_box.set_margin_end(8);
    diff_box.set_margin_bottom(4);

    let hunk_actions_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    hunk_actions_box.set_widget_name("hunk-actions-box");
    diff_box.append(&hunk_actions_box);

    diff_revealer.set_child(Some(&diff_box));
    outer_box.append(&diff_revealer);

    // Cache the handles bind needs. Without this every bind walked the row's
    // widget tree once per widget, and the walk was fragile besides: bind
    // renames `action-btn` to `stage-file`/`unstage-file`, so looking it up by
    // its template name missed on every re-bind of a recycled row.
    unsafe {
        outer_box.set_data(
            ROW_WIDGETS_KEY,
            RowWidgets {
                expand_icon,
                status_icon,
                path_label,
                action_btn,
                discard_btn,
                revealer: diff_revealer,
            },
        );
    }

    outer_box
}

/// Widget handles cached on each row template at construction time.
struct RowWidgets {
    expand_icon: gtk::Image,
    status_icon: gtk::Image,
    path_label: gtk::Label,
    action_btn: gtk::Button,
    discard_btn: gtk::Button,
    revealer: gtk::Revealer,
}

/// Key under which [`RowWidgets`] is attached to a row's outer Box.
const ROW_WIDGETS_KEY: &str = "gp-row-widgets";

/// Read back the handles stashed by `build_row_template`.
fn row_widgets(outer: &gtk::Box) -> Option<&RowWidgets> {
    // Safety: the value is set once in `build_row_template`, never replaced,
    // and lives as long as the widget. Callers only read through the shared
    // reference, whose lifetime is tied to `outer` by this signature.
    unsafe { outer.data::<RowWidgets>(ROW_WIDGETS_KEY).map(|p| p.as_ref()) }
}

/// Apply the data from `obj` onto the per-row widgets created by `build_row_template`.
fn bind_row_widgets(outer: &gtk::Box, obj: &ChangedFileObject) {
    outer.set_widget_name(&obj.path());

    let Some(widgets) = row_widgets(outer) else {
        return;
    };

    let is_staged = obj.is_staged();
    let status = obj.status();
    let path = obj.path();

    // Status icon — green check when staged, otherwise per-status glyph
    let (name, css, tooltip) = if is_staged {
        let kind = match status {
            FileStatusKind::New => i18n::t(Key::status_added),
            FileStatusKind::Modified => i18n::t(Key::status_modified),
            FileStatusKind::Deleted => i18n::t(Key::status_deleted),
            FileStatusKind::Renamed => i18n::t(Key::status_renamed),
            FileStatusKind::Typechange => i18n::t(Key::status_typechange),
        };
        ("object-select-symbolic", "success", kind.to_string())
    } else {
        let (n, c, t) = match status {
            FileStatusKind::New => ("list-add-symbolic", "success", i18n::t(Key::status_added)),
            FileStatusKind::Modified => ("document-edit-symbolic", "accent", i18n::t(Key::status_modified)),
            FileStatusKind::Deleted => ("list-remove-symbolic", "error", i18n::t(Key::status_deleted)),
            FileStatusKind::Renamed => ("edit-find-replace-symbolic", "accent", i18n::t(Key::status_renamed)),
            FileStatusKind::Typechange => ("dialog-warning-symbolic", "warning", i18n::t(Key::status_typechange)),
        };
        (n, c, t.to_string())
    };
    widgets.status_icon.set_icon_name(Some(name));
    widgets.status_icon.set_css_classes(&[css]);
    widgets.status_icon.set_tooltip_text(Some(&tooltip));

    widgets.path_label.set_label(&path);
    let attrs = gtk::pango::AttrList::new();
    if is_staged {
        attrs.insert(gtk::pango::AttrInt::new_weight(gtk::pango::Weight::Bold));
        widgets.path_label.add_css_class("success");
    } else {
        widgets.path_label.remove_css_class("success");
    }
    widgets.path_label.set_attributes(Some(&attrs));

    // The name doubles as the click handler's op selector, so it is rewritten
    // on every bind — see `invoke_row_button`.
    if is_staged {
        widgets.action_btn.set_icon_name("list-remove-symbolic");
        widgets.action_btn.set_tooltip_text(Some("Unstage"));
        widgets.action_btn.set_widget_name("unstage-file");
    } else {
        widgets.action_btn.set_icon_name("list-add-symbolic");
        widgets.action_btn.set_tooltip_text(Some("Stage"));
        widgets.action_btn.set_widget_name("stage-file");
    }

    widgets.discard_btn.set_visible(!is_staged);
    widgets.revealer.set_reveal_child(obj.expanded());
    widgets.expand_icon.set_icon_name(Some(if obj.expanded() {
        "pan-down-symbolic"
    } else {
        "pan-end-symbolic"
    }));
}

/// Install a single ListView-level pointer-motion controller that reveals the
/// row-action buttons under the cursor. One controller for the whole list
/// instead of one per row.
fn attach_row_hover_controller(list_view: &gtk::ListView) {
    use std::cell::RefCell;
    let visible: Rc<RefCell<Option<gtk::Box>>> = Rc::new(RefCell::new(None));

    let hover = gtk::EventControllerMotion::new();
    let lv = list_view.clone();
    let visible_motion = visible.clone();

    hover.connect_motion(move |_, x, y| {
        // `pick` returns the deepest widget under the pointer; walk up to the
        // row's outer Box (marked with ROW_OUTER_CSS), then locate "row-actions".
        let picked = lv.pick(x, y, gtk::PickFlags::DEFAULT);
        let new_box = picked.and_then(|w| {
            let mut cur: Option<gtk::Widget> = Some(w);
            while let Some(c) = cur {
                if let Ok(b) = c.clone().downcast::<gtk::Box>() {
                    if b.has_css_class(ROW_OUTER_CSS) {
                        return find_child_by_name(&b, "row-actions")
                            .and_then(|w| w.downcast::<gtk::Box>().ok());
                    }
                }
                cur = c.parent();
            }
            None
        });

        let mut current = visible_motion.borrow_mut();
        let same = match (current.as_ref(), new_box.as_ref()) {
            (Some(a), Some(b)) => a.eq(b),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        if let Some(prev) = current.take() {
            prev.set_visible(false);
        }
        if let Some(ref nb) = new_box {
            nb.set_visible(true);
        }
        *current = new_box;
    });

    let visible_leave = visible.clone();
    hover.connect_leave(move |_| {
        if let Some(prev) = visible_leave.borrow_mut().take() {
            prev.set_visible(false);
        }
    });

    list_view.add_controller(hover);
}

/// Toggle the diff section of a file row and return whether it's now expanded.
/// Takes the row's outer Box directly (works for both ListView and any caller
/// that holds a row Box reference).
pub fn toggle_file_diff(outer_box: &gtk::Box) -> bool {
    let diff_box = find_child_by_name(outer_box, "diff-box");
    let expand_icon = find_child_by_name(outer_box, "expand-icon");

    if let Some(diff) = diff_box {
        if let Ok(revealer) = diff.downcast::<gtk::Revealer>() {
            let new_visible = !revealer.reveals_child();
            revealer.set_reveal_child(new_visible);
            if let Some(icon) = expand_icon {
                if let Ok(img) = icon.downcast::<gtk::Image>() {
                    img.set_icon_name(Some(if new_visible {
                        "pan-down-symbolic"
                    } else {
                        "pan-end-symbolic"
                    }));
                }
            }
            return new_visible;
        }
    }
    false
}

/// Get (or lazily build) the diff TextView for a file row's outer Box.
pub fn get_diff_textview(outer_box: &gtk::Box) -> Option<gtk::TextView> {
    if let Some(existing) = find_child_by_name(outer_box, "diff-textview") {
        return existing.downcast::<gtk::TextView>().ok();
    }

    // Lazy build: find the diff Box inside the Revealer and append a TextView.
    let revealer = find_child_by_name(outer_box, "diff-box")?
        .downcast::<gtk::Revealer>()
        .ok()?;
    let diff_box = revealer.child()?.downcast::<gtk::Box>().ok()?;

    let diff_text = gtk::TextView::builder()
        .editable(false)
        .monospace(true)
        .left_margin(4)
        .right_margin(4)
        .top_margin(4)
        .bottom_margin(4)
        .cursor_visible(false)
        .wrap_mode(gtk::WrapMode::None)
        .build();
    diff_text.set_widget_name("diff-textview");
    diff_text.add_css_class("card");
    diff_box.append(&diff_text);
    Some(diff_text)
}

/// Render a single file's diff into the accordion's textview using tags.
pub fn render_file_diff(textview: &gtk::TextView, file: &DiffFile) {
    let buffer = textview.buffer();
    buffer.set_text("");

    // Handle binary files — show a message instead of trying to render binary diff
    if file.is_binary {
        use crate::i18n::{self, Key};
        let mut iter = buffer.end_iter();
        let msg = i18n::t(Key::binary_diff_not_supported);
        buffer.insert(&mut iter, msg);
        textview.set_height_request(24);
        return;
    }

    let is_dark = adw::StyleManager::default().is_dark();

    // Setup diff tags — recreated on each render to handle theme changes.
    // The 6 named tags are always the same; dynamic syn_* tags from the
    // previous render are simply not looked up (stale names differ by
    // color), so there is no accumulation.
    let tag_table = buffer.tag_table();
    for name in &[
        "addition", "deletion", "addition-emph", "deletion-emph",
        "hunk-header", "lineno",
    ] {
        if let Some(tag) = tag_table.lookup(name) {
            tag_table.remove(&tag);
        }
    }

    let (add_bg, add_fg, del_bg, del_fg, add_emph_bg, del_emph_bg, hunk_bg, hunk_fg, lineno_fg) = if is_dark {
        ("#1a3a2a", "#a3d9a5", "#3a1a1a", "#d9a3a3", "#2d6a3f", "#6a2d2d", "#1a2a3a", "#6cb6ff", "#6e7681")
    } else {
        ("#d4edda", "#155724", "#f8d7da", "#721c24", "#a3e0b3", "#f5b5b5", "#ddf4ff", "#0550ae", "#8b949e")
    };

    tag_table.add(&gtk::TextTag::builder().name("addition").background(add_bg).foreground(add_fg).build());
    tag_table.add(&gtk::TextTag::builder().name("deletion").background(del_bg).foreground(del_fg).build());
    tag_table.add(&gtk::TextTag::builder().name("addition-emph").background(add_emph_bg).build());
    tag_table.add(&gtk::TextTag::builder().name("deletion-emph").background(del_emph_bg).build());
    tag_table.add(&gtk::TextTag::builder().name("hunk-header").background(hunk_bg).foreground(hunk_fg).build());
    tag_table.add(&gtk::TextTag::builder().name("lineno").foreground(lineno_fg).build());

    // Collect all line contents for syntax highlighting
    let syntax_ref = syntax::detect_syntax(&file.path);
    let all_lines: Vec<&str> = file.hunks.iter()
        .flat_map(|h| h.lines.iter().map(|l| l.content.as_str()))
        .collect();
    let highlights = syntax_ref.map(|sr| syntax::highlight_lines(sr, &all_lines, is_dark));

    let mut iter = buffer.end_iter();
    let mut line_count = 0;
    let mut global_line_idx = 0;

    // Track each rendered diff line's content-area byte offset so we can run a
    // second-pass word-level diff for adjacent deletion/addition pairs.
    struct RenderedLine {
        kind: DiffLineKind,
        content_start: i32,
        content: String,
    }
    let mut hunk_rendered: Vec<RenderedLine> = Vec::new();

    for hunk in &file.hunks {
        // Hunk header
        let start = iter.offset();
        buffer.insert(&mut iter, &hunk.header);
        if !hunk.header.ends_with('\n') { buffer.insert(&mut iter, "\n"); }
        let start_iter = buffer.iter_at_offset(start);
        buffer.apply_tag_by_name("hunk-header", &start_iter, &iter);

        hunk_rendered.clear();

        for line in &hunk.lines {
            let prefix = match line.kind {
                DiffLineKind::Addition => "+",
                DiffLineKind::Deletion => "-",
                DiffLineKind::Context => " ",
            };

            let line_start = iter.offset();
            let text = format!("{}{}", prefix, line.content);
            buffer.insert(&mut iter, &text);
            if !line.content.ends_with('\n') { buffer.insert(&mut iter, "\n"); }

            // Apply diff background tag
            let diff_tag = match line.kind {
                DiffLineKind::Addition => Some("addition"),
                DiffLineKind::Deletion => Some("deletion"),
                DiffLineKind::Context => None,
            };
            if let Some(tag) = diff_tag {
                let s = buffer.iter_at_offset(line_start);
                buffer.apply_tag_by_name(tag, &s, &iter);
            }

            // Apply syntax highlighting on top (foreground only, higher priority)
            if let Some(ref hl) = highlights {
                if let Some(spans) = hl.get(global_line_idx) {
                    // prefix.len() is bytes but GTK offsets are character-based;
                    // syntect spans are also byte offsets — convert both.
                    let prefix_chars = prefix.chars().count() as i32;
                    let content_offset = line_start + prefix_chars;

                    // Build a byte→char offset map for the line content.
                    let byte_to_char: Vec<(usize, i32)> = line.content
                        .char_indices()
                        .map(|(b, _)| (b, line.content[..b].chars().count() as i32))
                        .chain(std::iter::once((line.content.len(), line.content.chars().count() as i32)))
                        .collect();

                    for span in spans {
                        let tag_name = format!("syn_{:02x}{:02x}{:02x}", span.fg.0, span.fg.1, span.fg.2);
                        if tag_table.lookup(&tag_name).is_none() {
                            let color = format!("#{:02x}{:02x}{:02x}", span.fg.0, span.fg.1, span.fg.2);
                            let tag = gtk::TextTag::builder()
                                .name(&tag_name)
                                .foreground(&color)
                                .foreground_set(true)
                                .build();
                            tag_table.add(&tag);
                        }
                        let s_char = byte_to_char.iter()
                            .find(|(b, _)| *b >= span.start)
                            .map(|(_, c)| *c)
                            .unwrap_or(0);
                        let e_char = byte_to_char.iter()
                            .find(|(b, _)| *b >= span.end)
                            .map(|(_, c)| *c)
                            .unwrap_or(line.content.chars().count() as i32);
                        let s = buffer.iter_at_offset(content_offset + s_char);
                        let e = buffer.iter_at_offset(content_offset + e_char);
                        buffer.apply_tag_by_name(&tag_name, &s, &e);
                    }
                }
            }

            hunk_rendered.push(RenderedLine {
                kind: line.kind,
                content_start: line_start + prefix.chars().count() as i32,
                content: line.content.clone(),
            });

            global_line_idx += 1;
            line_count += 1;
        }

        // Word-level diff pass: pair up runs of consecutive deletions then
        // additions (only when the run sizes match), then highlight changed
        // tokens within each paired line.
        let mut i = 0;
        while i < hunk_rendered.len() {
            if !matches!(hunk_rendered[i].kind, DiffLineKind::Deletion) {
                i += 1;
                continue;
            }
            let del_start = i;
            while i < hunk_rendered.len()
                && matches!(hunk_rendered[i].kind, DiffLineKind::Deletion)
            {
                i += 1;
            }
            let del_end = i;
            let add_start = i;
            while i < hunk_rendered.len()
                && matches!(hunk_rendered[i].kind, DiffLineKind::Addition)
            {
                i += 1;
            }
            let add_end = i;

            let del_count = del_end - del_start;
            let add_count = add_end - add_start;
            if del_count == 0 || add_count == 0 || del_count != add_count {
                continue;
            }

            for k in 0..del_count {
                let del = &hunk_rendered[del_start + k];
                let add = &hunk_rendered[add_start + k];
                let Some(wd) = super::word_diff::diff_lines(&del.content, &add.content) else {
                    continue;
                };
                for r in &wd.deleted {
                    let s = buffer.iter_at_offset(del.content_start + r.start as i32);
                    let e = buffer.iter_at_offset(del.content_start + r.end as i32);
                    buffer.apply_tag_by_name("deletion-emph", &s, &e);
                }
                for r in &wd.inserted {
                    let s = buffer.iter_at_offset(add.content_start + r.start as i32);
                    let e = buffer.iter_at_offset(add.content_start + r.end as i32);
                    buffer.apply_tag_by_name("addition-emph", &s, &e);
                }
            }
        }
    }

    // Set height based on content (cap at ~25 lines)
    let visible_lines = line_count.clamp(3, 25);
    textview.set_height_request(visible_lines * 18);
}

/// Get the hunk-actions-box from a file row's outer Box.
pub fn get_hunk_actions_box(outer_box: &gtk::Box) -> Option<gtk::Box> {
    find_child_by_name(outer_box, "hunk-actions-box")
        .and_then(|w| w.downcast::<gtk::Box>().ok())
}

/// Populate hunk action buttons for a file diff.
/// `is_staged` determines whether buttons say "Unstage Hunk" or "Stage Hunk".
/// Populate hunk action buttons for a file diff.
/// `is_staged` determines whether buttons say "Unstage Hunk" or "Stage Hunk".
/// `hunks` are the actual diff hunks for building line selectors.
pub fn populate_hunk_actions(
    hunk_box: &gtk::Box,
    hunks: &[crate::model::DiffHunk],
    is_staged: bool,
) {
    while let Some(child) = hunk_box.first_child() {
        hunk_box.remove(&child);
    }

    let num_hunks = hunks.len();
    if num_hunks == 0 {
        return;
    }

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    row.set_margin_start(4);
    row.set_margin_top(2);
    row.set_margin_bottom(2);
    row.set_widget_name("hunk-buttons-row");

    // Hunk-level buttons — always show so users can stage individual hunks
    // even when a file has only one hunk.
    for i in 0..num_hunks {
            let label = if is_staged {
                crate::i18n::FmtKey::unstage_hunk_n(i + 1).render()
            } else {
                crate::i18n::FmtKey::stage_hunk_n(i + 1).render()
            };
            let btn = gtk::Button::builder()
                .label(&label)
                .css_classes(["flat", "caption"])
                .build();
            let name = if is_staged {
                format!("unstage-hunk-{}", i)
            } else {
                format!("stage-hunk-{}", i)
            };
            btn.set_widget_name(&name);
            row.append(&btn);
        }

    // "Select Lines" toggle — opens line-level selection UI
    let select_lines_btn = gtk::Button::builder()
        .label(i18n::t(Key::select_lines))
        .css_classes(["flat", "caption"])
        .build();
    select_lines_btn.set_widget_name("select-lines-btn");
    row.append(&select_lines_btn);

    hunk_box.append(&row);

    // Prepare line selector containers (hidden initially)
    let selectors_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    selectors_box.set_widget_name("line-selectors-box");
    selectors_box.set_visible(false);

    for (i, hunk) in hunks.iter().enumerate() {
        let selector = build_line_selector(hunk, i);
        selectors_box.append(&selector);
    }

    // "Stage Selected Lines" button (hidden initially)
    let stage_lines_btn = gtk::Button::builder()
        .label(if is_staged { i18n::t(Key::unstage_selected_lines) } else { i18n::t(Key::stage_selected_lines) })
        .css_classes(["suggested-action", "caption"])
        .margin_start(4)
        .margin_top(4)
        .build();
    stage_lines_btn.set_widget_name(if is_staged { "unstage-selected-lines" } else { "stage-selected-lines" });
    stage_lines_btn.set_visible(false);

    hunk_box.append(&selectors_box);
    hunk_box.append(&stage_lines_btn);

    // Toggle line selection mode
    {
        let selectors = selectors_box.clone();
        let stage_btn = stage_lines_btn.clone();
        select_lines_btn.connect_clicked(move |btn| {
            let visible = !selectors.is_visible();
            selectors.set_visible(visible);
            stage_btn.set_visible(visible);
            btn.set_label(if visible { i18n::t(Key::hide_lines) } else { i18n::t(Key::select_lines) });
        });
    }
}

/// Build a line-selection ListBox for a single hunk.
/// Returns the box and a closure to collect selected line indices.
pub fn build_line_selector(hunk: &crate::model::DiffHunk, hunk_index: usize) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    container.set_widget_name(&format!("line-selector-{}", hunk_index));
    container.add_css_class("card");
    container.set_margin_start(4);
    container.set_margin_end(4);
    container.set_margin_top(2);
    container.set_margin_bottom(2);

    // Hunk header
    let header_label = gtk::Label::builder()
        .label(&hunk.header)
        .css_classes(["caption", "monospace", "dim-label"])
        .xalign(0.0)
        .margin_start(4)
        .margin_top(2)
        .build();
    container.append(&header_label);

    for (i, line) in hunk.lines.iter().enumerate() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        row.set_margin_start(4);

        let is_changeable = matches!(line.kind, DiffLineKind::Addition | DiffLineKind::Deletion);

        if is_changeable {
            let check = gtk::CheckButton::new();
            check.set_active(false);
            check.set_widget_name(&format!("line-check-{}-{}", hunk_index, i));
            row.append(&check);
        } else {
            // Spacer to align with checkboxes
            let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            spacer.set_width_request(20);
            row.append(&spacer);
        }

        let prefix = match line.kind {
            DiffLineKind::Addition => "+",
            DiffLineKind::Deletion => "-",
            DiffLineKind::Context => " ",
        };
        let css = match line.kind {
            DiffLineKind::Addition => "success",
            DiffLineKind::Deletion => "error",
            DiffLineKind::Context => "dim-label",
        };

        let content = gtk::Label::builder()
            .label(format!("{}{}", prefix, line.content.trim_end()))
            .css_classes(["caption", "monospace", css])
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .hexpand(true)
            .build();
        row.append(&content);

        container.append(&row);
    }

    container
}

/// Collect checked line indices from a line-selector box.
pub fn collect_selected_lines(selector: &gtk::Box) -> Vec<usize> {
    let mut indices = Vec::new();
    let mut child = selector.first_child();
    while let Some(c) = child {
        if let Ok(row) = c.clone().downcast::<gtk::Box>() {
            if let Some(first) = row.first_child() {
                if let Ok(check) = first.downcast::<gtk::CheckButton>() {
                    if check.is_active() {
                        let name = check.widget_name().to_string();
                        // Parse "line-check-{hunk}-{line}"
                        if let Some(idx_str) = name.rsplit('-').next() {
                            if let Ok(idx) = idx_str.parse::<usize>() {
                                indices.push(idx);
                            }
                        }
                    }
                }
            }
        }
        child = c.next_sibling();
    }
    indices
}

fn update_header_label<W: IsA<gtk::Widget>>(widget: &W, header_name: &str, full_label: &str) {
    let Some(parent) = widget.parent().and_then(|p| p.downcast::<gtk::Box>().ok()) else { return };
    let mut child = parent.first_child();
    while let Some(c) = child {
        if c.widget_name() == header_name {
            if let Ok(label) = c.downcast::<gtk::Label>() {
                label.set_label(full_label);
            }
            return;
        }
        child = c.next_sibling();
    }
}

fn find_child_by_name(widget: &gtk::Box, name: &str) -> Option<gtk::Widget> {
    let mut child = widget.first_child();
    while let Some(c) = child {
        if c.widget_name() == name {
            return Some(c);
        }
        if let Ok(inner_box) = c.clone().downcast::<gtk::Box>() {
            if let Some(found) = find_child_by_name(&inner_box, name) {
                return Some(found);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    use serial_test::serial;

    use crate::test_support;

    /// Depth-first search for a descendant button by widget name. Unlike
    /// `find_child_by_name` this does not care where in the row the button sits.
    fn find_button(root: &gtk::Widget, name: &str) -> Option<gtk::Button> {
        if root.widget_name() == name {
            return root.clone().downcast::<gtk::Button>().ok();
        }
        let mut child = root.first_child();
        while let Some(c) = child {
            if let Some(found) = find_button(&c, name) {
                return Some(found);
            }
            child = c.next_sibling();
        }
        None
    }

    /// Rows are recycled, so the stage/unstage button must be re-targeted on
    /// every bind. `bind_row_widgets` renames it to `stage-file`/`unstage-file`,
    /// which used to make the next bind's `find_child_by_name("action-btn")`
    /// lookup miss — leaving a recycled row with the previous file's action.
    #[test]
    #[serial]
    fn recycled_row_updates_its_stage_button() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }

        let (stage_after_first, unstage_after_rebind, stale_stage) =
            test_support::on_gtk_thread(|| {
                let row = build_row_template();

                let unstaged =
                    ChangedFileObject::new("a.rs".into(), FileStatusKind::Modified, false);
                bind_row_widgets(&row, &unstaged);
                let stage_after_first = find_button(row.upcast_ref(), "stage-file").is_some();

                let staged = ChangedFileObject::new("b.rs".into(), FileStatusKind::Modified, true);
                bind_row_widgets(&row, &staged);

                (
                    stage_after_first,
                    find_button(row.upcast_ref(), "unstage-file").is_some(),
                    find_button(row.upcast_ref(), "stage-file").is_some(),
                )
            });

        assert!(stage_after_first, "first bind should present a Stage button");
        assert!(
            unstage_after_rebind,
            "re-bound row should present an Unstage button"
        );
        assert!(
            !stale_stage,
            "stale Stage button must not survive the re-bind"
        );
    }

    /// The changes-view renderer must show the i18n message for a binary
    /// file instead of an empty (or garbled) diff pane.
    #[test]
    #[serial]
    fn render_file_diff_shows_binary_message() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }

        let text = test_support::on_gtk_thread(|| {
            let tv = gtk::TextView::new();
            let file = DiffFile {
                path: "data.bin".into(),
                hunks: vec![],
                stats: crate::model::DiffStats {
                    insertions: 0,
                    deletions: 0,
                },
                is_binary: true,
            };
            render_file_diff(&tv, &file);
            tv.buffer()
                .text(&tv.buffer().start_iter(), &tv.buffer().end_iter(), false)
                .to_string()
        });

        assert!(
            text.contains(i18n::t(Key::binary_diff_not_supported)),
            "changes view must show the binary message, got: {text:?}"
        );
    }

    /// Background refreshes splice the whole store; a row the user had open
    /// must stay expanded across the rebuild (GOAL 5.1).
    #[test]
    #[serial]
    fn populate_preserves_expanded_rows_across_refresh() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }

        let (expanded_after, collapsed_other) = test_support::on_gtk_thread(|| {
            let store = gio::ListStore::new::<ChangedFileObject>();
            let list_view = gtk::ListView::default();

            let files = vec![
                ChangedFileEntry {
                    path: "src/a.rs".into(),
                    status: FileStatusKind::Modified,
                    is_staged: false,
                },
                ChangedFileEntry {
                    path: "src/b.rs".into(),
                    status: FileStatusKind::Modified,
                    is_staged: false,
                },
            ];

            populate_file_lists(&store, &files, &list_view);
            // User expands the first row.
            let first = store.item(0).unwrap().downcast::<ChangedFileObject>().unwrap();
            first.set_expanded(true);
            drop(first);

            // Background refresh rebuilds the store with the same paths.
            populate_file_lists(&store, &files, &list_view);

            let first = store.item(0).unwrap().downcast::<ChangedFileObject>().unwrap();
            let second = store.item(1).unwrap().downcast::<ChangedFileObject>().unwrap();
            (first.expanded(), second.expanded())
        });

        assert!(expanded_after, "expanded row must stay expanded after a refresh");
        assert!(!collapsed_other, "other rows must not inherit expansion");
    }
}
