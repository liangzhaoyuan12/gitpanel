//! Dialog showing one file of a commit at a time, side-by-side.
//!
//! Commit rows used to expand a file's diff inline, inside a `ListView` row —
//! a tall scrollable widget nested in a virtualized list, which scrolled
//! unpredictably and often refused to collapse. The diff lives here instead.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;

use crate::model::DiffFile;

use crate::i18n::{self, Key};
use super::diff_view;

/// Handles the caller (and the tests) need after the dialog is built.
pub struct CommitDiffDialogRefs {
    pub dialog: adw::Dialog,
    /// The window only needs `dialog`; these two are how the tests inspect the
    /// dialog's state without walking its widget tree.
    #[allow(dead_code)]
    pub stack: gtk::Stack,
    #[allow(dead_code)]
    pub file_list: gtk::ListBox,
}

/// Build the diff dialog for one commit, opened on `initial_path`.
///
/// `files` is the commit's diff as already cached on its `CommitObject`; the
/// dialog performs no git reads of its own.
pub fn build_commit_diff_dialog(
    short_id: &str,
    summary: &str,
    files: &[DiffFile],
    initial_path: &str,
) -> CommitDiffDialogRefs {
    let dialog = adw::Dialog::builder()
        .title(short_id)
        .content_width(1000)
        .content_height(700)
        // A dialog without a minimum size warns and cannot be sized down on a
        // phone; match the main window's floor.
        .width_request(360)
        .height_request(294)
        .build();

    // --- Diff panes -------------------------------------------------------
    let (left_view, left_scroll) = build_diff_pane();
    let (right_view, right_scroll) = build_diff_pane();
    diff_view::sync_scroll(&left_scroll, &right_scroll);

    let split_page = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    split_page.append(&left_scroll);
    split_page.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    split_page.append(&right_scroll);

    let (unified_view, unified_scroll) = build_diff_pane();

    let stack = gtk::Stack::new();
    stack.add_named(&split_page, Some("split"));
    stack.add_named(&unified_scroll, Some("unified"));
    stack.set_visible_child_name("split");

    // --- Rendering --------------------------------------------------------
    let files_rc = Rc::new(files.to_vec());
    let render: Rc<dyn Fn(&str)> = {
        let files_rc = files_rc.clone();
        let left = left_view.buffer();
        let right = right_view.buffer();
        let unified = unified_view.buffer();
        Rc::new(move |path: &str| {
            let Some(file) = files_rc.iter().find(|f| f.path == path) else {
                return;
            };
            let one = std::slice::from_ref(file);
            diff_view::render_side_by_side(&left, &right, one);
            diff_view::render_unified(&unified, one);
        })
    };

    // --- File list --------------------------------------------------------
    let file_list = gtk::ListBox::new();
    file_list.add_css_class("navigation-sidebar");
    file_list.set_selection_mode(gtk::SelectionMode::Single);
    for file in files_rc.iter() {
        file_list.append(&build_file_row(file));
    }

    let render_on_select = render.clone();
    file_list.connect_row_selected(move |_, row| {
        if let Some(row) = row {
            render_on_select(&row.widget_name());
        }
    });

    let initial_row = find_row_by_path(&file_list, initial_path).or_else(|| file_list.row_at_index(0));
    if let Some(row) = initial_row {
        file_list.select_row(Some(&row));
    }

    let file_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&file_list)
        .build();

    let sidebar_toolbar = adw::ToolbarView::new();
    sidebar_toolbar.set_content(Some(&file_scroll));

    let split_view = adw::OverlaySplitView::builder()
        .sidebar(&sidebar_toolbar)
        .content(&stack)
        .min_sidebar_width(180.0)
        .max_sidebar_width(280.0)
        .show_sidebar(true)
        .build();

    // --- Header -----------------------------------------------------------
    let header = adw::HeaderBar::new();

    let sidebar_toggle = gtk::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text("Toggle File List")
        .active(true)
        .build();
    header.pack_start(&sidebar_toggle);

    let title = adw::WindowTitle::new(short_id, summary);
    header.set_title_widget(Some(&title));

    let view_toggle = gtk::ToggleButton::builder()
        .icon_name(split_icon_name())
        .tooltip_text(i18n::t(Key::side_by_side))
        .active(true)
        .build();
    header.pack_end(&view_toggle);

    let sv = split_view.clone();
    sidebar_toggle.connect_toggled(move |btn| sv.set_show_sidebar(btn.is_active()));
    let sb_toggle = sidebar_toggle.clone();
    split_view.connect_show_sidebar_notify(move |split| {
        if sb_toggle.is_active() != split.shows_sidebar() {
            sb_toggle.set_active(split.shows_sidebar());
        }
    });

    let stack_for_toggle = stack.clone();
    view_toggle.connect_toggled(move |btn| {
        stack_for_toggle.set_visible_child_name(if btn.is_active() {
            "split"
        } else {
            "unified"
        });
    });
    // The breakpoint switches the stack directly; keep the button honest.
    let vt = view_toggle.clone();
    stack.connect_visible_child_name_notify(move |stack| {
        let split_shown = stack.visible_child_name().as_deref() == Some("split");
        if vt.is_active() != split_shown {
            vt.set_active(split_shown);
        }
    });

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&split_view));
    dialog.set_child(Some(&toolbar));

    // --- Adaptivity -------------------------------------------------------
    // Two panes do not fit a phone; below the breakpoint the file list becomes
    // an overlay and the diff falls back to the unified rendering.
    let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        700.0,
        adw::LengthUnit::Sp,
    ));
    breakpoint.add_setter(&split_view, "collapsed", Some(&true.to_value()));
    breakpoint.add_setter(&split_view, "show-sidebar", Some(&false.to_value()));
    breakpoint.add_setter(&stack, "visible-child-name", Some(&"unified".to_value()));
    dialog.add_breakpoint(breakpoint);

    // --- Theme ------------------------------------------------------------
    // Tags are restyled by the renderer, so re-render whatever is on screen.
    let render_for_theme = render.clone();
    let list_for_theme = file_list.clone();
    let guard = Rc::new(Cell::new(false));
    adw::StyleManager::default().connect_dark_notify(move |_| {
        if guard.get() {
            return;
        }
        guard.set(true);
        if let Some(row) = list_for_theme.selected_row() {
            render_for_theme(&row.widget_name());
        }
        guard.set(false);
    });

    CommitDiffDialogRefs {
        dialog,
        stack,
        file_list,
    }
}

