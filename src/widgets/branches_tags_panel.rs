use adw::prelude::*;
use gtk::glib;

use crate::model::{BranchInfo, StashEntry, SubmoduleInfo, TagInfo, WorktreeInfo};

/// Build a collapsible section: clickable header that toggles list visibility.
fn build_collapsible_section(parent: &gtk::Box, title: &str, expanded: bool) -> gtk::ListBox {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let header_btn = gtk::Button::builder()
        .css_classes(["flat"])
        .build();
    let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    header_box.set_margin_start(8);
    header_box.set_margin_top(4);
    header_box.set_margin_bottom(2);

    let icon_name = if expanded { "pan-down-symbolic" } else { "pan-end-symbolic" };
    let arrow = gtk::Image::builder()
        .icon_name(icon_name)
        .css_classes(["dim-label"])
        .build();
    header_box.append(&arrow);

    let label = gtk::Label::builder()
        .label(title)
        .css_classes(["heading"])
        .xalign(0.0)
        .hexpand(true)
        .build();
    label.set_widget_name(&format!("section-label-{}", title.to_lowercase()));
    header_box.append(&label);

    header_btn.set_child(Some(&header_box));
    section.append(&header_btn);

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["navigation-sidebar"])
        .build();

    let revealer = gtk::Revealer::builder()
        .reveal_child(expanded)
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .transition_duration(150)
        .child(&list)
        .build();
    section.append(&revealer);

    let revealer_ref = revealer.clone();
    let arrow_ref = arrow.clone();
    header_btn.connect_clicked(move |_| {
        let visible = !revealer_ref.reveals_child();
        revealer_ref.set_reveal_child(visible);
        arrow_ref.set_icon_name(Some(if visible {
            "pan-down-symbolic"
        } else {
            "pan-end-symbolic"
        }));
    });

    parent.append(&section);
    list
}

pub struct BranchesTagsRefs {
    pub local_list: gtk::ListBox,
    pub remote_list: gtk::ListBox,
    pub tags_list: gtk::ListBox,
    pub stashes_list: gtk::ListBox,
    pub submodules_list: gtk::ListBox,
    pub worktrees_list: gtk::ListBox,
    pub create_branch_btn: gtk::Button,
}

pub fn build_branches_tags_panel() -> (gtk::Box, BranchesTagsRefs) {
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    // No explicit width_request — let AdwOverlaySplitView's min_sidebar_width drive sizing.

    // Search
    let search_entry = gtk::SearchEntry::builder()
        .placeholder_text("Filter…")
        .margin_start(8)
        .margin_end(8)
        .margin_top(8)
        .margin_bottom(4)
        .build();
    panel.append(&search_entry);

    let scrolled = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();

    let inner = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let local_list = build_collapsible_section(&inner, "Local", true);
    let remote_list = build_collapsible_section(&inner, "Remote", false);
    let tags_list = build_collapsible_section(&inner, "Tags", false);
    let stashes_list = build_collapsible_section(&inner, "Stashes", false);
    let submodules_list = build_collapsible_section(&inner, "Submodules", false);
    let worktrees_list = build_collapsible_section(&inner, "Worktrees", false);

    scrolled.set_child(Some(&inner));
    panel.append(&scrolled);

    // Create branch button — icon + label, ellipsize if narrow
    let cb_inner = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    cb_inner.set_halign(gtk::Align::Center);
    let cb_icon = gtk::Image::from_icon_name("list-add-symbolic");
    cb_inner.append(&cb_icon);
    let cb_label = gtk::Label::builder()
        .label("New branch")
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    cb_inner.append(&cb_label);

    let create_branch_btn = gtk::Button::builder()
        .child(&cb_inner)
        .css_classes(["suggested-action", "pill"])
        .tooltip_text("Create branch from HEAD")
        .margin_start(8)
        .margin_end(8)
        .margin_top(4)
        .margin_bottom(8)
        .build();
    panel.append(&create_branch_btn);

    // Search filter
    let ll = local_list.clone();
    let rl = remote_list.clone();
    let tl = tags_list.clone();
    let sl = stashes_list.clone();
    let sml = submodules_list.clone();
    let wtl = worktrees_list.clone();
    search_entry.connect_search_changed(move |entry| {
        let query = entry.text().to_lowercase();
        let q = query.clone();
        ll.set_filter_func(move |row| matches_search(row, &q));
        let q = query.clone();
        rl.set_filter_func(move |row| matches_search(row, &q));
        let q = query.clone();
        tl.set_filter_func(move |row| matches_search(row, &q));
        let q = query.clone();
        sl.set_filter_func(move |row| matches_search(row, &q));
        let q = query.clone();
        sml.set_filter_func(move |row| matches_search(row, &q));
        let q = query;
        wtl.set_filter_func(move |row| matches_search(row, &q));
    });

    let refs = BranchesTagsRefs {
        local_list,
        remote_list,
        tags_list,
        stashes_list,
        submodules_list,
        worktrees_list,
        create_branch_btn,
    };

    (panel, refs)
}

