use adw::prelude::*;

use gitpulsar_core::models::{BranchInfo, TagInfo};

pub struct BranchesTagsRefs {
    pub local_list: gtk::ListBox,
    pub remote_list: gtk::ListBox,
    pub tags_list: gtk::ListBox,
    pub create_branch_btn: gtk::Button,
    pub search_entry: gtk::SearchEntry,
}

pub fn build_branches_tags_panel() -> (gtk::Box, BranchesTagsRefs) {
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.set_width_request(200);

    // Search
    let search_entry = gtk::SearchEntry::builder()
        .placeholder_text("Filter…")
        .margin_start(8)
        .margin_end(8)
        .margin_top(8)
        .margin_bottom(4)
        .build();
    panel.append(&search_entry);

    let scrolled = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();

    let inner = gtk::Box::new(gtk::Orientation::Vertical, 0);

    // === Local Branches ===
    let local_header = gtk::Label::builder()
        .label("Local")
        .css_classes(["heading"])
        .xalign(0.0)
        .margin_start(12)
        .margin_top(8)
        .margin_bottom(4)
        .build();
    inner.append(&local_header);

    let local_list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["navigation-sidebar"])
        .build();
    inner.append(&local_list);

    // === Remote Branches ===
    let remote_header = gtk::Label::builder()
        .label("Remote")
        .css_classes(["heading"])
        .xalign(0.0)
        .margin_start(12)
        .margin_top(8)
        .margin_bottom(4)
        .build();
    inner.append(&remote_header);

    let remote_list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["navigation-sidebar"])
        .build();
    inner.append(&remote_list);

    // === Tags ===
    let tags_header = gtk::Label::builder()
        .label("Tags")
        .css_classes(["heading"])
        .xalign(0.0)
        .margin_start(12)
        .margin_top(8)
        .margin_bottom(4)
        .build();
    inner.append(&tags_header);

    let tags_list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["navigation-sidebar"])
        .build();
    inner.append(&tags_list);

    scrolled.set_child(Some(&inner));
    panel.append(&scrolled);

    // Create branch button
    let create_branch_btn = gtk::Button::builder()
        .label("Create Branch…")
        .css_classes(["suggested-action"])
        .margin_start(8)
        .margin_end(8)
        .margin_top(4)
        .margin_bottom(8)
        .build();
    panel.append(&create_branch_btn);

    // Search filter
    let ll = local_list.clone();
    let rl = remote_list.clone();
    let tl = tags_list.clone();
    search_entry.connect_search_changed(move |entry| {
        let query = entry.text().to_lowercase();
        let q1 = query.clone();
        let q2 = query.clone();
        let q3 = query;
        ll.set_filter_func(move |row| {
            q1.is_empty() || row.widget_name().to_lowercase().contains(&q1)
        });
        rl.set_filter_func(move |row| {
            q2.is_empty() || row.widget_name().to_lowercase().contains(&q2)
        });
        tl.set_filter_func(move |row| {
            q3.is_empty() || row.widget_name().to_lowercase().contains(&q3)
        });
    });

    let refs = BranchesTagsRefs {
        local_list,
        remote_list,
        tags_list,
        create_branch_btn,
        search_entry,
    };

    (panel, refs)
}

/// Populate the branches panel with branch data.
pub fn populate_branches(local_list: &gtk::ListBox, remote_list: &gtk::ListBox, branches: &[BranchInfo]) {
    // Clear
    while let Some(child) = local_list.first_child() {
        local_list.remove(&child);
    }
    while let Some(child) = remote_list.first_child() {
        remote_list.remove(&child);
    }

    for branch in branches {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row_box.set_margin_start(8);
        row_box.set_margin_end(8);
        row_box.set_margin_top(4);
        row_box.set_margin_bottom(4);

        if branch.is_head {
            row_box.append(&gtk::Image::builder()
                .icon_name("object-select-symbolic")
                .css_classes(["success"])
                .build());
        }

        let name_label = gtk::Label::builder()
            .label(&branch.name)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        row_box.append(&name_label);

        if !branch.is_remote && (branch.ahead > 0 || branch.behind > 0) {
            let indicator = gtk::Label::builder()
                .label(&format!("▲{} ▼{}", branch.ahead, branch.behind))
                .css_classes(["caption", "dim-label"])
                .build();
            row_box.append(&indicator);
        }

        let row = gtk::ListBoxRow::builder()
            .child(&row_box)
            .activatable(true)
            .build();
        row.set_widget_name(&branch.name);

        if branch.is_remote {
            remote_list.append(&row);
        } else {
            local_list.append(&row);
        }
    }
}

/// Populate the tags list.
pub fn populate_tags(tags_list: &gtk::ListBox, tags: &[TagInfo]) {
    while let Some(child) = tags_list.first_child() {
        tags_list.remove(&child);
    }

    for tag in tags {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row_box.set_margin_start(8);
        row_box.set_margin_end(8);
        row_box.set_margin_top(4);
        row_box.set_margin_bottom(4);

        row_box.append(&gtk::Image::builder()
            .icon_name("tag-symbolic")
            .css_classes(["dim-label"])
            .build());

        let name_label = gtk::Label::builder()
            .label(&tag.name)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        row_box.append(&name_label);

        let row = gtk::ListBoxRow::builder()
            .child(&row_box)
            .activatable(false)
            .build();
        row.set_widget_name(&tag.name);

        tags_list.append(&row);
    }
}
