use adw::prelude::*;

use gitpulsar_core::models::{DiffFile, DiffLineKind};

/// Set up diff tags on a text buffer
fn setup_tags(buffer: &gtk::TextBuffer) {
    let tag_table = buffer.tag_table();

    if tag_table.lookup("addition").is_none() {
        tag_table.add(&gtk::TextTag::builder()
            .name("addition").background("#d4edda").foreground("#1a7f37").build());
    }
    if tag_table.lookup("deletion").is_none() {
        tag_table.add(&gtk::TextTag::builder()
            .name("deletion").background("#f8d7da").foreground("#cf222e").build());
    }
    if tag_table.lookup("hunk-header").is_none() {
        tag_table.add(&gtk::TextTag::builder()
            .name("hunk-header").background("#ddf4ff").foreground("#0969da").build());
    }
    if tag_table.lookup("file-header").is_none() {
        tag_table.add(&gtk::TextTag::builder()
            .name("file-header").weight(700).foreground("#656d76").build());
    }
    if tag_table.lookup("lineno").is_none() {
        tag_table.add(&gtk::TextTag::builder()
            .name("lineno").foreground("#8b949e").build());
    }
    if tag_table.lookup("empty-line").is_none() {
        tag_table.add(&gtk::TextTag::builder()
            .name("empty-line").background("#f6f8fa").build());
    }
}

/// Render unified diff into a single buffer
pub fn render_unified(buffer: &gtk::TextBuffer, files: &[DiffFile]) {
    setup_tags(buffer);
    buffer.set_text("");
    let mut iter = buffer.end_iter();

    for file in files {
        // File header
        let text = format!("── {} (+{} -{}) ──\n", file.path, file.stats.insertions, file.stats.deletions);
        let s = iter.offset();
        buffer.insert(&mut iter, &text);
        buffer.apply_tag_by_name("file-header", &buffer.iter_at_offset(s), &iter);

        for hunk in &file.hunks {
            let s = iter.offset();
            buffer.insert(&mut iter, &hunk.header);
            if !hunk.header.ends_with('\n') { buffer.insert(&mut iter, "\n"); }
            buffer.apply_tag_by_name("hunk-header", &buffer.iter_at_offset(s), &iter);

            for line in &hunk.lines {
                let (prefix, tag) = match line.kind {
                    DiffLineKind::Addition => ("+ ", Some("addition")),
                    DiffLineKind::Deletion => ("- ", Some("deletion")),
                    DiffLineKind::Context => ("  ", None),
                };

                let lineno_str = match line.kind {
                    DiffLineKind::Deletion => format!("{:>4}      ", line.old_lineno.unwrap_or(0)),
                    DiffLineKind::Addition => format!("     {:>4} ", line.new_lineno.unwrap_or(0)),
                    DiffLineKind::Context => format!("{:>4} {:>4} ",
                        line.old_lineno.unwrap_or(0), line.new_lineno.unwrap_or(0)),
                };

                let s = iter.offset();
                buffer.insert(&mut iter, &lineno_str);
                let mid = iter.offset();
                buffer.insert(&mut iter, prefix);
                buffer.insert(&mut iter, &line.content);
                if !line.content.ends_with('\n') { buffer.insert(&mut iter, "\n"); }

                // Line number color
                buffer.apply_tag_by_name("lineno", &buffer.iter_at_offset(s), &buffer.iter_at_offset(mid));

                if let Some(tag) = tag {
                    buffer.apply_tag_by_name(tag, &buffer.iter_at_offset(s), &iter);
                }
            }
        }
        buffer.insert(&mut iter, "\n");
    }
}