/// Widget name of the synthetic row `apply_row_limit` appends. It is not an
/// item, and callers must not read a branch, tag or stash name out of it.
pub const SHOW_MORE_ROW: &str = "show-more-row";

/// Where the toggle's signal handler id is parked on the ListBox, so the next
/// application of the limit can drop it.
const TOGGLE_HANDLER_KEY: &str = "gp-show-more-handler";

/// Connect `on_item` to row activations, skipping the synthetic "Show all" row.
///
/// Every consumer of these lists used to wire `connect_row_activated` itself and
/// read `widget_name()` as the item name, which turned a click on "Show all"
/// into a checkout of a branch called `show-more-row` (issue #7). Going through
/// here keeps that filtering in one place, next to the code that creates the row.
pub fn connect_item_activated<F: Fn(&str) + 'static>(list: &gtk::ListBox, on_item: F) {
    list.connect_row_activated(move |_, row| {
        if let Some(name) = item_name(row) {
            on_item(&name);
        }
    });
}

/// The branch, tag or stash a row stands for, or `None` when the row is not an
/// item at all — the synthetic "Show all" toggle, or an unnamed row.
///
/// Every path that turns a row into an operation must go through here. Filtering
/// left-click activation alone was not enough: the right-click context menus
/// read `widget_name()` straight off the row under the cursor and offered to
/// check out, merge or delete a branch called `show-more-row` (issues #7, #8).
/// Whether `row` survives the sidebar search for `query`, which must already be
/// lowercased.
///
/// The "Show all" toggle belongs to the list rather than to the results:
/// matching it on "show", "more" or "row" surfaced it as a branch called
/// `show-more-row`. With no query it stays, because it is still the toggle.
pub fn matches_search(row: &gtk::ListBoxRow, query: &str) -> bool {
    match item_name(row) {
        None => query.is_empty(),
        Some(name) => query.is_empty() || name.to_lowercase().contains(query),
    }
}

pub fn item_name(row: &gtk::ListBoxRow) -> Option<String> {
    let name = row.widget_name().to_string();
    // GTK falls back to the widget's type name when none was set, so an
    // unnamed row reports "GtkListBoxRow" and never an empty string — which is
    // why the `is_empty()` guard the context menus used could not fire.
    if name.is_empty() || name == SHOW_MORE_ROW || name == "GtkListBoxRow" {
        return None;
    }
    Some(name)
}

