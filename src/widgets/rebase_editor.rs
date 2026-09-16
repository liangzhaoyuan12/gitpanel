use adw::prelude::*;

use crate::i18n::{self, Key};
use crate::model::{RebaseAction, RebaseEntry};

/// Build an interactive rebase editor dialog.
/// `entries` are the commits to rebase (oldest first).
/// `on_execute` is called with the modified entries and the onto target.
pub fn build_rebase_editor<F>(
    entries: &[RebaseEntry],
    onto: &str,
    on_execute: F,
) -> adw::Dialog
where
    F: Fn(Vec<RebaseEntry>, String) + 'static,
{
    let dialog = adw::Dialog::builder()
        .title(i18n::t(Key::rebase_title))
        .content_width(600)
        .content_height(500)
        .build();

    let toolbar_view = adw::ToolbarView::new();

    let header = adw::HeaderBar::new();
    let execute_btn = gtk::Button::builder()
        .label(i18n::t(Key::rebase_start))
        .css_classes(["suggested-action"])
        .build();
    header.pack_end(&execute_btn);
    toolbar_view.add_top_bar(&header);

    let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let info_label = gtk::Label::builder()
        .label(format!("Rebasing {} commits onto {}", entries.len(), onto))
        .css_classes(["caption", "dim-label"])
        .margin_start(12)
        .margin_top(8)
        .margin_bottom(4)
        .xalign(0.0)
        .build();
    content_box.append(&info_label);

    // Build the commit list with action dropdowns
    let list_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .margin_start(8)
        .margin_end(8)
        .margin_top(4)
        .build();

    let actions_model = gtk::StringList::new(&["Pick", "Squash", "Fixup", "Reword", "Edit", "Drop"]);

    // Store shared state for each entry's action
    let entry_actions: std::rc::Rc<std::cell::RefCell<Vec<RebaseAction>>> =
        std::rc::Rc::new(std::cell::RefCell::new(
            entries.iter().map(|e| e.action).collect(),
        ));
    let entry_data: Vec<RebaseEntry> = entries.to_vec();

    for (i, entry) in entries.iter().enumerate() {
        let row = adw::ComboRow::builder()
            .title(&entry.message)
            .subtitle(&entry.short_id)
            .model(&actions_model)
            .selected(action_to_index(entry.action))
            .build();

        let actions = entry_actions.clone();
        row.connect_selected_notify(move |row| {
            let action = index_to_action(row.selected());
            actions.borrow_mut()[i] = action;
        });

        list_box.append(&row);
    }

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&list_box)
        .vexpand(true)
        .build();
    content_box.append(&scrolled);
    toolbar_view.set_content(Some(&content_box));

    dialog.set_child(Some(&toolbar_view));

    // Execute handler
    {
        let entry_data = entry_data.clone();
        let entry_actions = entry_actions.clone();
        let onto = onto.to_string();
        let dialog_weak = dialog.downgrade();
        execute_btn.connect_clicked(move |_| {
            let actions = entry_actions.borrow();
            let modified: Vec<RebaseEntry> = entry_data
                .iter()
                .enumerate()
                .map(|(i, e)| RebaseEntry {
                    action: actions[i],
                    ..e.clone()
                })
                .collect();
            on_execute(modified, onto.clone());
            if let Some(d) = dialog_weak.upgrade() {
                d.close();
            }
        });
    }

    dialog
}

fn action_to_index(action: RebaseAction) -> u32 {
    match action {
        RebaseAction::Pick => 0,
        RebaseAction::Squash => 1,
        RebaseAction::Fixup => 2,
        RebaseAction::Reword => 3,
        RebaseAction::Edit => 4,
        RebaseAction::Drop => 5,
    }
}

fn index_to_action(index: u32) -> RebaseAction {
    match index {
        1 => RebaseAction::Squash,
        2 => RebaseAction::Fixup,
        3 => RebaseAction::Reword,
        4 => RebaseAction::Edit,
        5 => RebaseAction::Drop,
        _ => RebaseAction::Pick,
    }
}
