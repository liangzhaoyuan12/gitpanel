use adw::glib;
use adw::prelude::*;

use crate::i18n::{self, Key};
use crate::utils::logging;

/// Build a dialog that shows the current log file with a one-click copy button.
///
/// The text view is read-only and monospace; the log is scrolled to the most
/// recent entry on open. The "Copy" button in the header bar copies the whole
/// log to the clipboard and briefly relabels itself to confirm.
pub fn build_log_viewer_dialog() -> adw::Dialog {
    let dialog = adw::Dialog::builder()
        .title(i18n::t(Key::logs_title))
        .content_width(760)
        .content_height(560)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    toolbar_view.add_top_bar(&header);

    // One-click copy of the entire log.
    let copy_btn = gtk::Button::builder()
        .label(i18n::t(Key::logs_copy))
        .tooltip_text("Copy the log to the clipboard")
        .css_classes(["suggested-action"])
        .build();
    header.pack_end(&copy_btn);

    // Read the active log file; fall back to a readable message on failure so
    // the dialog is never empty/confusing.
    let log_path = logging::current_log_file();
    let text = std::fs::read_to_string(&log_path).unwrap_or_else(|e| {
        format!(
            "Could not read the log file at:\n{}\n\nError: {e}\n\nIt is created as soon as the app logs anything (e.g. on start-up).",
            log_path.display()
        )
    });

    let text_view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    text_view.buffer().set_text(&text);

    // Jump to the most recent entry once the widget is realised.
    text_view.connect_map(|tv| {
        let buffer = tv.buffer();
        let mut iter = buffer.end_iter();
        tv.scroll_to_iter(&mut iter, 0.0, false, 0.0, 1.0);
    });

    let copy_text = text.clone();
    copy_btn.connect_clicked(move |btn| {
        btn.clipboard().set_text(&copy_text);
        btn.set_label(i18n::t(Key::logs_copied));
        let btn = btn.clone();
        glib::timeout_add_seconds_local_once(1, move || {
            btn.set_label(i18n::t(Key::logs_copy));
        });
    });

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&text_view)
        .vexpand(true)
        .build();
    toolbar_view.set_content(Some(&scrolled));

    dialog.set_child(Some(&toolbar_view));
    dialog
}
