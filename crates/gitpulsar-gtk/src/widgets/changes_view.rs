use adw::prelude::*;

use gitpulsar_core::models::{DiffFile, FileStatusKind, RepoStatus};

/// Refs returned to window.rs for connecting signals.
pub struct ChangesViewRefs {
    pub file_list_box: gtk::ListBox,
    pub commit_entry: gtk::TextView,
    pub commit_button: gtk::Button,
    pub amend_check: gtk::CheckButton,
    pub stage_all_btn: gtk::Button,
    pub unstage_all_btn: gtk::Button,
}

/// Build the changes tab content.
/// Top: compact action bar (Stage All | Unstage All | Commit msg | Commit btn)
/// Below: scrollable list of file accordion rows.
pub fn build_changes_view(
    commit_entry: &gtk::TextView,
    commit_button: &gtk::Button,
    amend_check: &gtk::CheckButton,
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
        .label("Commit message")
        .css_classes(["dim-label"])
        .xalign(0.0)
        .yalign(0.0)
        .margin_start(12)
        .margin_top(8)
        .can_focus(false)
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
        .label("Stage All")
        .css_classes(["flat", "caption"])
        .build();
    action_row.append(&stage_all_btn);

    let unstage_all_btn = gtk::Button::builder()
        .label("Unstage All")
        .css_classes(["flat", "caption"])
        .build();
    action_row.append(&unstage_all_btn);

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    action_row.append(&spacer);

    action_row.append(amend_check);

    commit_button.set_label("Commit");
    commit_button.add_css_class("suggested-action");
    commit_button.add_css_class("pill");
    action_row.append(commit_button);

    commit_bar.append(&action_row);
    container.append(&commit_bar);

    container.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // === File list (accordion) ===
    let file_list_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["navigation-sidebar"])
        .build();

    let file_placeholder = gtk::Label::builder()
        .label("No changes")
        .css_classes(["dim-label"])
        .margin_top(24)
        .margin_bottom(24)
        .build();
    file_list_box.set_placeholder(Some(&file_placeholder));

    let file_scrolled = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    file_scrolled.set_child(Some(&file_list_box));
    container.append(&file_scrolled);

    let refs = ChangesViewRefs {
        file_list_box,
        commit_entry: commit_entry.clone(),
        commit_button: commit_button.clone(),
        amend_check: amend_check.clone(),
        stage_all_btn,
        unstage_all_btn,
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

/// Populate the file list with accordion rows.
pub fn populate_file_list(list_box: &gtk::ListBox, files: &[ChangedFileEntry]) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    for file in files {
        let row = create_file_accordion_row(file);
        list_box.append(&row);
    }
}

/// Create a file accordion row: compact header (badge + path + stage/unstage/discard buttons),
/// expandable diff section below.
fn create_file_accordion_row(file: &ChangedFileEntry) -> gtk::ListBoxRow {
    let outer_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

    // === Header row ===
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    header.set_margin_start(8);
    header.set_margin_end(4);
    header.set_margin_top(4);
    header.set_margin_bottom(4);

    // Expand icon
    let expand_icon = gtk::Image::builder()
        .icon_name("pan-end-symbolic")
        .css_classes(["dim-label"])
        .build();
    expand_icon.set_widget_name("expand-icon");
    header.append(&expand_icon);

    // Status badge
    let (badge_text, badge_css) = match file.status {
        FileStatusKind::New => ("A", "success"),
        FileStatusKind::Modified => ("M", "accent"),
        FileStatusKind::Deleted => ("D", "error"),
        FileStatusKind::Renamed => ("R", "accent"),
        FileStatusKind::Typechange => ("T", "warning"),
    };

    let badge = gtk::Label::builder()
        .label(badge_text)
        .css_classes(["caption", badge_css])
        .width_chars(2)
        .build();
    header.append(&badge);

    // Staged indicator
    if file.is_staged {
        let staged_badge = gtk::Label::builder()
            .label("S")
            .css_classes(["caption", "success"])
            .tooltip_text("Staged")
            .build();
        header.append(&staged_badge);
    }

    // File path
    let path_label = gtk::Label::builder()
        .label(&file.path)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::Start)
        .css_classes(["caption"])
        .build();
    header.append(&path_label);

    // Action buttons (shown on hover)
    let btn_box = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    btn_box.set_visible(false);

    if file.is_staged {
        let unstage_btn = gtk::Button::builder()
            .icon_name("list-remove-symbolic")
            .css_classes(["flat", "circular"])
            .tooltip_text("Unstage")
            .valign(gtk::Align::Center)
            .build();
        unstage_btn.set_widget_name("unstage-file");
        btn_box.append(&unstage_btn);
    } else {
        let stage_btn = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .css_classes(["flat", "circular"])
            .tooltip_text("Stage")
            .valign(gtk::Align::Center)
            .build();
        stage_btn.set_widget_name("stage-file");
        btn_box.append(&stage_btn);

        let discard_btn = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .css_classes(["flat", "circular"])
            .tooltip_text("Discard")
            .valign(gtk::Align::Center)
            .build();
        discard_btn.set_widget_name("discard-file");
        btn_box.append(&discard_btn);
    }

    header.append(&btn_box);
    outer_box.append(&header);

    // === Diff section (hidden by default) ===
    let diff_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    diff_box.set_widget_name("diff-box");
    diff_box.set_visible(false);
    diff_box.set_margin_start(16);
    diff_box.set_margin_end(8);
    diff_box.set_margin_bottom(4);

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

    // Will be sized dynamically when diff is loaded
    diff_box.append(&diff_text);
    outer_box.append(&diff_box);

    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&outer_box));
    row.set_widget_name(&file.path);

    // Hover to show buttons
    let bb = btn_box.clone();
    let hover = gtk::EventControllerMotion::new();
    hover.connect_enter(move |_, _, _| {
        bb.set_visible(true);
    });
    let bb2 = btn_box;
    hover.connect_leave(move |_| {
        bb2.set_visible(false);
    });
    row.add_controller(hover);

    row
}

