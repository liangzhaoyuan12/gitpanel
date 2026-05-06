use adw::prelude::*;
use adw::glib;

use gitpulsar_core::models::CommitInfo;

/// Build a dialog showing the commit history for a single file.
/// `on_restore` is called with a commit SHA when the user clicks "Restore from this commit".
pub fn build_file_history_dialog<F>(
    file_path: &str,
    commits: &[CommitInfo],
    on_restore: F,
) -> adw::Dialog
where
    F: Fn(String) + 'static,
{
    let dialog = adw::Dialog::builder()
        .title(&format!("History: {}", file_path))
        .content_width(720)
        .content_height(520)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    toolbar_view.add_top_bar(&header);

    if commits.is_empty() {
        let empty = adw::StatusPage::builder()
            .icon_name("document-open-recent-symbolic")
            .title("No history")
            .description("This file has no commit history in the current branch.")
            .build();
        toolbar_view.set_content(Some(&empty));
        dialog.set_child(Some(&toolbar_view));
        return dialog;
    }

    let list_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();

    let on_restore = std::rc::Rc::new(on_restore);

    for commit in commits {
        let row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(&commit.summary).as_str())
            .subtitle(&format!(
                "{} · {} · {}",
                commit.short_id,
                commit.author.name,
                commit.time.format("%Y-%m-%d %H:%M")
            ))
            .build();

        let restore_btn = gtk::Button::builder()
            .icon_name("edit-undo-symbolic")
            .tooltip_text("Restore file from this commit")
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .build();
        let sha = commit.id.clone();
        let on_restore = on_restore.clone();
        restore_btn.connect_clicked(move |_| {
            on_restore(sha.clone());
        });
        row.add_suffix(&restore_btn);

        list_box.append(&row);
    }

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&list_box)
        .vexpand(true)
        .build();
    toolbar_view.set_content(Some(&scrolled));

    dialog.set_child(Some(&toolbar_view));
    dialog
}
