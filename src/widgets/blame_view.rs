use adw::prelude::*;

use crate::model::BlameLine;

/// Build a blame viewer dialog for a file.
pub fn build_blame_dialog<F>(file_path: &str, lines: &[BlameLine], _on_commit_click: F) -> adw::Dialog
where
    F: Fn(String) + 'static,
{
    let dialog = adw::Dialog::builder()
        .title(format!("Blame: {}", file_path))
        .content_width(800)
        .content_height(600)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    toolbar_view.add_top_bar(&header);

    let tv = gtk::TextView::builder()
        .editable(false)
        .monospace(true)
        .left_margin(4)
        .right_margin(4)
        .top_margin(4)
        .bottom_margin(4)
        .cursor_visible(false)
        .wrap_mode(gtk::WrapMode::None)
        .vexpand(true)
        .build();

    render_blame(&tv.buffer(), lines);

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&tv)
        .vexpand(true)
        .build();
    toolbar_view.set_content(Some(&scrolled));

    dialog.set_child(Some(&toolbar_view));
    dialog
}

/// Render blame output into a text buffer.
fn render_blame(buffer: &gtk::TextBuffer, lines: &[BlameLine]) {
    let is_dark = adw::StyleManager::default().is_dark();
    setup_blame_tags(buffer, is_dark);
    buffer.set_text("");
    let mut iter = buffer.end_iter();

    let mut last_commit = String::new();
    let mut color_idx: usize = 0;

    for line in lines {
        if line.commit_id != last_commit && !line.commit_id.is_empty() {
            last_commit = line.commit_id.clone();
            color_idx += 1;
        }

        let bg_tag = if color_idx.is_multiple_of(2) { "blame-even" } else { "blame-odd" };
        let line_start = iter.offset();

        // Blame info: sha author date
        let date_str = format_timestamp(line.time);
        let info = format!(
            "{:<8} {:<16} {:<12} ",
            line.short_id,
            truncate(&line.author, 16),
            date_str,
        );

        let info_start = iter.offset();
        buffer.insert(&mut iter, &info);
        buffer.apply_tag_by_name("blame-info", &buffer.iter_at_offset(info_start), &iter);

        // Line number
        let lineno_start = iter.offset();
        let lineno_str = format!("{:>4} | ", line.line_no);
        buffer.insert(&mut iter, &lineno_str);
        buffer.apply_tag_by_name("lineno", &buffer.iter_at_offset(lineno_start), &iter);

        // Content
        buffer.insert(&mut iter, &line.content);
        buffer.insert(&mut iter, "\n");

        // Apply background to full line
        buffer.apply_tag_by_name(bg_tag, &buffer.iter_at_offset(line_start), &iter);
    }
}

fn setup_blame_tags(buffer: &gtk::TextBuffer, is_dark: bool) {
    let tag_table = buffer.tag_table();
    for name in &["blame-info", "lineno", "blame-even", "blame-odd"] {
        if let Some(tag) = tag_table.lookup(name) {
            tag_table.remove(&tag);
        }
    }

    let (info_fg, lineno_fg, even_bg, odd_bg) = if is_dark {
        ("#8b949e", "#6e7681", "#1c2128", "#22272e")
    } else {
        ("#8b949e", "#6e7781", "#f6f8fa", "#ffffff")
    };

    tag_table.add(&gtk::TextTag::builder().name("blame-info").foreground(info_fg).build());
    tag_table.add(&gtk::TextTag::builder().name("lineno").foreground(lineno_fg).build());
    tag_table.add(&gtk::TextTag::builder().name("blame-even").background(even_bg).build());
    tag_table.add(&gtk::TextTag::builder().name("blame-odd").background(odd_bg).build());
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}…", &s[..max - 1])
    } else {
        s.to_string()
    }
}

fn format_timestamp(seconds: i64) -> String {
    if seconds == 0 {
        return String::new();
    }
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}