/// Apply a row limit to a ListBox: hide rows beyond `limit` and add a "Show all" toggle.
/// If limit is 0, show everything.
pub fn apply_row_limit(list: &gtk::ListBox, limit: u32) {
    if limit == 0 {
        return;
    }
    let limit = limit as i32;
    let mut count = 0;
    let mut child = list.first_child();
    let mut overflow_rows: Vec<gtk::Widget> = Vec::new();

    while let Some(c) = child {
        let next = c.next_sibling();
        if c.widget_name() == SHOW_MORE_ROW {
            list.remove(&c);
        } else {
            count += 1;
            if count > limit {
                c.set_visible(false);
                overflow_rows.push(c);
            }
        }
        child = next;
    }

    if overflow_rows.is_empty() {
        return;
    }

    let toggle_row = gtk::ListBoxRow::builder()
        .selectable(false)
        .activatable(true)
        .build();
    toggle_row.set_widget_name(SHOW_MORE_ROW);
    let collapsed_text = format!("Show all ({} more)", overflow_rows.len());
    let label = gtk::Label::builder()
        .label(&collapsed_text)
        .css_classes(["caption", "dim-label"])
        .margin_top(2)
        .margin_bottom(2)
        .build();
    toggle_row.set_child(Some(&label));
    list.append(&toggle_row);

    // This function runs on every background refresh. Without dropping the
    // previous handler they accumulate, and each one toggles the rows in turn —
    // so after an even number of refreshes "Show all" flipped the rows twice
    // and appeared to do nothing at all.
    unsafe {
        if let Some(previous) = list.steal_data::<glib::SignalHandlerId>(TOGGLE_HANDLER_KEY) {
            list.disconnect(previous);
        }
    }

    let overflow_rows = std::rc::Rc::new(overflow_rows);
    let rows = overflow_rows.clone();
    let collapsed = collapsed_text.clone();
    let handler = list.connect_row_activated(move |_, row| {
        if row.widget_name() != SHOW_MORE_ROW {
            return;
        }
        let label = row.child()
            .and_then(|c| c.downcast::<gtk::Label>().ok());
        let Some(label) = label else { return };

        let currently_hidden = rows.first().map(|r| !r.is_visible()).unwrap_or(false);
        for r in rows.iter() {
            r.set_visible(currently_hidden);
        }
        label.set_label(if currently_hidden { "Show less" } else { &collapsed });
    });
    unsafe {
        list.set_data(TOGGLE_HANDLER_KEY, handler);
    }
}

/// Update the section header label to show count, e.g. "Tags (13)".
/// The list must be inside a section created by `build_collapsible_section`.
pub fn update_section_header(list: &gtk::ListBox, title: &str, count: usize) {
    let expected_name = format!("section-label-{}", title.to_lowercase());
    let section = list.parent();
    let Some(section) = section else { return };

    // Hide entire section if empty
    section.set_visible(count > 0);

    // Update label text
    let mut child = section.first_child();
    while let Some(c) = child {
        if let Ok(btn) = c.clone().downcast::<gtk::Button>() {
            if let Some(header_box) = btn.child() {
                let mut inner_child = header_box.first_child();
                while let Some(ic) = inner_child {
                    if ic.widget_name() == expected_name {
                        if let Ok(label) = ic.clone().downcast::<gtk::Label>() {
                            label.set_label(&format!("{} ({})", title, count));
                            return;
                        }
                    }
                    inner_child = ic.next_sibling();
                }
            }
        }
        child = c.next_sibling();
    }
}

