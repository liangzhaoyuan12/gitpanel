use adw::prelude::*;
use std::rc::Rc;

use crate::generated::licenses::all_licenses;

/// Replace common placeholders in a license template with user-provided values.
fn fill_template(content: &str, name: &str, email: &str, year: &str) -> String {
    let mut result = content.to_string();

    // Year placeholders (case-insensitive set)
    let year_placeholders = ["<year>", "<YEAR>", "<Year>", "<yyyy>", "<Jahr>"];
    for ph in &year_placeholders {
        result = result.replace(ph, year);
    }

    // Name / owner / copyright holder placeholders
    let display_name = if email.is_empty() {
        name.to_string()
    } else {
        format!("{} <{}>", name, email)
    };

    // For copyright lines we use "name <email>" format
    let copyright_display = if email.is_empty() {
        name.to_string()
    } else {
        format!("{} <{}>", name, email)
    };

    // `<name of author>` and similar variants — use full "name <email>" form
    let name_placeholders = [
        "<name of author>",
        "<Name of author>",
        "<NAME OF AUTHOR>",
        "<AUTHOR>",
    ];
    for ph in &name_placeholders {
        result = result.replace(ph, &display_name);
    }

    // `<copyright holders>`, `<copyright holder>`, `<COPYRIGHT HOLDERS>` — use full form
    let holder_placeholders = [
        "<copyright holders>",
        "<copyright holder>",
        "<COPYRIGHT HOLDERS>",
        "<COPYRIGHT HOLDER>",
        "<HOLDERS>",
        "<Copyright Information>",
        "<Copyright Inhaber>",
    ];
    for ph in &holder_placeholders {
        result = result.replace(ph, &copyright_display);
    }

    // `<owner>`, `<OWNER>`, `<Asset Owner>`, `<Owner Organization Name>`
    let owner_placeholders = [
        "<owner>",
        "<OWNER>",
        "<Asset Owner>",
        "<Owner Organization Name>",
    ];
    for ph in &owner_placeholders {
        result = result.replace(ph, &name);
    }

    // `<ORGANIZATION>`, `<Name of Institution>`, `<Name of Development Group>`
    let org_placeholders = [
        "<ORGANIZATION>",
        "<Name of Institution>",
        "<Name of Development Group>",
    ];
    for ph in &org_placeholders {
        result = result.replace(ph, &name);
    }

    // `<PRODUCT>` — just leave as-is (project-specific, not user info)
    // `<program>` — leave as-is too

    result
}

/// Check if a license template has any user-fillable placeholders.
fn has_placeholders(content: &str) -> bool {
    content.contains("<year>")
        || content.contains("<YEAR>")
        || content.contains("<Year>")
        || content.contains("<yyyy>")
        || content.contains("<Jahr>")
        || content.contains("<name of author>")
        || content.contains("<Name of author>")
        || content.contains("<NAME OF AUTHOR>")
        || content.contains("<AUTHOR>")
        || content.contains("<copyright holders>")
        || content.contains("<copyright holder>")
        || content.contains("<COPYRIGHT HOLDERS>")
        || content.contains("<COPYRIGHT HOLDER>")
        || content.contains("<HOLDERS>")
        || content.contains("<owner>")
        || content.contains("<OWNER>")
        || content.contains("<Asset Owner>")
        || content.contains("<ORGANIZATION>")
        || content.contains("<Name of Institution>")
        || content.contains("<Name of Development Group>")
        || content.contains("<Copyright Information>")
        || content.contains("<Copyright Inhaber>")
        || content.contains("<Owner Organization Name>")
}

