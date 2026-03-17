use adw::prelude::*;

use gitpulsar_core::models::{DiffFile, DiffLineKind, FileStatusKind, RepoStatus};

use super::syntax;

/// Refs returned to window.rs for connecting signals.
pub struct ChangesViewRefs {
    pub unstaged_list_box: gtk::ListBox,
    pub staged_list_box: gtk::ListBox,
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

    // Conventional commit prefix button
    let commit_entry_for_template = commit_entry.clone();
    let template_btn = super::commit_templates::build_template_button(move |prefix| {
        super::commit_templates::insert_prefix(&commit_entry_for_template.buffer(), prefix);
    });
    action_row.append(&template_btn);

    action_row.append(amend_check);

    commit_button.set_label("Commit");
    commit_button.add_css_class("suggested-action");
    commit_button.add_css_class("pill");
    action_row.append(commit_button);

    commit_bar.append(&action_row);
    container.append(&commit_bar);

    container.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // === Scrollable area with two file lists ===
    let lists_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

    // Unstaged section
    let unstaged_header = gtk::Label::builder()
        .label("Unstaged Changes")
        .css_classes(["caption", "dim-label"])
        .xalign(0.0)
        .margin_start(8)
        .margin_top(6)
        .margin_bottom(2)
        .build();
    lists_box.append(&unstaged_header);