/// Toggle the diff section of a file row and return whether it's now expanded.
pub fn toggle_file_diff(row: &gtk::ListBoxRow) -> bool {
    let Some(outer_box) = row.child().and_then(|c| c.downcast::<gtk::Box>().ok()) else {
        return false;
    };

    let diff_box = find_child_by_name(&outer_box, "diff-box");
    let expand_icon = find_child_by_name(&outer_box, "expand-icon");

    if let Some(diff) = diff_box {
        let new_visible = !diff.is_visible();
        diff.set_visible(new_visible);
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

/// Get the diff TextView from a file row for rendering diff content.
pub fn get_diff_textview(row: &gtk::ListBoxRow) -> Option<gtk::TextView> {
    let outer_box = row.child()?.downcast::<gtk::Box>().ok()?;
    find_child_by_name(&outer_box, "diff-textview")
        .and_then(|w| w.downcast::<gtk::TextView>().ok())
}

/// Render a single file's diff into the accordion's textview using tags.
pub fn render_file_diff(textview: &gtk::TextView, file: &DiffFile) {
    let buffer = textview.buffer();
    buffer.set_text("");

    // Setup tags if not already
    let tag_table = buffer.tag_table();
    if tag_table.lookup("addition").is_none() {
        let addition_tag = gtk::TextTag::builder()
            .name("addition")
            .background("#d4edda")
            .foreground("#155724")
            .build();
        tag_table.add(&addition_tag);

        let deletion_tag = gtk::TextTag::builder()
            .name("deletion")
            .background("#f8d7da")
            .foreground("#721c24")
            .build();
        tag_table.add(&deletion_tag);

        let hunk_tag = gtk::TextTag::builder()
            .name("hunk-header")
            .background("#ddf4ff")
            .foreground("#0550ae")
            .build();
        tag_table.add(&hunk_tag);

        let lineno_tag = gtk::TextTag::builder()
            .name("lineno")
            .foreground("#8b949e")
            .build();
        tag_table.add(&lineno_tag);
    }

    let mut iter = buffer.end_iter();
    let mut line_count = 0;

    for hunk in &file.hunks {
        // Hunk header
        let start = iter.offset();
        buffer.insert(&mut iter, &format!("{}\n", hunk.header));
        let start_iter = buffer.iter_at_offset(start);
        buffer.apply_tag_by_name("hunk-header", &start_iter, &iter);

        for line in &hunk.lines {
            let prefix = match line.kind {
                gitpulsar_core::models::DiffLineKind::Addition => "+",
                gitpulsar_core::models::DiffLineKind::Deletion => "-",
                gitpulsar_core::models::DiffLineKind::Context => " ",
            };

            let start = iter.offset();
            buffer.insert(&mut iter, &format!("{}{}\n", prefix, line.content));

            let tag_name = match line.kind {
                gitpulsar_core::models::DiffLineKind::Addition => Some("addition"),
                gitpulsar_core::models::DiffLineKind::Deletion => Some("deletion"),
                gitpulsar_core::models::DiffLineKind::Context => None,
            };

            if let Some(tag) = tag_name {
                let start_iter = buffer.iter_at_offset(start);
                buffer.apply_tag_by_name(tag, &start_iter, &iter);
            }

            line_count += 1;
        }
    }

    // Set height based on content (cap at ~20 lines)
    let visible_lines = line_count.min(25).max(3);
    textview.set_height_request(visible_lines as i32 * 18);
}

/// Get the file path from a changes row.
pub fn get_row_file_path(row: &gtk::ListBoxRow) -> Option<String> {
    let name = row.widget_name().to_string();
    if name.is_empty() { None } else { Some(name) }
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
        child = c.next_sibling();
    }
    None
}
