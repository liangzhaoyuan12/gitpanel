use adw::prelude::*;

use gitpulsar_core::models::{CommitInfo, DiffFile};

/// Create an expandable commit row.
/// The detail section is hidden by default; click to toggle.
/// `is_head` marks the first commit (HEAD) for the edit-message button.
pub fn create_commit_row(commit: &CommitInfo, tags: &[String], is_unpushed: bool, is_head: bool) -> gtk::ListBoxRow {
    let outer_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

    // === Compact summary row ===
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row_box.set_margin_start(8);
    row_box.set_margin_end(8);
    row_box.set_margin_top(6);
    row_box.set_margin_bottom(6);

    // Hash (monospace, dim)
    let hash_label = gtk::Label::builder()
        .label(&commit.short_id)
        .css_classes(["caption", "monospace", "dim-label"])
        .valign(gtk::Align::Start)
        .build();
    row_box.append(&hash_label);

    // Message + tags + author + time
    let info_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    info_box.set_hexpand(true);

    // First line: message + tag badges
    let msg_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let message_label = gtk::Label::builder()
        .label(&commit.summary)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(60)
        .build();
    msg_row.append(&message_label);

    for tag_name in tags {
        let tag_label = gtk::Label::builder()
            .label(tag_name)
            .css_classes(["caption"])
            .valign(gtk::Align::Center)
            .build();
        let tag_frame = gtk::Frame::new(None);
        tag_frame.set_child(Some(&tag_label));
        tag_frame.add_css_class("accent");
        tag_frame.set_margin_start(2);
        msg_row.append(&tag_frame);
    }

    info_box.append(&msg_row);

    let meta = format!("{} {}", commit.author.name, format_relative_time(&commit.time));
    let meta_label = gtk::Label::builder()
        .label(&meta)
        .xalign(0.0)
        .css_classes(["caption", "dim-label"])
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    info_box.append(&meta_label);

    row_box.append(&info_box);

    // Expand indicator
    let expand_icon = gtk::Image::builder()
        .icon_name("pan-end-symbolic")
        .css_classes(["dim-label"])
        .valign(gtk::Align::Center)
        .build();
    expand_icon.set_widget_name("expand-icon");
    row_box.append(&expand_icon);

    outer_box.append(&row_box);

    // === Detail section (hidden by default) ===
    let detail_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    detail_box.set_widget_name("detail-box");
    detail_box.set_margin_start(16);
    detail_box.set_margin_end(8);
    detail_box.set_margin_bottom(8);
    detail_box.set_visible(false);
    detail_box.add_css_class("card");
    detail_box.set_margin_top(0);

    let detail_inner = gtk::Box::new(gtk::Orientation::Vertical, 4);
    detail_inner.set_margin_start(12);
    detail_inner.set_margin_end(12);
    detail_inner.set_margin_top(8);
    detail_inner.set_margin_bottom(8);

    // Full SHA (selectable/copyable)
    let sha_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    sha_row.append(&gtk::Label::builder()
        .label("SHA:")
        .css_classes(["caption", "dim-label"])
        .build());
    sha_row.append(&gtk::Label::builder()
        .label(&commit.id)
        .css_classes(["caption", "monospace"])
        .selectable(true)
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .hexpand(true)
        .xalign(0.0)
        .build());
    detail_inner.append(&sha_row);

    // Parent SHAs
    if !commit.parent_ids.is_empty() {
        let parent_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        parent_row.append(&gtk::Label::builder()
            .label("Parent:")
            .css_classes(["caption", "dim-label"])
            .build());
        let parents_text = commit.parent_ids.iter()
            .map(|p| &p[..7.min(p.len())])
            .collect::<Vec<_>>()
            .join(", ");
        parent_row.append(&gtk::Label::builder()
            .label(&parents_text)
            .css_classes(["caption", "monospace"])
            .selectable(true)
            .xalign(0.0)
            .build());
        detail_inner.append(&parent_row);
    }

    // Author + email
    let author_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    author_row.append(&gtk::Label::builder()
        .label("Author:")
        .css_classes(["caption", "dim-label"])
        .build());
    author_row.append(&gtk::Label::builder()
        .label(&format!("{} <{}>", commit.author.name, commit.author.email))
        .css_classes(["caption"])
        .selectable(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .hexpand(true)
        .xalign(0.0)
        .build());
    detail_inner.append(&author_row);

    // Date (full)
    let date_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    date_row.append(&gtk::Label::builder()
        .label("Date:")
        .css_classes(["caption", "dim-label"])
        .build());
    date_row.append(&gtk::Label::builder()
        .label(&commit.time.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .css_classes(["caption"])
        .xalign(0.0)
        .build());
    detail_inner.append(&date_row);

    // Full commit message (if differs from summary)
    let full_msg = commit.message.trim();
    if full_msg != commit.summary.trim() && !full_msg.is_empty() {
        detail_inner.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let msg_label = gtk::Label::builder()
            .label(full_msg)
            .xalign(0.0)
            .wrap(true)
            .css_classes(["caption"])
            .selectable(true)
            .build();
        detail_inner.append(&msg_label);
    }

    // Action buttons row
    if is_head {
        detail_inner.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let actions_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions_row.set_margin_top(6);
        actions_row.set_margin_bottom(2);

        let edit_msg_btn = gtk::Button::builder()
            .icon_name("document-edit-symbolic")
            .label("Edit Message")
            .css_classes(["suggested-action", "pill"])
            .build();
        edit_msg_btn.set_widget_name("edit-message-btn");
        actions_row.append(&edit_msg_btn);

        detail_inner.append(&actions_row);
    }

    // Placeholder for file list — will be populated by window.rs
    let files_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    files_box.set_widget_name("files-box");
    detail_inner.append(&files_box);

    detail_box.append(&detail_inner);
    outer_box.append(&detail_box);

    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&outer_box));

    // Unpushed indicator
    if is_unpushed {
        row.add_css_class("unpushed-commit");
    }

    row
}