/// Populate the branches panel with branch data.
pub fn populate_branches(local_list: &gtk::ListBox, remote_list: &gtk::ListBox, branches: &[BranchInfo]) {
    // Clear
    while let Some(child) = local_list.first_child() {
        local_list.remove(&child);
    }
    while let Some(child) = remote_list.first_child() {
        remote_list.remove(&child);
    }

    for branch in branches {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row_box.set_margin_start(8);
        row_box.set_margin_end(8);
        row_box.set_margin_top(4);
        row_box.set_margin_bottom(4);

        let branch_icon = gtk::Image::builder()
            .icon_name("branch-fork-symbolic")
            .css_classes(if branch.is_head { vec!["success"] } else { vec!["dim-label"] })
            .pixel_size(14)
            .build();
        row_box.append(&branch_icon);

        if branch.is_head {
            row_box.append(&gtk::Image::builder()
                .icon_name("object-select-symbolic")
                .css_classes(["success"])
                .pixel_size(12)
                .build());
        }

        let name_label = gtk::Label::builder()
            .label(&branch.name)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        row_box.append(&name_label);

        if !branch.is_remote && (branch.ahead > 0 || branch.behind > 0) {
            let indicator = gtk::Label::builder()
                .label(format!("▲{} ▼{}", branch.ahead, branch.behind))
                .css_classes(["caption", "dim-label"])
                .build();
            row_box.append(&indicator);
        }

        let row = gtk::ListBoxRow::builder()
            .child(&row_box)
            .activatable(true)
            .build();
        row.set_widget_name(&branch.name);

        if branch.is_remote {
            remote_list.append(&row);
        } else {
            local_list.append(&row);
        }
    }
}

/// Populate the tags list.
pub fn populate_tags(tags_list: &gtk::ListBox, tags: &[TagInfo]) {
    while let Some(child) = tags_list.first_child() {
        tags_list.remove(&child);
    }

    for tag in tags {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row_box.set_margin_start(8);
        row_box.set_margin_end(8);
        row_box.set_margin_top(4);
        row_box.set_margin_bottom(4);

        row_box.append(&gtk::Image::builder()
            .icon_name("tag-outline-symbolic")
            .css_classes(["dim-label"])
            .pixel_size(14)
            .build());

        let name_label = gtk::Label::builder()
            .label(&tag.name)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        row_box.append(&name_label);

        let row = gtk::ListBoxRow::builder()
            .child(&row_box)
            .activatable(true)
            .build();
        row.set_widget_name(&tag.name);

        tags_list.append(&row);
    }
}

/// Populate the stashes list. Each row has Apply and Drop buttons.
/// `on_apply` and `on_drop` are called with the stash index.
pub fn populate_stashes<FA, FD>(
    stashes_list: &gtk::ListBox,
    entries: &[StashEntry],
    on_apply: FA,
    on_drop: FD,
)
where
    FA: Fn(usize) + Clone + 'static,
    FD: Fn(usize) + Clone + 'static,
{
    while let Some(child) = stashes_list.first_child() {
        stashes_list.remove(&child);
    }

    if entries.is_empty() {
        return;
    }

    for entry in entries {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row_box.set_margin_start(8);
        row_box.set_margin_end(4);
        row_box.set_margin_top(4);
        row_box.set_margin_bottom(4);

        let label = gtk::Label::builder()
            .label(format!("{}: {}", entry.index, entry.message))
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .tooltip_text(&entry.message)
            .build();
        row_box.append(&label);

        let apply_btn = gtk::Button::builder()
            .icon_name("go-up-symbolic")
            .css_classes(["flat", "circular"])
            .tooltip_text("Apply")
            .valign(gtk::Align::Center)
            .build();
        let idx = entry.index;
        let on_apply_clone = on_apply.clone();
        apply_btn.connect_clicked(move |_| {
            on_apply_clone(idx);
        });
        row_box.append(&apply_btn);

        let drop_btn = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .css_classes(["flat", "circular"])
            .tooltip_text("Drop")
            .valign(gtk::Align::Center)
            .build();
        let idx = entry.index;
        let on_drop_clone = on_drop.clone();
        drop_btn.connect_clicked(move |_| {
            on_drop_clone(idx);
        });
        row_box.append(&drop_btn);

        let row = gtk::ListBoxRow::builder()
            .child(&row_box)
            .activatable(false)
            .build();
        stashes_list.append(&row);
    }
}

