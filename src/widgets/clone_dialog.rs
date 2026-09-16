use adw::prelude::*;
use crate::i18n::{self, Key};

/// Build a clone dialog. `on_clone` is called with (url, destination_dir).
pub fn build_clone_dialog<F>(on_clone: F) -> adw::Dialog
where
    F: Fn(String, String) + 'static,
{
    let dialog = adw::Dialog::builder()
        .title(i18n::t(Key::clone_title))
        .content_width(520)
        .content_height(320)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    toolbar_view.add_top_bar(&header);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.set_margin_top(16);
    content.set_margin_bottom(16);

    let group = adw::PreferencesGroup::new();

    let url_row = adw::EntryRow::builder()
        .title(i18n::t(Key::clone_url))
        .build();
    group.add(&url_row);

    let home = dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let dest_row = adw::EntryRow::builder()
        .title(i18n::t(Key::clone_dest))
        .text(&home)
        .build();
    group.add(&dest_row);

    content.append(&group);

    let hint = gtk::Label::builder()
        .label(i18n::t(Key::clone_desc))
        .wrap(true)
        .xalign(0.0)
        .css_classes(["dim-label", "caption"])
        .build();
    content.append(&hint);

    let clone_btn = gtk::Button::builder()
        .label(i18n::t(Key::clone_btn))
        .css_classes(["suggested-action", "pill"])
        .halign(gtk::Align::End)
        .sensitive(false)
        .build();
    content.append(&clone_btn);

    toolbar_view.set_content(Some(&content));
    dialog.set_child(Some(&toolbar_view));

    // Enable clone button when both fields are non-empty
    let update_btn = {
        let url_row = url_row.clone();
        let dest_row = dest_row.clone();
        let clone_btn = clone_btn.clone();
        move || {
            let enabled = !url_row.text().is_empty() && !dest_row.text().is_empty();
            clone_btn.set_sensitive(enabled);
        }
    };
    {
        let update = update_btn.clone();
        url_row.connect_changed(move |_| update());
    }
    {
        let update = update_btn.clone();
        dest_row.connect_changed(move |_| update());
    }

    // Clone action
    let dialog_weak = dialog.downgrade();
    clone_btn.connect_clicked(move |_| {
        let url = url_row.text().trim().to_string();
        let dest = dest_row.text().trim().to_string();
        if url.is_empty() || dest.is_empty() {
            return;
        }
        on_clone(url, dest);
        if let Some(d) = dialog_weak.upgrade() {
            d.close();
        }
    });

    dialog
}