/// Toggle the detail section visibility and update the expand icon.
pub fn toggle_detail(row: &gtk::ListBoxRow) -> bool {
    let Some(outer_box) = row.child().and_then(|c| c.downcast::<gtk::Box>().ok()) else {
        return false;
    };

    let detail_box = find_child_by_name(&outer_box, "detail-box");
    let expand_icon = find_child_by_name(&outer_box, "expand-icon");

    if let Some(detail) = detail_box {
        let new_visible = !detail.is_visible();
        detail.set_visible(new_visible);
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
    false
}

/// Get the files-box widget from a commit row for populating file list.
pub fn get_files_box(row: &gtk::ListBoxRow) -> Option<gtk::Box> {
    let outer_box = row.child()?.downcast::<gtk::Box>().ok()?;
    find_child_by_name(&outer_box, "files-box")
        .and_then(|w| w.downcast::<gtk::Box>().ok())
}

/// Populate the files-box with changed files from a commit diff.
pub fn populate_commit_files(files_box: &gtk::Box, files: &[DiffFile]) {
    // Clear existing
    while let Some(child) = files_box.first_child() {
        files_box.remove(&child);
    }

    if files.is_empty() {
        return;
    }

    files_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    let header = gtk::Label::builder()
        .label(&format!("Files ({})", files.len()))
        .css_classes(["caption", "dim-label"])
        .xalign(0.0)
        .margin_top(2)
        .build();
    files_box.append(&header);

    for file in files {
        let file_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        file_row.set_margin_start(4);

        // Status badge
        let (badge_text, badge_class) = diff_file_badge(file);
        let badge = gtk::Label::builder()
            .label(badge_text)
            .css_classes(["caption", "monospace", badge_class])
            .width_chars(2)
            .build();
        file_row.append(&badge);

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

        files_box.append(&file_row);
    }
}

/// Determine badge text and CSS class for a diff file.
fn diff_file_badge(file: &DiffFile) -> (&'static str, &'static str) {
    let has_additions = file.stats.insertions > 0;
    let has_deletions = file.stats.deletions > 0;

    if has_additions && !has_deletions {
        ("A", "success")
    } else if has_deletions && !has_additions {
        ("D", "error")
    } else {
        ("M", "warning")
    }
}

/// Find the "Edit Message" button inside a commit row detail section.
pub fn find_edit_message_btn(row: &gtk::ListBoxRow) -> Option<gtk::Button> {
    let outer_box = row.child()?.downcast::<gtk::Box>().ok()?;
    find_child_by_name(&outer_box, "edit-message-btn")
        .and_then(|w| w.downcast::<gtk::Button>().ok())
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
        child = c.next_sibling();
    }
    None
}

fn format_relative_time(time: &chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(*time);

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
        time.format("%Y-%m-%d").to_string()
    }
}
