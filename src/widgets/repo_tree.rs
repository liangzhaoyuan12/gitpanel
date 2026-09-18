use adw::prelude::*;
use std::collections::HashSet;

use crate::utils::workspace::WorkspaceEntry;

/// Track expanded folder paths in the sidebar tree.
#[derive(Debug, Default, Clone)]
pub struct TreeState {
    pub expanded: HashSet<String>,
}

impl TreeState {
    pub fn is_expanded(&self, path: &str) -> bool {
        self.expanded.contains(path)
    }

    pub fn toggle(&mut self, path: &str) {
        if !self.expanded.remove(path) {
            self.expanded.insert(path.to_string());
        }
    }
}

/// Create a ListBoxRow for a workspace entry at a given depth.
fn create_repo_row(entry: &WorkspaceEntry, depth: usize, expanded: bool) -> gtk::ListBoxRow {
    let row_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
    row_box.set_margin_start(8 + (depth as i32) * 16);
    row_box.set_margin_end(8);
    row_box.set_margin_top(4);
    row_box.set_margin_bottom(4);

    // Top line: expand arrow (folders) or icon + name + indicators
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 6);

    if !entry.is_git_repo && !entry.children.is_empty() {
        // Folder with children: show expand arrow
        let arrow_icon = if expanded {
            "pan-down-symbolic"
        } else {
            "pan-end-symbolic"
        };
        let arrow = gtk::Image::builder()
            .icon_name(arrow_icon)
            .pixel_size(12)
            .build();
        arrow.add_css_class("dim-label");
        top.append(&arrow);
    }

    let icon_name = if entry.is_git_repo {
        "git-symbolic"
    } else {
        "folder-symbolic"
    };
    let icon = gtk::Image::builder()
        .icon_name(icon_name)
        .pixel_size(16)
        .build();
    if !entry.is_git_repo {
        icon.add_css_class("dim-label");
    }
    top.append(&icon);

    let name_label = gtk::Label::builder()
        .label(&entry.name)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    if !entry.is_git_repo {
        name_label.add_css_class("dim-label");
    }
    top.append(&name_label);

    // Indicators for git repos
    if let Some(ref indicator) = entry.indicator {
        if indicator.is_dirty {
            let (class, tooltip) = if indicator.has_tracked_changes {
                ("gp-ind-yellow", "Uncommitted changes")
            } else {
                ("gp-ind-blue", "Untracked files")
            };
            let dirty = gtk::Label::builder()
                .label("●")
                .css_classes(["caption", class])
                .build();
            dirty.set_tooltip_text(Some(tooltip));
            dirty.set_widget_name("repo-indicator-dirty");
            top.append(&dirty);
        }
        if indicator.ahead > 0 {
            let ahead = gtk::Label::builder()
                .label(format!("●{}", indicator.ahead))
                .css_classes(["caption", "gp-ind-green"])
                .build();
            ahead.set_tooltip_text(Some(&format!("{} unpushed commit(s)", indicator.ahead)));
            ahead.set_widget_name("repo-indicator-ahead");
            top.append(&ahead);
        }
    }

    row_box.append(&top);

    // Bottom line: branch name (small, dim)
    if let Some(ref indicator) = entry.indicator {
        if let Some(ref branch) = indicator.branch {
            let branch_label = gtk::Label::builder()
                .label(branch)
                .xalign(0.0)
                .css_classes(["caption", "dim-label"])
                .margin_start(22)
                .build();
            branch_label.set_widget_name("repo-branch-label");
            row_box.append(&branch_label);
        }
    }

    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&row_box));
    // Store the path in the row's widget_name for lookup
    row.set_widget_name(&entry.path.to_string_lossy());
    row.set_activatable(true); // folders are also clickable
    row.set_selectable(entry.is_git_repo);
    row
}

/// Populate a ListBox with workspace entries, respecting tree expansion state.
/// Returns a map: path -> row index, for later selection.
pub fn populate_repo_list(
    list_box: &gtk::ListBox,
    entries: &[WorkspaceEntry],
    tree_state: &TreeState,
) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    add_entries_recursive(list_box, entries, 0, tree_state);
}