/// Render side-by-side diff into two buffers (left = old, right = new).
/// Blank lines are inserted to keep both sides aligned.
pub fn render_side_by_side(
    left_buffer: &gtk::TextBuffer,
    right_buffer: &gtk::TextBuffer,
    files: &[DiffFile],
) {
    setup_tags(left_buffer);
    setup_tags(right_buffer);
    left_buffer.set_text("");
    right_buffer.set_text("");

    let mut left_iter = left_buffer.end_iter();
    let mut right_iter = right_buffer.end_iter();

    for file in files {
        // File header on both sides
        let header = format!("── {} (+{} -{}) ──\n", file.path, file.stats.insertions, file.stats.deletions);

        let ls = left_iter.offset();
        left_buffer.insert(&mut left_iter, &header);
        left_buffer.apply_tag_by_name("file-header", &left_buffer.iter_at_offset(ls), &left_iter);

        let rs = right_iter.offset();
        right_buffer.insert(&mut right_iter, &header);
        right_buffer.apply_tag_by_name("file-header", &right_buffer.iter_at_offset(rs), &right_iter);

        for hunk in &file.hunks {
            // Hunk header
            let ls = left_iter.offset();
            left_buffer.insert(&mut left_iter, &hunk.header);
            if !hunk.header.ends_with('\n') { left_buffer.insert(&mut left_iter, "\n"); }
            left_buffer.apply_tag_by_name("hunk-header", &left_buffer.iter_at_offset(ls), &left_iter);

            let rs = right_iter.offset();
            right_buffer.insert(&mut right_iter, &hunk.header);
            if !hunk.header.ends_with('\n') { right_buffer.insert(&mut right_iter, "\n"); }
            right_buffer.apply_tag_by_name("hunk-header", &right_buffer.iter_at_offset(rs), &right_iter);

            for line in &hunk.lines {
                match line.kind {
                    DiffLineKind::Context => {
                        let left_text = format!("{:>4}  {}", line.old_lineno.unwrap_or(0), line.content);
                        let right_text = format!("{:>4}  {}", line.new_lineno.unwrap_or(0), line.content);

                        left_buffer.insert(&mut left_iter, &left_text);
                        if !left_text.ends_with('\n') { left_buffer.insert(&mut left_iter, "\n"); }

                        right_buffer.insert(&mut right_iter, &right_text);
                        if !right_text.ends_with('\n') { right_buffer.insert(&mut right_iter, "\n"); }
                    }
                    DiffLineKind::Deletion => {
                        let text = format!("{:>4}  {}", line.old_lineno.unwrap_or(0), line.content);
                        let ls = left_iter.offset();
                        left_buffer.insert(&mut left_iter, &text);
                        if !text.ends_with('\n') { left_buffer.insert(&mut left_iter, "\n"); }
                        left_buffer.apply_tag_by_name("deletion", &left_buffer.iter_at_offset(ls), &left_iter);

                        // Empty line on right to keep alignment
                        let rs = right_iter.offset();
                        right_buffer.insert(&mut right_iter, "\n");
                        right_buffer.apply_tag_by_name("empty-line", &right_buffer.iter_at_offset(rs), &right_iter);
                    }
                    DiffLineKind::Addition => {
                        // Empty line on left
                        let ls = left_iter.offset();
                        left_buffer.insert(&mut left_iter, "\n");
                        left_buffer.apply_tag_by_name("empty-line", &left_buffer.iter_at_offset(ls), &left_iter);

                        let text = format!("{:>4}  {}", line.new_lineno.unwrap_or(0), line.content);
                        let rs = right_iter.offset();
                        right_buffer.insert(&mut right_iter, &text);
                        if !text.ends_with('\n') { right_buffer.insert(&mut right_iter, "\n"); }
                        right_buffer.apply_tag_by_name("addition", &right_buffer.iter_at_offset(rs), &right_iter);
                    }
                }
            }
        }
        left_buffer.insert(&mut left_iter, "\n");
        right_buffer.insert(&mut right_iter, "\n");
    }
}

/// Synchronize vertical scrolling of two ScrolledWindows
pub fn sync_scroll(sw1: &gtk::ScrolledWindow, sw2: &gtk::ScrolledWindow) {
    let adj1 = sw1.vadjustment();
    let adj2 = sw2.vadjustment();

    let a2 = adj2.clone();
    adj1.connect_value_changed(move |adj| {
        if (a2.value() - adj.value()).abs() > 1.0 {
            a2.set_value(adj.value());
        }
    });

    let a1 = adj1.clone();
    adj2.connect_value_changed(move |adj| {
        if (a1.value() - adj.value()).abs() > 1.0 {
            a1.set_value(adj.value());
        }
    });
}