fn build_diff_pane() -> (gtk::TextView, gtk::ScrolledWindow) {
    let view = gtk::TextView::builder()
        .editable(false)
        .monospace(true)
        .cursor_visible(false)
        .wrap_mode(gtk::WrapMode::None)
        .left_margin(4)
        .right_margin(4)
        .top_margin(4)
        .build();
    let scroll = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&view)
        .build();
    (view, scroll)
}

fn build_file_row(file: &DiffFile) -> gtk::ListBoxRow {
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row_box.set_margin_start(6);
    row_box.set_margin_end(6);
    row_box.set_margin_top(4);
    row_box.set_margin_bottom(4);

    let (icon_name, icon_class) = file_icon(file);
    row_box.append(
        &gtk::Image::builder()
            .icon_name(icon_name)
            .css_classes([icon_class])
            .pixel_size(14)
            .build(),
    );

    row_box.append(
        &gtk::Label::builder()
            .label(&file.path)
            .css_classes(["caption"])
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::Start)
            .build(),
    );

    row_box.append(
        &gtk::Label::builder()
            .label(format!("+{} −{}", file.stats.insertions, file.stats.deletions))
            .css_classes(["caption", "dim-label", "monospace"])
            .build(),
    );

    let row = gtk::ListBoxRow::builder().child(&row_box).build();
    // The selection handler reads the path back from here.
    row.set_widget_name(&file.path);
    row
}

/// Same rule the commit row uses, so a file looks the same in both places.
fn file_icon(file: &DiffFile) -> (&'static str, &'static str) {
    let added = file.stats.insertions > 0;
    let removed = file.stats.deletions > 0;
    match (added, removed) {
        (true, false) => ("list-add-symbolic", "success"),
        (false, true) => ("list-remove-symbolic", "error"),
        _ => ("document-edit-symbolic", "accent"),
    }
}

fn find_row_by_path(list: &gtk::ListBox, path: &str) -> Option<gtk::ListBoxRow> {
    let mut index = 0;
    while let Some(row) = list.row_at_index(index) {
        if row.widget_name() == path {
            return Some(row);
        }
        index += 1;
    }
    None
}

/// `view-dual-symbolic` is not in every Adwaita release — fall back rather than
/// render an empty button.
fn split_icon_name() -> &'static str {
    let has = gtk::gdk::Display::default()
        .map(|display| gtk::IconTheme::for_display(&display).has_icon("view-dual-symbolic"))
        .unwrap_or(false);
    if has {
        "view-dual-symbolic"
    } else {
        "view-paged-symbolic"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serial_test::serial;

    use crate::model::{DiffFile, DiffHunk, DiffLine, DiffLineKind, DiffStats};

    use crate::test_support;

    fn sample_files() -> Vec<DiffFile> {
        let hunk = DiffHunk {
            header: "@@ -9,0 +10 @@\n".into(),
            lines: vec![DiffLine {
                kind: DiffLineKind::Addition,
                content: "let x = 1;\n".into(),
                old_lineno: None,
                new_lineno: Some(10),
            }],
        };
        ["src/main.rs", "src/lib.rs"]
            .iter()
            .map(|p| DiffFile {
                path: (*p).to_string(),
                hunks: vec![hunk.clone()],
                stats: DiffStats {
                    insertions: 1,
                    deletions: 0,
                },
                is_binary: false,
            })
            .collect()
    }

    #[test]
    #[serial]
    fn dialog_lists_files_and_starts_split() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }

        let (rows, page, page_after_switch, title) = test_support::on_gtk_thread(|| {
            let files = sample_files();
            let refs = build_commit_diff_dialog("5a6607e", "fix: something", &files, "src/lib.rs");

            let mut rows = 0;
            let mut child = refs.file_list.first_child();
            while let Some(c) = child {
                rows += 1;
                child = c.next_sibling();
            }

            let page = refs.stack.visible_child_name().map(|s| s.to_string());
            refs.stack.set_visible_child_name("unified");
            let after = refs.stack.visible_child_name().map(|s| s.to_string());
            (rows, page, after, refs.dialog.title().to_string())
        });

        assert_eq!(rows, 2, "sidebar lists every file in the commit");
        assert_eq!(page.as_deref(), Some("split"));
        assert_eq!(page_after_switch.as_deref(), Some("unified"));
        assert_eq!(title, "5a6607e");
    }

    /// The dialog opens on the file the user clicked, not on the first one.
    #[test]
    #[serial]
    fn dialog_opens_on_the_requested_file() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }

        let selected = test_support::on_gtk_thread(|| {
            let files = sample_files();
            let refs = build_commit_diff_dialog("5a6607e", "fix: something", &files, "src/lib.rs");
            refs.file_list
                .selected_row()
                .map(|r| r.widget_name().to_string())
        });

        assert_eq!(selected.as_deref(), Some("src/lib.rs"));
    }
}
