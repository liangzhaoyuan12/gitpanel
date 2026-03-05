use adw::prelude::*;

use gitpulsar_core::models::{FileStatus, FileStatusKind};

/// Buttons returned so window.rs can connect signals to them.
pub struct StagingButtons {
    pub stage_all_btn: gtk::Button,
    pub unstage_all_btn: gtk::Button,
    pub amend_check: gtk::CheckButton,
}

/// Build the staging panel. Returns (container, buttons).
pub fn build_staging_panel(
    unstaged_list: &gtk::ListBox,
    staged_list: &gtk::ListBox,
    commit_entry: &gtk::TextView,
    commit_button: &gtk::Button,
    amend_check: &gtk::CheckButton,
) -> (gtk::Box, StagingButtons) {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    container.set_vexpand(true);

    // Separator at top
    container.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // === Unstaged Changes ===
    let stage_all_btn = gtk::Button::builder()
        .label("Stage All")
        .css_classes(["flat"])
        .build();
    let unstaged_section = build_section("Unstaged Changes", &stage_all_btn, unstaged_list);
    unstaged_section.set_vexpand(true);
    container.append(&unstaged_section);

    container.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // === Staged Changes ===
    let unstage_all_btn = gtk::Button::builder()
        .label("Unstage All")
        .css_classes(["flat"])
        .build();
    let staged_section = build_section("Staged Changes", &unstage_all_btn, staged_list);
    staged_section.set_vexpand(true);
    container.append(&staged_section);

    container.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // === Commit area ===
    let commit_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    commit_box.set_margin_start(12);
    commit_box.set_margin_end(12);
    commit_box.set_margin_top(8);
    commit_box.set_margin_bottom(12);

    commit_entry.set_wrap_mode(gtk::WrapMode::Word);
    commit_entry.set_top_margin(8);
    commit_entry.set_bottom_margin(8);
    commit_entry.set_left_margin(8);
    commit_entry.set_right_margin(8);
    commit_entry.set_height_request(72);
    commit_entry.add_css_class("card");

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(commit_entry));

    let placeholder_label = gtk::Label::builder()
        .label("Commit message")
        .css_classes(["dim-label"])
        .xalign(0.0)
        .yalign(0.0)
        .margin_start(12)
        .margin_top(10)
        .can_focus(false)
        .build();
    overlay.add_overlay(&placeholder_label);

    let pl = placeholder_label.clone();
    commit_entry.buffer().connect_changed(move |buf| {
        pl.set_visible(buf.char_count() == 0);
    });

    commit_box.append(&overlay);

    let commit_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    commit_button.set_label("Commit");
    commit_button.add_css_class("suggested-action");
    commit_button.add_css_class("pill");
    commit_button.set_hexpand(true);
    commit_row.append(commit_button);
    commit_row.append(amend_check);
    commit_box.append(&commit_row);

    container.append(&commit_box);

    let buttons = StagingButtons {
        stage_all_btn,
        unstage_all_btn,
        amend_check: amend_check.clone(),
    };

    (container, buttons)
}

fn build_section(title: &str, action_btn: &gtk::Button, list_box: &gtk::ListBox) -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    header.set_margin_start(12);
    header.set_margin_end(8);
    header.set_margin_top(8);
    header.set_margin_bottom(4);

    let label = gtk::Label::builder()
        .label(title)
        .css_classes(["heading"])
        .hexpand(true)
        .xalign(0.0)
        .build();
    header.append(&label);
    header.append(action_btn);

    section.append(&header);

    let scrolled = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .min_content_height(60)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();

    list_box.set_selection_mode(gtk::SelectionMode::Single);
    list_box.add_css_class("navigation-sidebar");

    let placeholder = gtk::Label::builder()
        .label("No changes")
        .css_classes(["dim-label"])
        .margin_top(16)
        .margin_bottom(16)
        .build();
    list_box.set_placeholder(Some(&placeholder));

    scrolled.set_child(Some(list_box));
    section.append(&scrolled);

    section
}

/// Populate the unstaged file list (modified + untracked).
/// Each row has stage (+) and discard (x) buttons.
pub fn populate_file_list(
    list_box: &gtk::ListBox,
    unstaged: &[FileStatus],
    untracked: &[String],
) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    for file in unstaged {
        list_box.append(&create_file_row(&file.path, file.status, RowKind::Unstaged));
    }

    for path in untracked {
        list_box.append(&create_file_row(path, FileStatusKind::New, RowKind::Unstaged));
    }
}

/// Populate the staged file list. Each row has unstage (-) button.
pub fn populate_staged_list(list_box: &gtk::ListBox, staged: &[FileStatus]) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    for file in staged {
        list_box.append(&create_file_row(&file.path, file.status, RowKind::Staged));
    }
}

#[derive(Clone, Copy)]
enum RowKind {
    Unstaged,
    Staged,
}

fn create_file_row(path: &str, status: FileStatusKind, kind: RowKind) -> gtk::ListBoxRow {
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    row_box.set_margin_start(8);
    row_box.set_margin_end(4);
    row_box.set_margin_top(3);
    row_box.set_margin_bottom(3);

    let (badge_text, badge_css) = match status {
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
    row_box.append(&badge);

    let path_label = gtk::Label::builder()
        .label(path)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::Start)
        .css_classes(["caption"])
        .build();
    row_box.append(&path_label);

    // Button container — hidden by default, shown on hover
    let btn_box = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    btn_box.set_visible(false);

    match kind {
        RowKind::Unstaged => {
            let stage_btn = gtk::Button::builder()
                .icon_name("list-add-symbolic")
                .css_classes(["flat", "circular"])
                .tooltip_text("Stage file")
                .valign(gtk::Align::Center)
                .build();
            stage_btn.set_widget_name("stage-file");
            btn_box.append(&stage_btn);

            let discard_btn = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .css_classes(["flat", "circular"])
                .tooltip_text("Discard changes")
                .valign(gtk::Align::Center)
                .build();
            discard_btn.set_widget_name("discard-file");
            btn_box.append(&discard_btn);
        }
        RowKind::Staged => {
            let unstage_btn = gtk::Button::builder()
                .icon_name("list-remove-symbolic")
                .css_classes(["flat", "circular"])
                .tooltip_text("Unstage file")
                .valign(gtk::Align::Center)
                .build();
            unstage_btn.set_widget_name("unstage-file");
            btn_box.append(&unstage_btn);
        }
    }

    row_box.append(&btn_box);

    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&row_box));

    // Show buttons on hover
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

/// Extract file path from a staging row (second label child).
pub fn get_row_file_path(row: &gtk::ListBoxRow) -> Option<String> {
    let row_box = row.child()?.downcast::<gtk::Box>().ok()?;
    // Children: badge, path_label, [buttons...]
    let mut child = row_box.first_child();
    // Skip badge
    child = child?.next_sibling();
    // path_label
    let label = child?.downcast::<gtk::Label>().ok()?;
    Some(label.text().to_string())
}
