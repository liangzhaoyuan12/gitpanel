use adw::prelude::*;

use gitpulsar_core::models::{CommitInfo, DiffFile};
use crate::config::DateFormat;

/// Create an expandable commit row.
/// The detail section is hidden by default; click to toggle.
/// `is_head` marks the first commit (HEAD) for the edit-message button.
pub fn create_commit_row(commit: &CommitInfo, tags: &[String], is_unpushed: bool, is_head: bool, date_format: DateFormat) -> gtk::ListBoxRow {
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

    let meta = format!("{} {}", commit.author.name, format_relative_time(&commit.time, date_format));
    let meta_label = gtk::Label::builder()
        .label(&meta)
        .xalign(0.0)
        .css_classes(["caption", "dim-label"])
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    info_box.append(&meta_label);

    row_box.append(&info_box);

    // Edit message button (pencil icon, only for HEAD commit)
    if is_head {
        let edit_msg_btn = gtk::Button::builder()
            .icon_name("document-edit-symbolic")
            .css_classes(["flat", "circular"])
            .tooltip_text("Edit Commit Message")
            .valign(gtk::Align::Center)
            .build();
        edit_msg_btn.set_widget_name("edit-message-btn");
        row_box.append(&edit_msg_btn);
    }

    // Unpushed indicator (green dot)
    if is_unpushed {
        let dot = gtk::Label::builder()
            .label("●")
            .css_classes(["success"])
            .tooltip_text("Not pushed")
            .valign(gtk::Align::Center)
            .build();
        row_box.append(&dot);
    }

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

    // --- 1) Commit message (prominent) ---
    let summary_label = gtk::Label::builder()
        .label(&commit.summary)
        .xalign(0.0)
        .wrap(true)
        .selectable(true)
        .css_classes(["title-4"])
        .build();
    detail_inner.append(&summary_label);

    // Body (if differs from summary)
    let full_msg = commit.message.trim();
    let summary_trimmed = commit.summary.trim();
    if full_msg.len() > summary_trimmed.len() {
        let body = full_msg.strip_prefix(summary_trimmed).unwrap_or(full_msg).trim();
        if !body.is_empty() {
            let body_label = gtk::Label::builder()
                .label(body)
                .xalign(0.0)
                .wrap(true)
                .selectable(true)
                .margin_top(4)
                .build();
            detail_inner.append(&body_label);
        }
    }

    // --- 2) Files list (populated later by window.rs) ---
    let files_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    files_box.set_widget_name("files-box");
    files_box.set_margin_top(8);
    detail_inner.append(&files_box);

    // --- 3) Technical details (collapsed by default) ---
    let details_expander = gtk::Box::new(gtk::Orientation::Vertical, 0);
    details_expander.set_margin_top(8);

    let details_toggle_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let details_toggle_icon = gtk::Image::builder()
        .icon_name("pan-end-symbolic")
        .css_classes(["dim-label"])
        .build();
    details_toggle_icon.set_widget_name("details-toggle-icon");
    details_toggle_row.append(&details_toggle_icon);
    let details_toggle_label = gtk::Label::builder()
        .label("Details")
        .css_classes(["caption", "dim-label"])
        .build();
    details_toggle_row.append(&details_toggle_label);

    let details_btn = gtk::Button::builder()
        .child(&details_toggle_row)
        .css_classes(["flat"])
        .build();
    details_expander.append(&details_btn);

    let details_content = gtk::Box::new(gtk::Orientation::Vertical, 4);
    details_content.set_widget_name("details-content");
    details_content.set_visible(false);
    details_content.set_margin_start(4);
    details_content.set_margin_top(4);

    // SHA
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
    details_content.append(&sha_row);

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
        details_content.append(&parent_row);
    }

    // Author
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
    details_content.append(&author_row);

    // Date
    let date_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    date_row.append(&gtk::Label::builder()
        .label("Date:")
        .css_classes(["caption", "dim-label"])
        .build());
    date_row.append(&gtk::Label::builder()
        .label(&date_format.format_datetime(&commit.time))
        .css_classes(["caption"])
        .xalign(0.0)
        .build());
    details_content.append(&date_row);

    details_expander.append(&details_content);

    // Toggle details visibility on button click
    {
        let details_content = details_content.clone();
        let details_toggle_icon = details_toggle_icon.clone();
        details_btn.connect_clicked(move |_| {
            let new_visible = !details_content.is_visible();
            details_content.set_visible(new_visible);
            details_toggle_icon.set_icon_name(Some(if new_visible {
                "pan-down-symbolic"
            } else {
                "pan-end-symbolic"
            }));
        });
    }

    detail_inner.append(&details_expander);

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

/// Show a spinner in the files-box while loading commit files.
pub fn show_files_loading(files_box: &gtk::Box) {
    while let Some(child) = files_box.first_child() {
        files_box.remove(&child);
    }
    let spinner = gtk::Spinner::builder()
        .spinning(true)
        .halign(gtk::Align::Center)
        .margin_top(4)
        .margin_bottom(4)
        .build();
    files_box.append(&spinner);
}

/// Get the files-box widget from a commit row for populating file list.
pub fn get_files_box(row: &gtk::ListBoxRow) -> Option<gtk::Box> {
    let outer_box = row.child()?.downcast::<gtk::Box>().ok()?;
    find_child_by_name(&outer_box, "files-box")
        .and_then(|w| w.downcast::<gtk::Box>().ok())
}

/// Populate the files-box with changed files from a commit diff.
/// `limit` controls how many files are shown before a "Show all" expander.
/// Pass 0 to show all files without limit.
pub fn populate_commit_files(files_box: &gtk::Box, files: &[DiffFile], limit: u32) {
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

    let limit = limit as usize;
    let needs_expander = limit > 0 && files.len() > limit;
    let visible_count = if needs_expander { limit } else { files.len() };

    // Show files up to the limit
    for file in &files[..visible_count] {
        files_box.append(&build_file_row(file));
    }

    // "Show all" expander for remaining files
    if needs_expander {
        let remaining = &files[visible_count..];
        let overflow_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        overflow_box.set_visible(false);

        for file in remaining {
            overflow_box.append(&build_file_row(file));
        }

        let toggle_label_text = format!("Show all ({} more)", remaining.len());
        let toggle_btn = gtk::Button::builder()
            .label(&toggle_label_text)
            .css_classes(["flat", "caption"])
            .halign(gtk::Align::Start)
            .margin_top(2)
            .build();

        {
            let overflow_box = overflow_box.clone();
            let collapsed_label = format!("Show all ({} more)", remaining.len());
            toggle_btn.connect_clicked(move |btn| {
                let new_visible = !overflow_box.is_visible();
                overflow_box.set_visible(new_visible);
                btn.set_label(if new_visible {
                    "Show less"
                } else {
                    &collapsed_label
                });
            });
        }

        files_box.append(&toggle_btn);
        files_box.append(&overflow_box);
    }
}

fn build_file_row(file: &DiffFile) -> gtk::Box {
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

    file_row
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

fn format_relative_time(time: &chrono::DateTime<chrono::Utc>, date_format: DateFormat) -> String {
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
        date_format.format_date(time)
    }
}
