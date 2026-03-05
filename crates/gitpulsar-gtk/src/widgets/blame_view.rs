use adw::prelude::*;

use gitpulsar_core::models::BlameLine;

/// Set up blame-specific tags on a text buffer
fn setup_blame_tags(buffer: &gtk::TextBuffer) {
    let tag_table = buffer.tag_table();

    if tag_table.lookup("blame-info").is_none() {
        tag_table.add(
            &gtk::TextTag::builder()
                .name("blame-info")
                .foreground("#8b949e")
                .build(),
        );
    }
    if tag_table.lookup("lineno").is_none() {
        tag_table.add(
            &gtk::TextTag::builder()
                .name("lineno")
                .foreground("#6e7781")
                .build(),
        );
    }
    // Alternating background colors for different commits
    if tag_table.lookup("blame-even").is_none() {
        tag_table.add(
            &gtk::TextTag::builder()
                .name("blame-even")
                .background("#f6f8fa")
                .build(),
        );
    }
    if tag_table.lookup("blame-odd").is_none() {
        tag_table.add(
            &gtk::TextTag::builder()
                .name("blame-odd")
                .background("#ffffff")
                .build(),
        );
    }
}

/// Render blame output into a text buffer.
/// Format: `short_id  author  date  line_no | content`
/// Alternating backgrounds for different commits.
pub fn render_blame(buffer: &gtk::TextBuffer, lines: &[BlameLine]) {
    setup_blame_tags(buffer);
    buffer.set_text("");
    let mut iter = buffer.end_iter();

    // Track commit groups for alternating colors
    let mut last_commit = String::new();
    let mut color_idx: usize = 0;

    for line in lines {
        // Switch color group when commit changes
        if line.commit_id != last_commit && !line.commit_id.is_empty() {
            last_commit = line.commit_id.clone();
            color_idx += 1;
        }

        let bg_tag = if color_idx % 2 == 0 {
            "blame-even"
        } else {
            "blame-odd"
        };

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
        buffer.apply_tag_by_name(
            "blame-info",
            &buffer.iter_at_offset(info_start),
            &iter,
        );

        // Line number
        let lineno_start = iter.offset();
        let lineno_str = format!("{:>4} │ ", line.line_no);
        buffer.insert(&mut iter, &lineno_str);
        buffer.apply_tag_by_name(
            "lineno",
            &buffer.iter_at_offset(lineno_start),
            &iter,
        );

        // Content
        buffer.insert(&mut iter, &line.content);
        buffer.insert(&mut iter, "\n");

        // Apply background to full line
        buffer.apply_tag_by_name(
            bg_tag,
            &buffer.iter_at_offset(line_start),
            &iter,
        );
    }
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
    let dt = chrono::DateTime::from_timestamp(seconds, 0);
    match dt {
        Some(d) => d.format("%Y-%m-%d").to_string(),
        None => String::new(),
    }
}