    let unstaged_list_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["navigation-sidebar"])
        .build();
    let unstaged_placeholder = gtk::Label::builder()
        .label("No unstaged changes")
        .css_classes(["dim-label"])
        .margin_top(12)
        .margin_bottom(12)
        .build();
    unstaged_list_box.set_placeholder(Some(&unstaged_placeholder));

    // DnD: drop target on unstaged list (accepts staged files to unstage)
    let drop_unstaged = gtk::DropTarget::builder()
        .actions(gtk::gdk::DragAction::MOVE)
        .build();
    drop_unstaged.set_types(&[gtk::glib::Type::STRING]);
    unstaged_list_box.add_controller(drop_unstaged.clone());

    lists_box.append(&unstaged_list_box);
    lists_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // Staged section
    let staged_header = gtk::Label::builder()
        .label("Staged Changes")
        .css_classes(["caption", "dim-label"])
        .xalign(0.0)
        .margin_start(8)
        .margin_top(6)
        .margin_bottom(2)
        .build();
    lists_box.append(&staged_header);

    let staged_list_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["navigation-sidebar"])
        .build();
    let staged_placeholder = gtk::Label::builder()
        .label("No staged changes")
        .css_classes(["dim-label"])
        .margin_top(12)
        .margin_bottom(12)
        .build();
    staged_list_box.set_placeholder(Some(&staged_placeholder));

    // DnD: drop target on staged list (accepts unstaged files to stage)
    let drop_staged = gtk::DropTarget::builder()
        .actions(gtk::gdk::DragAction::MOVE)
        .build();
    drop_staged.set_types(&[gtk::glib::Type::STRING]);
    staged_list_box.add_controller(drop_staged.clone());

    lists_box.append(&staged_list_box);

    let file_scrolled = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    file_scrolled.set_child(Some(&lists_box));
    container.append(&file_scrolled);

    let refs = ChangesViewRefs {
        unstaged_list_box,
        staged_list_box,
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

/// Populate both file lists (unstaged and staged) from the collected files.
pub fn populate_file_lists(
    unstaged_list: &gtk::ListBox,
    staged_list: &gtk::ListBox,
    files: &[ChangedFileEntry],
) {
    while let Some(child) = unstaged_list.first_child() {
        unstaged_list.remove(&child);
    }
    while let Some(child) = staged_list.first_child() {
        staged_list.remove(&child);
    }

    for file in files {
        let row = create_file_accordion_row(file);
        // Add DnD source to each row
        let path = file.path.clone();
        let drag_source = gtk::DragSource::builder()
            .actions(gtk::gdk::DragAction::MOVE)
            .build();
        drag_source.connect_prepare(move |_, _, _| {
            Some(gtk::gdk::ContentProvider::for_value(&gtk::glib::Value::from(&path)))
        });
        row.add_controller(drag_source);

        if file.is_staged {
            staged_list.append(&row);
        } else {
            unstaged_list.append(&row);
        }
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

    // Status icon
    let (icon_name, icon_css, tooltip) = match file.status {
        FileStatusKind::New => ("list-add-symbolic", "success", "Added"),
        FileStatusKind::Modified => ("document-edit-symbolic", "accent", "Modified"),
        FileStatusKind::Deleted => ("list-remove-symbolic", "error", "Deleted"),
        FileStatusKind::Renamed => ("edit-find-replace-symbolic", "accent", "Renamed"),
        FileStatusKind::Typechange => ("dialog-warning-symbolic", "warning", "Typechange"),
    };

    let status_icon = gtk::Image::builder()
        .icon_name(icon_name)
        .css_classes([icon_css])
        .pixel_size(14)
        .tooltip_text(tooltip)
        .build();
    header.append(&status_icon);

    // Staged indicator
    if file.is_staged {
        let staged_icon = gtk::Image::builder()
            .icon_name("object-select-symbolic")
            .css_classes(["success"])
            .pixel_size(12)
            .tooltip_text("Staged")
            .build();
        header.append(&staged_icon);
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

    // Blame button (always available)
    let blame_btn = gtk::Button::builder()
        .icon_name("view-list-symbolic")
        .css_classes(["flat", "circular"])
        .tooltip_text("Blame")
        .valign(gtk::Align::Center)
        .build();
    blame_btn.set_widget_name("blame-file");
    btn_box.append(&blame_btn);

    header.append(&btn_box);
    outer_box.append(&header);

    // === Diff section (hidden by default, with animation) ===
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

    // Hunk action buttons container (populated when diff is loaded)
    let hunk_actions_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    hunk_actions_box.set_widget_name("hunk-actions-box");
    diff_box.append(&hunk_actions_box);

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
    diff_revealer.set_child(Some(&diff_box));
    outer_box.append(&diff_revealer);

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

    let is_dark = adw::StyleManager::default().is_dark();

    // Setup diff tags (recreate on each render to handle theme changes)
    let tag_table = buffer.tag_table();
    for name in &["addition", "deletion", "hunk-header", "lineno"] {
        if let Some(tag) = tag_table.lookup(name) {
            tag_table.remove(&tag);
        }
    }

    let (add_bg, add_fg, del_bg, del_fg, hunk_bg, hunk_fg, lineno_fg) = if is_dark {
        ("#1a3a2a", "#a3d9a5", "#3a1a1a", "#d9a3a3", "#1a2a3a", "#6cb6ff", "#6e7681")
    } else {
        ("#d4edda", "#155724", "#f8d7da", "#721c24", "#ddf4ff", "#0550ae", "#8b949e")
    };

    tag_table.add(&gtk::TextTag::builder().name("addition").background(add_bg).foreground(add_fg).build());
    tag_table.add(&gtk::TextTag::builder().name("deletion").background(del_bg).foreground(del_fg).build());
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

    for hunk in &file.hunks {
        // Hunk header
        let start = iter.offset();
        buffer.insert(&mut iter, &format!("{}\n", hunk.header));
        let start_iter = buffer.iter_at_offset(start);
        buffer.apply_tag_by_name("hunk-header", &start_iter, &iter);

        for line in &hunk.lines {
            let prefix = match line.kind {
                DiffLineKind::Addition => "+",
                DiffLineKind::Deletion => "-",
                DiffLineKind::Context => " ",
            };

            let line_start = iter.offset();
            let text = format!("{}{}\n", prefix, line.content);
            buffer.insert(&mut iter, &text);

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
                    let content_offset = line_start + prefix.len() as i32;
                    for span in spans {
                        let tag_name = format!("syn_{:02x}{:02x}{:02x}", span.fg.0, span.fg.1, span.fg.2);
                        if tag_table.lookup(&tag_name).is_none() {
                            let color = format!("#{:02x}{:02x}{:02x}", span.fg.0, span.fg.1, span.fg.2);
                            let tag = gtk::TextTag::builder()
                                .name(&tag_name)
                                .foreground(&color)
                                .foreground_set(true)
                                .build();
                            tag.set_priority(tag_table.size() - 1);
                            tag_table.add(&tag);
                        }
                        let s = buffer.iter_at_offset(content_offset + span.start as i32);
                        let e = buffer.iter_at_offset(content_offset + span.end as i32);
                        buffer.apply_tag_by_name(&tag_name, &s, &e);
                    }
                }
            }

            global_line_idx += 1;
            line_count += 1;
        }
    }

    // Set height based on content (cap at ~25 lines)
    let visible_lines = line_count.min(25).max(3);
    textview.set_height_request(visible_lines as i32 * 18);
}

/// Get the hunk-actions-box from a file row.
pub fn get_hunk_actions_box(row: &gtk::ListBoxRow) -> Option<gtk::Box> {
    let outer_box = row.child()?.downcast::<gtk::Box>().ok()?;
    find_child_by_name(&outer_box, "hunk-actions-box")
        .and_then(|w| w.downcast::<gtk::Box>().ok())
}

/// Populate hunk action buttons for a file diff.
/// `is_staged` determines whether buttons say "Unstage Hunk" or "Stage Hunk".
/// Populate hunk action buttons for a file diff.
/// `is_staged` determines whether buttons say "Unstage Hunk" or "Stage Hunk".
/// `hunks` are the actual diff hunks for building line selectors.
pub fn populate_hunk_actions(
    hunk_box: &gtk::Box,
    hunks: &[gitpulsar_core::models::DiffHunk],
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

    // Hunk-level buttons (always show if multiple hunks)
    if num_hunks > 1 {
        for i in 0..num_hunks {
            let label = if is_staged {
                format!("Unstage Hunk {}", i + 1)
            } else {
                format!("Stage Hunk {}", i + 1)
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
    }

    // "Select Lines" toggle — opens line-level selection UI
    let select_lines_btn = gtk::Button::builder()
        .label("Select Lines")
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
        .label(if is_staged { "Unstage Selected Lines" } else { "Stage Selected Lines" })
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
            btn.set_label(if visible { "Hide Lines" } else { "Select Lines" });
        });
    }
}

/// Build a line-selection ListBox for a single hunk.
/// Returns the box and a closure to collect selected line indices.
pub fn build_line_selector(hunk: &gitpulsar_core::models::DiffHunk, hunk_index: usize) -> gtk::Box {
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
            .label(&format!("{}{}", prefix, line.content.trim_end()))
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
