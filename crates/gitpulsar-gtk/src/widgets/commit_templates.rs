use adw::prelude::*;

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
