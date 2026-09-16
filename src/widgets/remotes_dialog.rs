use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::i18n::{self, Key};
use crate::model::RemoteInfo;

pub enum RemoteAction {
    Add { name: String, url: String },
    Remove { name: String },
    Rename { old: String, new: String },
    SetUrl { name: String, url: String },
}

/// Build a remotes management dialog. `on_action` is called whenever the user
/// performs an action; it should mutate the repository and refresh the list.
pub fn build_remotes_dialog<F>(remotes: &[RemoteInfo], on_action: F) -> adw::Dialog
where
    F: Fn(RemoteAction) + 'static,
{
    let dialog = adw::Dialog::builder()
        .title(i18n::t(Key::remotes_title))
        .content_width(560)
        .content_height(480)
        .build();

    let on_action = Rc::new(on_action);

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    toolbar_view.add_top_bar(&header);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.set_margin_top(16);
    content.set_margin_bottom(16);

    // Existing remotes
    let existing_group = adw::PreferencesGroup::builder()
        .title(i18n::t(Key::remotes_existing))
        .build();

    if remotes.is_empty() {
        let empty = gtk::Label::builder()
            .label(i18n::t(Key::remotes_no_configured))
            .css_classes(["dim-label"])
            .xalign(0.0)
            .build();
        existing_group.add(&empty);
    } else {
        for remote in remotes {
            let row = adw::ActionRow::builder()
                .title(&remote.name)
                .subtitle(&remote.url)
                .build();

            let edit_btn = gtk::Button::builder()
                .icon_name("document-edit-symbolic")
                .tooltip_text(i18n::t(Key::remotes_edit_url))
                .css_classes(["flat"])
                .valign(gtk::Align::Center)
                .build();
            let rename_btn = gtk::Button::builder()
                .icon_name("input-keyboard-symbolic")
                .tooltip_text("Rename")
                .css_classes(["flat"])
                .valign(gtk::Align::Center)
                .build();
            let remove_btn = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .tooltip_text("Remove")
                .css_classes(["flat", "error"])
                .valign(gtk::Align::Center)
                .build();

            row.add_suffix(&edit_btn);
            row.add_suffix(&rename_btn);
            row.add_suffix(&remove_btn);

            // Edit URL — replace row with entry
            {
                let on_action = on_action.clone();
                let name = remote.name.clone();
                let current_url = remote.url.clone();
                let dialog_weak = dialog.downgrade();
                edit_btn.connect_clicked(move |_| {
                    let alert = adw::AlertDialog::new(
                        Some(&format!("Edit URL of '{}'", name)),
                        None,
                    );
                    let entry = gtk::Entry::builder()
                        .text(&current_url)
                        .activates_default(true)
                        .build();
                    alert.set_extra_child(Some(&entry));
                    alert.add_response("cancel", "Cancel");
                    alert.add_response("save", "Save");
                    alert.set_response_appearance("save", adw::ResponseAppearance::Suggested);
                    alert.set_default_response(Some("save"));
                    alert.set_close_response("cancel");

                    let on_action = on_action.clone();
                    let name = name.clone();
                    alert.connect_response(None, move |_, response| {
                        if response == "save" {
                            let url = entry.text().trim().to_string();
                            if !url.is_empty() {
                                on_action(RemoteAction::SetUrl {
                                    name: name.clone(),
                                    url,
                                });
                            }
                        }
                    });
                    if let Some(d) = dialog_weak.upgrade() {
                        alert.present(Some(&d));
                    }
                });
            }

            // Rename
            {
                let on_action = on_action.clone();
                let old_name = remote.name.clone();
                let dialog_weak = dialog.downgrade();
                rename_btn.connect_clicked(move |_| {
                    let alert = adw::AlertDialog::new(
                        Some(&format!("Rename '{}'", old_name)),
                        None,
                    );
                    let entry = gtk::Entry::builder()
                        .text(&old_name)
                        .activates_default(true)
                        .build();
                    alert.set_extra_child(Some(&entry));
                    alert.add_response("cancel", "Cancel");
                    alert.add_response("rename", "Rename");
                    alert.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
                    alert.set_default_response(Some("rename"));
                    alert.set_close_response("cancel");

                    let on_action = on_action.clone();
                    let old_name = old_name.clone();
                    alert.connect_response(None, move |_, response| {
                        if response == "rename" {
                            let new = entry.text().trim().to_string();
                            if !new.is_empty() && new != old_name {
                                on_action(RemoteAction::Rename {
                                    old: old_name.clone(),
                                    new,
                                });
                            }
                        }
                    });
                    if let Some(d) = dialog_weak.upgrade() {
                        alert.present(Some(&d));
                    }
                });
            }

            // Remove with confirmation
            {
                let on_action = on_action.clone();
                let name = remote.name.clone();
                let dialog_weak = dialog.downgrade();
                remove_btn.connect_clicked(move |_| {
                    let alert = adw::AlertDialog::new(
                        Some("Remove Remote?"),
                        Some(&format!("Remove '{}' from this repository?", name)),
                    );
                    alert.add_response("cancel", "Cancel");
                    alert.add_response("remove", "Remove");
                    alert.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
                    alert.set_default_response(Some("cancel"));
                    alert.set_close_response("cancel");

                    let on_action = on_action.clone();
                    let name = name.clone();
                    alert.connect_response(None, move |_, response| {
                        if response == "remove" {
                            on_action(RemoteAction::Remove {
                                name: name.clone(),
                            });
                        }
                    });
                    if let Some(d) = dialog_weak.upgrade() {
                        alert.present(Some(&d));
                    }
                });
            }

            existing_group.add(&row);
        }
    }
    content.append(&existing_group);

    // Add new remote
    let add_group = adw::PreferencesGroup::builder()
        .title(i18n::t(Key::remotes_add))
        .build();
    let name_row = adw::EntryRow::builder().title(i18n::t(Key::remotes_name)).build();
    let url_row = adw::EntryRow::builder().title(i18n::t(Key::remotes_url)).build();
    add_group.add(&name_row);
    add_group.add(&url_row);

    let add_btn = gtk::Button::builder()
        .label(i18n::t(Key::remotes_add))
        .css_classes(["suggested-action", "pill"])
        .halign(gtk::Align::End)
        .sensitive(false)
        .build();

    let inputs = Rc::new(RefCell::new((name_row.clone(), url_row.clone())));
    let update = {
        let inputs = inputs.clone();
        let add_btn = add_btn.clone();
        move || {
            let i = inputs.borrow();
            add_btn.set_sensitive(!i.0.text().is_empty() && !i.1.text().is_empty());
        }
    };
    {
        let u = update.clone();
        name_row.connect_changed(move |_| u());
    }
    {
        let u = update.clone();
        url_row.connect_changed(move |_| u());
    }
    {
        let on_action = on_action.clone();
        let inputs = inputs.clone();
        add_btn.connect_clicked(move |_| {
            let i = inputs.borrow();
            let name = i.0.text().trim().to_string();
            let url = i.1.text().trim().to_string();
            if name.is_empty() || url.is_empty() {
                return;
            }
            i.0.set_text("");
            i.1.set_text("");
            on_action(RemoteAction::Add { name, url });
        });
    }
    content.append(&add_group);
    content.append(&add_btn);

    toolbar_view.set_content(Some(&gtk::ScrolledWindow::builder().child(&content).build()));
    dialog.set_child(Some(&toolbar_view));
    dialog
}