/// Populate the submodules list. Each row has an Update button.
pub fn populate_submodules<F>(
    list: &gtk::ListBox,
    submodules: &[SubmoduleInfo],
    on_update: F,
)
where
    F: Fn(String) + Clone + 'static,
{
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    if submodules.is_empty() {
        return;
    }

    for sm in submodules {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row_box.set_margin_start(8);
        row_box.set_margin_end(4);
        row_box.set_margin_top(4);
        row_box.set_margin_bottom(4);

        let icon = if sm.is_initialized { "folder-remote-symbolic" } else { "folder-symbolic" };
        row_box.append(&gtk::Image::builder()
            .icon_name(icon)
            .css_classes(["dim-label"])
            .build());

        let label = gtk::Label::builder()
            .label(&sm.name)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .tooltip_text(&sm.url)
            .build();
        row_box.append(&label);

        if !sm.is_initialized {
            let init_label = gtk::Label::builder()
                .label("not init")
                .css_classes(["caption", "dim-label"])
                .build();
            row_box.append(&init_label);
        }

        let update_btn = gtk::Button::builder()
            .icon_name("view-refresh-symbolic")
            .css_classes(["flat", "circular"])
            .tooltip_text("Update")
            .valign(gtk::Align::Center)
            .build();
        let on_update_clone = on_update.clone();
        let name = sm.name.clone();
        update_btn.connect_clicked(move |_| {
            on_update_clone(name.clone());
        });
        row_box.append(&update_btn);

        let row = gtk::ListBoxRow::builder()
            .child(&row_box)
            .activatable(false)
            .build();
        row.set_widget_name(&sm.name);
        list.append(&row);
    }
}