pub fn build_license_dialog<F>(on_apply: F) -> adw::Dialog
where
    F: Fn(String) + 'static,
{
    let dialog = adw::Dialog::builder()
        .title("Add License")
        .content_width(800)
        .content_height(560)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    toolbar_view.add_top_bar(&header);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_bottom(12);

    // Search entry
    let search_entry = gtk::SearchEntry::builder()
        .placeholder_text("Search licenses\u{2026}")
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(4)
        .build();
    content.append(&search_entry);

    // Main area: left list + right info/preview
    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.set_wide_handle(true);

    // Left: license list with search
    let licenses = all_licenses();
    let list_store = gio::ListStore::new::<gtk::StringObject>();
    let mut display_names: Vec<String> = Vec::new();
    for lic in &licenses {
        let s = gtk::StringObject::new(&lic.name);
        list_store.append(&s);
        display_names.push(lic.name.to_string());
    }

    let filter_model = gtk::FilterListModel::new(Some(list_store.clone()), None::<gtk::CustomFilter>);
    let selection_model = gtk::SingleSelection::new(Some(filter_model.clone()));

    let list_view = gtk::ListView::builder()
        .model(&selection_model)
        .build();

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(move |_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let label = gtk::Label::builder()
            .xalign(0.0)
            .margin_start(8)
            .margin_end(8)
            .margin_top(4)
            .margin_bottom(4)
            .build();
        item.set_child(Some(&label));
    });
    factory.connect_bind(move |_, item| {
        let item = match item.downcast_ref::<gtk::ListItem>() {
            Some(i) => i,
            None => return,
        };
        let obj = match item.item().and_downcast::<gtk::StringObject>() {
            Some(o) => o,
            None => return,
        };
        let label = match item.child().and_downcast::<gtk::Label>() {
            Some(l) => l,
            None => return,
        };
        label.set_label(&obj.string());
    });
    list_view.set_factory(Some(&factory));

    let scrolled_list = gtk::ScrolledWindow::builder()
        .child(&list_view)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .min_content_width(220)
        .build();
    paned.set_start_child(Some(&scrolled_list));

    // Right: info + preview
    let right_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    right_box.set_margin_start(8);
    right_box.set_margin_end(4);

    let info_group = adw::PreferencesGroup::builder()
        .title("Your Information")
        .build();

    let name_row = adw::EntryRow::builder()
        .title("Name / Organization")
        .build();
    info_group.add(&name_row);

    let email_row = adw::EntryRow::builder()
        .title("Email (optional)")
        .build();
    info_group.add(&email_row);

    let year_row = adw::EntryRow::builder()
        .title("Year")
        .text(&chrono::Local::now().format("%Y").to_string())
        .build();
    info_group.add(&year_row);

    right_box.append(&info_group);

    // Preview label
    let preview_label = gtk::Label::builder()
        .label("Select a license to preview")
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .xalign(0.0)
        .yalign(0.0)
        .css_classes(["dim-label", "caption"])
        .build();

    let preview_scrolled = gtk::ScrolledWindow::builder()
        .child(&preview_label)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .build();
    right_box.append(&preview_scrolled);

    paned.set_end_child(Some(&right_box));
    content.append(&paned);

    // Bottom bar with "Add License" button
    let bottom_bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bottom_bar.set_margin_start(12);
    bottom_bar.set_margin_end(12);
    bottom_bar.set_margin_top(8);

    let placeholder_hint = gtk::Label::builder()
        .label("")
        .css_classes(["dim-label", "caption"])
        .hexpand(true)
        .xalign(0.0)
        .build();
    bottom_bar.append(&placeholder_hint);

    let add_btn = gtk::Button::builder()
        .label("Add License")
        .css_classes(["suggested-action", "pill"])
        .sensitive(false)
        .build();
    bottom_bar.append(&add_btn);

    content.append(&bottom_bar);

    toolbar_view.set_content(Some(&content));
    dialog.set_child(Some(&toolbar_view));

    // State: currently selected license index
    let selected_index: std::cell::Cell<Option<usize>> = std::cell::Cell::new(None);
    let selected_index = Rc::new(selected_index);

    // License data for quick lookup
    let license_contents: Vec<String> = licenses.iter().map(|l| l.content.to_string()).collect();
    let license_contents = Rc::new(license_contents);

    // Filter logic
    let filter = gtk::CustomFilter::new(move |obj| {
        // Always show all — filter is controlled by the search entry callback
        let _ = obj;
        true
    });
    filter_model.set_filter(Some(&filter));

    // Search entry filters the list
    {
        let filter_model = filter_model.clone();
        let display_names = display_names.clone();
        let list_store = list_store.clone();
        search_entry.connect_search_changed(move |entry| {
            let query = entry.text().to_lowercase();
            let filter = gtk::CustomFilter::new(move |obj| {
                let Some(s) = obj.downcast_ref::<gtk::StringObject>() else {
                    return false;
                };
                s.string().to_lowercase().contains(&query)
            });
            filter_model.set_filter(Some(&filter));
        });
    }

    // Preview update on selection change
    {
        let preview_label = preview_label.clone();
        let name_row = name_row.clone();
        let email_row = email_row.clone();
        let year_row = year_row.clone();
        let license_contents = license_contents.clone();
        let selected_index = selected_index.clone();
        let add_btn = add_btn.clone();
        let display_names = display_names.clone();

        selection_model.connect_selection_changed(move |model, _, _| {
            if let Some(item) = model.selected_item() {
                let s = item.downcast::<gtk::StringObject>().unwrap();
                let name = s.string().to_string();

                // Find index in the original license list
                if let Some(idx) = display_names.iter().position(|n| n == &name) {
                    selected_index.set(Some(idx));

                    let content = &license_contents[idx];
                    let filled = fill_template(
                        content,
                        &name_row.text(),
                        &email_row.text(),
                        &year_row.text(),
                    );

                    let has_ph = has_placeholders(content);
                    if has_ph {
                        placeholder_hint.set_label("This license has placeholders that will be filled with your info above.");
                    } else {
                        placeholder_hint.set_label("");
                    }

                    // Truncate preview if too long — use floor_char_boundary to
                    // avoid slicing a multi-byte character in half.
                    let preview = if filled.len() > 3000 {
                        let end = filled.floor_char_boundary(3000);
                        format!("{}...\n\n-License truncated for preview-", &filled[..end])
                    } else {
                        filled
                    };
                    preview_label.set_label(&preview);
                    add_btn.set_sensitive(true);
                }
            } else {
                selected_index.set(None);
                preview_label.set_label("Select a license to preview");
                add_btn.set_sensitive(false);
            }
        });
    }

    // Update preview when user info changes
    {
        let license_contents = license_contents.clone();
        let selected_index = selected_index.clone();

        let make_updater = {
            let preview_label = preview_label.clone();
            let license_contents = license_contents.clone();
            let selected_index = selected_index.clone();
            move |name: &str, email: &str, year: &str| {
                if let Some(idx) = selected_index.get() {
                    let content = &license_contents[idx];
                    let filled = fill_template(content, name, email, year);
                    let preview = if filled.len() > 3000 {
                        let end = filled.floor_char_boundary(3000);
                        format!("{}...\n\n-License truncated for preview-", &filled[..end])
                    } else {
                        filled
                    };
                    preview_label.set_label(&preview);
                }
            }
        };

        // Each connect_changed captures the updater Rc and all three rows
        // (cloned), so it can read all fields when any one changes.
        {
            let update = make_updater.clone();
            let nr2 = name_row.clone();
            let er2 = email_row.clone();
            let yr2 = year_row.clone();
            let nr_for_signal = name_row.clone();
            nr_for_signal.connect_changed(move |_| {
                update(&nr2.text(), &er2.text(), &yr2.text());
            });
        }
        {
            let update = make_updater.clone();
            let nr2 = name_row.clone();
            let er2 = email_row.clone();
            let yr2 = year_row.clone();
            let er_for_signal = email_row.clone();
            er_for_signal.connect_changed(move |_| {
                update(&nr2.text(), &er2.text(), &yr2.text());
            });
        }
        {
            let update = make_updater;
            let nr2 = name_row.clone();
            let er2 = email_row.clone();
            let yr2 = year_row.clone();
            let yr_for_signal = year_row.clone();
            yr_for_signal.connect_changed(move |_| {
                update(&nr2.text(), &er2.text(), &yr2.text());
            });
        }
    }

    // Add License button click
    {
        let dialog = dialog.clone();
        let name_row = name_row.clone();
        let email_row = email_row.clone();
        let year_row = year_row.clone();
        let license_contents = license_contents.clone();
        let selected_index = selected_index.clone();
        let display_names = display_names.clone();

        add_btn.connect_clicked(move |_| {
            if let Some(idx) = selected_index.get() {
                let content = &license_contents[idx];
                let name = display_names[idx].clone();
                let filled = fill_template(
                    content,
                    &name_row.text(),
                    &email_row.text(),
                    &year_row.text(),
                );
                on_apply(filled);
                dialog.close();
            }
        });
    }

    dialog
}
