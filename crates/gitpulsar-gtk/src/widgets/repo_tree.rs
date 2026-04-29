use adw::prelude::*;

use gitpulsar_core::workspace::WorkspaceEntry;

/// Create a ListBoxRow for a workspace entry (repo or folder).
pub fn create_repo_row(entry: &WorkspaceEntry) -> gtk::ListBoxRow {
    let row_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
    row_box.set_margin_start(8);
    row_box.set_margin_end(8);
    row_box.set_margin_top(4);
    row_box.set_margin_bottom(4);

    // Top line: icon + name + indicators
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 6);

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
            top.append(&dirty);
        }
        if indicator.ahead > 0 {
            let ahead = gtk::Label::builder()
                .label(&format!("●{}", indicator.ahead))
                .css_classes(["caption", "gp-ind-green"])
                .build();
            ahead.set_tooltip_text(Some(&format!("{} unpushed commit(s)", indicator.ahead)));
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
                .margin_start(22) // align with name (icon width + spacing)
                .build();
            row_box.append(&branch_label);
        }
    }

    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&row_box));
    row.set_activatable(entry.is_git_repo);
    row.set_selectable(entry.is_git_repo);
    row
}

/// Populate a ListBox with workspace entries.
pub fn populate_repo_list(list_box: &gtk::ListBox, entries: &[WorkspaceEntry]) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    for entry in entries {
        list_box.append(&create_repo_row(entry));
    }
}
