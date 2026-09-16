use adw::glib;
use adw::prelude::*;

use crate::i18n::{self, Key};
use crate::model::ReflogEntry;

/// Build a dialog showing the HEAD reflog.
pub fn build_reflog_dialog(entries: &[ReflogEntry]) -> adw::Dialog {
    let dialog = adw::Dialog::builder()
        .title(i18n::t(Key::reflog_title))
        .content_width(720)
        .content_height(520)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    toolbar_view.add_top_bar(&header);

    if entries.is_empty() {
        let empty = adw::StatusPage::builder()
            .icon_name("emblem-system-symbolic")
            .title(i18n::t(Key::reflog_no_entries))
            .description("This repository has no reflog history yet.")
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

    for entry in entries {
        let date = chrono::DateTime::from_timestamp(entry.time, 0)
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default();
        let row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(&entry.message).as_str())
            .subtitle(format!("{} · {} · {}", entry.short_new, entry.committer.name, date))
            .build();
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
