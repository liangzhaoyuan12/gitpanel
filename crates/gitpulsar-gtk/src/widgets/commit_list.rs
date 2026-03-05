use adw::prelude::*;

use gitpulsar_core::models::CommitInfo;

pub fn create_commit_row(commit: &CommitInfo, tags: &[String]) -> gtk::ListBoxRow {
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row_box.set_margin_start(8);
    row_box.set_margin_end(8);
    row_box.set_margin_top(6);
    row_box.set_margin_bottom(6);

    // Hash (monospace, dim)
    let hash_label = gtk::Label::builder()
        .label(&commit.short_id)
        .css_classes(["caption", "monospace", "dim-label"])
        .valign(gtk::Align::Start)
        .build();
    row_box.append(&hash_label);

    // Message + tags + author + time
    let info_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    info_box.set_hexpand(true);

    // First line: message + tag badges
    let msg_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let message_label = gtk::Label::builder()
        .label(&commit.summary)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(30)
        .build();
    msg_row.append(&message_label);

    for tag_name in tags {
        let tag_label = gtk::Label::builder()
            .label(tag_name)
            .css_classes(["caption"])
            .valign(gtk::Align::Center)
            .build();
        let tag_frame = gtk::Frame::new(None);
        tag_frame.set_child(Some(&tag_label));
        tag_frame.add_css_class("accent");
        tag_frame.set_margin_start(2);
        msg_row.append(&tag_frame);
    }

    info_box.append(&msg_row);

    let meta = format!("{} {}", commit.author.name, format_relative_time(&commit.time));
    let meta_label = gtk::Label::builder()
        .label(&meta)
        .xalign(0.0)
        .css_classes(["caption", "dim-label"])
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    info_box.append(&meta_label);

    row_box.append(&info_box);

    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&row_box));
    row
}

fn format_relative_time(time: &chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(*time);

    if duration.num_minutes() < 1 {
        "just now".to_string()
    } else if duration.num_hours() < 1 {
        format!("{} min ago", duration.num_minutes())
    } else if duration.num_days() < 1 {
        format!("{}h ago", duration.num_hours())
    } else if duration.num_weeks() < 1 {
        format!("{}d ago", duration.num_days())
    } else if duration.num_weeks() < 5 {
        format!("{}w ago", duration.num_weeks())
    } else {
        time.format("%Y-%m-%d").to_string()
    }
}