fn add_entries_recursive(
    list_box: &gtk::ListBox,
    entries: &[WorkspaceEntry],
    depth: usize,
    tree_state: &TreeState,
) {
    for entry in entries {
        let expanded = tree_state.is_expanded(&entry.path.to_string_lossy());
        list_box.append(&create_repo_row(entry, depth, expanded));

        // If folder is expanded, show children recursively
        if !entry.is_git_repo && expanded && !entry.children.is_empty() {
            add_entries_recursive(list_box, &entry.children, depth + 1, tree_state);
        }
    }
}

/// Find the row index for a given path in the populated ListBox.
/// This must be called right after populate_repo_list.
pub fn find_row_index_for_path(list_box: &gtk::ListBox, path: &str) -> Option<i32> {
    let mut i = 0;
    let mut child = list_box.first_child();
    while let Some(ref widget) = child {
        if widget.widget_name().as_str() == path {
            return Some(i);
        }
        child = widget.next_sibling();
        i += 1;
    }
    None
}

/// Update only the indicator widgets (dirty dot, ahead count, branch label)
/// of a single row in-place, without rebuilding the entire list.
/// This preserves scroll position and tree expansion state.
pub fn update_indicator_in_place(
    list_box: &gtk::ListBox,
    path_str: &str,
    indicator: &Option<crate::utils::workspace::RepoIndicator>,
) {
    // Find the ListBoxRow by widget_name == path_str
    let mut child = list_box.first_child();
    let row = loop {
        let Some(widget) = child else { return };
        child = widget.next_sibling();
        if widget.widget_name().as_str() == path_str {
            // Should be a ListBoxRow, get its first child (row_box)
            break widget;
        }
    };
    let Ok(list_row) = row.clone().downcast::<gtk::ListBoxRow>() else {
        return;
    };
    let Some(row_box) = list_row.child() else {
        return;
    };
    let Ok(row_box) = row_box.downcast::<gtk::Box>() else {
        return;
    };

    // top = first child of row_box
    let Some(top_widget) = row_box.first_child() else {
        return;
    };
    let Ok(top) = top_widget.downcast::<gtk::Box>() else {
        return;
    };

    // Remove existing indicator labels from top
    let mut to_remove = Vec::new();
    let mut c = top.first_child();
    while let Some(w) = c {
        c = w.next_sibling();
        let name = w.widget_name();
        if name == "repo-indicator-dirty" || name == "repo-indicator-ahead" {
            to_remove.push(w);
        }
    }
    for w in to_remove {
        top.remove(&w);
    }

    // Add new indicator labels
    if let Some(ref ind) = indicator {
        if ind.is_dirty {
            let (class, tooltip) = if ind.has_tracked_changes {
                ("gp-ind-yellow", "Uncommitted changes")
            } else {
                ("gp-ind-blue", "Untracked files")
            };
            let dirty = gtk::Label::builder()
                .label("●")
                .css_classes(["caption", class])
                .build();
            dirty.set_tooltip_text(Some(tooltip));
            dirty.set_widget_name("repo-indicator-dirty");
            top.append(&dirty);
        }
        if ind.ahead > 0 {
            let ahead = gtk::Label::builder()
                .label(format!("●{}", ind.ahead))
                .css_classes(["caption", "gp-ind-green"])
                .build();
            ahead.set_tooltip_text(Some(&format!("{} unpushed commit(s)", ind.ahead)));
            ahead.set_widget_name("repo-indicator-ahead");
            top.append(&ahead);
        }
    }

    // Update branch label (second child of row_box, if it exists)
    let branch_widget = row_box.first_child().and_then(|c| c.next_sibling());
    let new_branch = indicator.as_ref().and_then(|ind| ind.branch.clone());

    match branch_widget {
        Some(w) if w.widget_name() == "repo-branch-label" => {
            if let Some(branch) = &new_branch {
                if let Ok(label) = w.clone().downcast::<gtk::Label>() {
                    label.set_label(branch);
                }
            } else {
                row_box.remove(&w);
            }
        }
        _ => {
            if let Some(branch) = &new_branch {
                let branch_label = gtk::Label::builder()
                    .label(branch)
                    .xalign(0.0)
                    .css_classes(["caption", "dim-label"])
                    .margin_start(22)
                    .build();
                branch_label.set_widget_name("repo-branch-label");
                row_box.append(&branch_label);
            }
        }
    }
}