/// Populate the worktrees list.
pub fn populate_worktrees<F>(
    list: &gtk::ListBox,
    worktrees: &[WorktreeInfo],
    on_open: F,
)
where
    F: Fn(String) + Clone + 'static,
{
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    if worktrees.len() <= 1 {
        // Only the main worktree — nothing interesting to show
        return;
    }

    for wt in worktrees {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row_box.set_margin_start(8);
        row_box.set_margin_end(4);
        row_box.set_margin_top(4);
        row_box.set_margin_bottom(4);

        let icon_name = if wt.is_current { "object-select-symbolic" } else { "folder-symbolic" };
        row_box.append(&gtk::Image::builder()
            .icon_name(icon_name)
            .css_classes(["dim-label"])
            .build());

        let label = gtk::Label::builder()
            .label(&wt.name)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .tooltip_text(&wt.path)
            .build();
        row_box.append(&label);

        if !wt.is_current {
            let open_btn = gtk::Button::builder()
                .icon_name("document-open-symbolic")
                .css_classes(["flat", "circular"])
                .tooltip_text("Open")
                .valign(gtk::Align::Center)
                .build();
            let on_open_clone = on_open.clone();
            let path = wt.path.clone();
            open_btn.connect_clicked(move |_| {
                on_open_clone(path.clone());
            });
            row_box.append(&open_btn);
        }

        let row = gtk::ListBoxRow::builder()
            .child(&row_box)
            .activatable(false)
            .build();
        row.set_widget_name(&wt.name);
        list.append(&row);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serial_test::serial;
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::test_support;

    /// A ListBox of `names`, each row named after its item, as the panel builds them.
    fn list_of(names: &[&str]) -> gtk::ListBox {
        let list = gtk::ListBox::new();
        for name in names {
            let row = gtk::ListBoxRow::builder().activatable(true).build();
            row.set_widget_name(name);
            row.set_child(Some(&gtk::Label::new(Some(name))));
            list.append(&row);
        }
        list
    }

    fn row_named(list: &gtk::ListBox, name: &str) -> gtk::ListBoxRow {
        let mut child = list.first_child();
        while let Some(c) = child {
            if c.widget_name() == name {
                return c.downcast::<gtk::ListBoxRow>().expect("rows are ListBoxRow");
            }
            child = c.next_sibling();
        }
        panic!("no row named {name}");
    }

    /// Activations reported to a consumer wired through `connect_item_activated`.
    fn activations(names: &[&'static str], limit: u32, activate: &'static str) -> Vec<String> {
        test_support::on_gtk_thread({
            let names: Vec<String> = names.iter().map(|s| s.to_string()).collect();
            move || {
                let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
                let list = list_of(&refs);
                apply_row_limit(&list, limit);

                let seen = Rc::new(RefCell::new(Vec::new()));
                let recorder = seen.clone();
                connect_item_activated(&list, move |name| {
                    recorder.borrow_mut().push(name.to_string());
                });

                let row = row_named(&list, activate);
                list.emit_by_name::<()>("row-activated", &[&row]);
                let reported = seen.borrow().clone();
                reported
            }
        })
    }

    #[test]
    #[serial]
    fn the_toggle_row_is_never_a_search_hit() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }
        // Searching the sidebar for "show", "more" or "row" used to return the
        // synthetic toggle as though it were a branch called `show-more-row`.
        let (on_show, on_row, on_empty, on_branch) = test_support::on_gtk_thread(|| {
            let list = list_of(&["main", "dev", "wip"]);
            apply_row_limit(&list, 1);
            let toggle = row_named(&list, SHOW_MORE_ROW);
            (
                matches_search(&toggle, "show"),
                matches_search(&toggle, "row"),
                matches_search(&toggle, ""),
                matches_search(&row_named(&list, "wip"), "wi"),
            )
        });
        assert!(!on_show, "\"show\" matched the toggle row");
        assert!(!on_row, "\"row\" matched the toggle row");
        assert!(on_empty, "the toggle must stay visible with no query");
        assert!(on_branch, "a real branch must still match");
    }

    #[test]
    #[serial]
    fn item_name_rejects_everything_that_is_not_an_item() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }
        // The right-click menus look rows up by position, so they need the same
        // filter the activation path uses (issue #8).
        let (show_more, unnamed, branch) = test_support::on_gtk_thread(|| {
            let list = list_of(&["main", "dev", "wip"]);
            apply_row_limit(&list, 1);

            let unnamed = gtk::ListBoxRow::new();
            list.append(&unnamed);

            (
                item_name(&row_named(&list, SHOW_MORE_ROW)),
                item_name(&unnamed),
                item_name(&row_named(&list, "main")),
            )
        });
        assert_eq!(show_more, None, "the Show all row is not an item");
        assert_eq!(unnamed, None, "an unnamed row is not an item");
        assert_eq!(branch, Some("main".to_string()));
    }

    #[test]
    #[serial]
    fn activating_a_branch_reports_its_name() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }
        assert_eq!(activations(&["main", "dev", "wip"], 1, "dev"), vec!["dev"]);
    }

    #[test]
    #[serial]
    fn activating_show_all_reports_nothing() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }
        // Issue #7: this used to check out a branch named "show-more-row".
        let seen = activations(&["main", "dev", "wip"], 1, SHOW_MORE_ROW);
        assert!(seen.is_empty(), "the Show all row was taken for an item: {seen:?}");
    }
    /// Visibility of the overflow rows after activating "Show all" `times`
    /// applications of the limit.
    fn overflow_visible_after_applies(applies: usize) -> Vec<bool> {
        test_support::on_gtk_thread(move || {
            let list = list_of(&["main", "dev", "wip"]);
            for _ in 0..applies {
                apply_row_limit(&list, 1);
            }
            let row = row_named(&list, SHOW_MORE_ROW);
            list.emit_by_name::<()>("row-activated", &[&row]);

            ["dev", "wip"]
                .iter()
                .map(|n| row_named(&list, n).is_visible())
                .collect()
        })
    }

    #[test]
    #[serial]
    fn show_all_still_expands_after_a_refresh() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }
        // The background refresh re-applies the limit on every tick. A handler
        // connected per call would toggle the rows once per tick, so an even
        // number of ticks would leave them hidden.
        assert_eq!(overflow_visible_after_applies(1), vec![true, true]);
        assert_eq!(overflow_visible_after_applies(2), vec![true, true]);
    }
}
