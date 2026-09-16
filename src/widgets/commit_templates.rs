use adw::prelude::*;
use crate::i18n::{self, Key};

const CONVENTIONAL_PREFIXES: &[(&str, &str)] = &[
    ("feat: ", "A new feature"),
    ("fix: ", "A bug fix"),
    ("docs: ", "Documentation only"),
    ("style: ", "Formatting, no code change"),
    ("refactor: ", "Code restructuring"),
    ("perf: ", "Performance improvement"),
    ("test: ", "Adding tests"),
    ("build: ", "Build system changes"),
    ("ci: ", "CI configuration"),
    ("chore: ", "Maintenance tasks"),
    ("revert: ", "Revert a commit"),
];

/// Known conventional commit prefixes for detection/replacement.
const PREFIX_PATTERNS: &[&str] = &[
    "feat: ", "fix: ", "docs: ", "style: ", "refactor: ",
    "perf: ", "test: ", "build: ", "ci: ", "chore: ", "revert: ",
    "feat(", "fix(", "docs(", "style(", "refactor(",
    "perf(", "test(", "build(", "ci(", "chore(", "revert(",
];

/// Build a conventional commit prefix menu button.
/// `on_select` is called with the chosen prefix string.
pub fn build_template_button<F>(on_select: F) -> gtk::MenuButton
where
    F: Fn(&str) + Clone + 'static,
{
    let menu_btn = gtk::MenuButton::builder()
        .icon_name("document-new-symbolic")
        .css_classes(["flat", "circular"])
        .tooltip_text("Commit type")
        .valign(gtk::Align::Center)
        .build();

    let popover = gtk::Popover::new();
    let list_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["navigation-sidebar"])
        .build();

    for (prefix, description) in CONVENTIONAL_PREFIXES {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row_box.set_margin_start(8);
        row_box.set_margin_end(8);
        row_box.set_margin_top(4);
        row_box.set_margin_bottom(4);

        let prefix_label = gtk::Label::builder()
            .label(*prefix)
            .css_classes(["monospace"])
            .width_chars(12)
            .xalign(0.0)
            .build();
        row_box.append(&prefix_label);

        let desc_label = gtk::Label::builder()
            .label(*description)
            .css_classes(["caption", "dim-label"])
            .build();
        row_box.append(&desc_label);

        let row = gtk::ListBoxRow::builder()
            .child(&row_box)
            .build();
        row.set_widget_name(prefix);
        list_box.append(&row);
    }

    let on_select_clone = on_select.clone();
    let popover_ref = popover.clone();
    list_box.connect_row_activated(move |_, row| {
        let prefix = row.widget_name().to_string();
        on_select_clone(&prefix);
        popover_ref.popdown();
    });

    popover.set_child(Some(&list_box));
    menu_btn.set_popover(Some(&popover));

    menu_btn
}

/// Build a Co-Authored-By trailer popover button.
/// `on_add` is called with (name, email) when the user submits.
pub fn build_coauthor_button<F>(on_add: F) -> gtk::MenuButton
where
    F: Fn(&str, &str) + 'static,
{
    let menu_btn = gtk::MenuButton::builder()
        .icon_name("system-users-symbolic")
        .css_classes(["flat", "circular"])
        .tooltip_text(i18n::t(Key::template_add_coauthor))
        .valign(gtk::Align::Center)
        .build();

    let popover = gtk::Popover::new();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_width_request(280);

    let title = gtk::Label::builder()
        .label(i18n::t(Key::template_add_coauthor))
        .css_classes(["heading"])
        .xalign(0.0)
        .build();
    content.append(&title);

    let name_entry = gtk::Entry::builder()
        .placeholder_text("Full name")
        .build();
    content.append(&name_entry);

    let email_entry = gtk::Entry::builder()
        .placeholder_text("email@example.com")
        .build();
    content.append(&email_entry);

    let add_btn = gtk::Button::builder()
        .label(i18n::t(Key::template_add_trailer))
        .css_classes(["suggested-action"])
        .sensitive(false)
        .build();
    content.append(&add_btn);

    // Enable add button only when both fields filled
    let update = {
        let name_entry = name_entry.clone();
        let email_entry = email_entry.clone();
        let add_btn = add_btn.clone();
        move || {
            add_btn.set_sensitive(
                !name_entry.text().is_empty() && !email_entry.text().is_empty(),
            );
        }
    };
    {
        let u = update.clone();
        name_entry.connect_changed(move |_| u());
    }
    {
        let u = update.clone();
        email_entry.connect_changed(move |_| u());
    }

    let popover_ref = popover.clone();
    add_btn.connect_clicked(move |_| {
        let name = name_entry.text().trim().to_string();
        let email = email_entry.text().trim().to_string();
        if name.is_empty() || email.is_empty() {
            return;
        }
        on_add(&name, &email);
        name_entry.set_text("");
        email_entry.set_text("");
        popover_ref.popdown();
    });

    popover.set_child(Some(&content));
    menu_btn.set_popover(Some(&popover));
    menu_btn
}

/// Append a Co-Authored-By trailer to a commit message buffer.
pub fn append_coauthor(buffer: &gtk::TextBuffer, name: &str, email: &str) {
    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
    let trailer = format!("Co-Authored-By: {} <{}>", name, email);

    if text.contains(&trailer) {
        return; // already present
    }

    let mut end = buffer.end_iter();
    let trimmed_end = text.trim_end();
    let needs_blank_line = !trimmed_end.is_empty()
        && !trimmed_end.ends_with("\n\n")
        && !trimmed_end.lines().last().map(|l| l.starts_with("Co-Authored-By:")).unwrap_or(false);

    let to_insert = if trimmed_end.is_empty() {
        trailer
    } else if needs_blank_line {
        format!("\n\n{}", trailer)
    } else {
        format!("\n{}", trailer)
    };
    buffer.insert(&mut end, &to_insert);
}

/// Insert a conventional commit prefix into a TextBuffer.
/// If an existing prefix is detected, replace it.
pub fn insert_prefix(buffer: &gtk::TextBuffer, prefix: &str) {
    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
    let text_str = text.as_str();

    // Check if there's already a conventional prefix
    for pattern in PREFIX_PATTERNS {
        if text_str.starts_with(pattern) {
            // Find the end of existing prefix (after ": " or after "): ")
            if let Some(colon_pos) = text_str.find(": ") {
                let end = colon_pos + 2;
                let mut start_iter = buffer.start_iter();
                let mut end_iter = buffer.iter_at_offset(end as i32);
                buffer.delete(&mut start_iter, &mut end_iter);
                let mut start_iter = buffer.start_iter();
                buffer.insert(&mut start_iter, prefix);
                return;
            }
        }
    }

    // No existing prefix — insert at start
    let mut start_iter = buffer.start_iter();
    buffer.insert(&mut start_iter, prefix);
}
