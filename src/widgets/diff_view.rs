use adw::prelude::*;

use crate::model::{DiffFile, DiffLineKind};

/// Colours for diff rendering, chosen per theme.
pub struct DiffPalette {
    pub addition_bg: &'static str,
    pub addition_fg: &'static str,
    pub deletion_bg: &'static str,
    pub deletion_fg: &'static str,
    pub hunk_bg: &'static str,
    pub hunk_fg: &'static str,
    pub file_header_fg: &'static str,
    pub lineno_fg: &'static str,
    pub empty_bg: &'static str,
}

/// The light palette is what this renderer has always used; the dark one keeps
/// the same hues at the luminance the dark theme expects.
pub fn diff_palette(dark: bool) -> DiffPalette {
    if dark {
        DiffPalette {
            addition_bg: "#1e3a24",
            addition_fg: "#7ee787",
            deletion_bg: "#3d1d20",
            deletion_fg: "#ff7b72",
            hunk_bg: "#12283f",
            hunk_fg: "#79c0ff",
            file_header_fg: "#8b949e",
            lineno_fg: "#6e7681",
            empty_bg: "#161b22",
        }
    } else {
        DiffPalette {
            addition_bg: "#d4edda",
            addition_fg: "#1a7f37",
            deletion_bg: "#f8d7da",
            deletion_fg: "#cf222e",
            hunk_bg: "#ddf4ff",
            hunk_fg: "#0969da",
            file_header_fg: "#656d76",
            lineno_fg: "#8b949e",
            empty_bg: "#f6f8fa",
        }
    }
}

/// Set up diff tags on a text buffer, using the current theme's palette.
fn setup_tags(buffer: &gtk::TextBuffer) {
    let dark = adw::StyleManager::default().is_dark();
    setup_tags_with(buffer, &diff_palette(dark));
}

/// Create the diff tags, or restyle them when the buffer already has them —
/// re-rendering after a theme change must not keep the previous colours.
pub fn setup_tags_with(buffer: &gtk::TextBuffer, palette: &DiffPalette) {
    let table = buffer.tag_table();

    let apply = |name: &str, bg: Option<&str>, fg: Option<&str>, weight: Option<i32>| {
        let tag = table.lookup(name).unwrap_or_else(|| {
            let t = gtk::TextTag::builder().name(name).build();
            table.add(&t);
            t
        });
        tag.set_background(bg);
        tag.set_foreground(fg);
        if let Some(w) = weight {
            tag.set_weight(w);
        }
    };

    apply("addition", Some(palette.addition_bg), Some(palette.addition_fg), None);
    apply("deletion", Some(palette.deletion_bg), Some(palette.deletion_fg), None);
    apply("hunk-header", Some(palette.hunk_bg), Some(palette.hunk_fg), None);
    apply("file-header", None, Some(palette.file_header_fg), Some(700));
    apply("lineno", None, Some(palette.lineno_fg), None);
    apply("empty-line", Some(palette.empty_bg), None, None);
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

        // Binary file — show message instead of empty diff
        if file.is_binary {
            use crate::i18n::{self, Key};
            let s = iter.offset();
            buffer.insert(&mut iter, i18n::t(Key::binary_diff_not_supported));
            buffer.insert(&mut iter, "\n");
            buffer.apply_tag_by_name("file-header", &buffer.iter_at_offset(s), &iter);
            buffer.insert(&mut iter, "\n");
            continue;
        }

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

        // Binary file — show message on both sides
        if file.is_binary {
            use crate::i18n::{self, Key};
            let msg = i18n::t(Key::binary_diff_not_supported);
            let ls = left_iter.offset();
            left_buffer.insert(&mut left_iter, &msg);
            left_buffer.insert(&mut left_iter, "\n");
            left_buffer.apply_tag_by_name("file-header", &left_buffer.iter_at_offset(ls), &left_iter);

            let rs = right_iter.offset();
            right_buffer.insert(&mut right_iter, &msg);
            right_buffer.insert(&mut right_iter, "\n");
            right_buffer.apply_tag_by_name("file-header", &right_buffer.iter_at_offset(rs), &right_iter);

            left_buffer.insert(&mut left_iter, "\n");
            right_buffer.insert(&mut right_iter, "\n");
            continue;
        }

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
    // Both axes: code lines are not wrapped (wrapping would misalign the two
    // sides), so long lines scroll horizontally — and the sides have to travel
    // together or the comparison stops lining up.
    link_adjustments(&sw1.vadjustment(), &sw2.vadjustment());
    link_adjustments(&sw1.hadjustment(), &sw2.hadjustment());
}

/// Keep two adjustments at the same value, without the two handlers chasing
/// each other: the guard drops updates smaller than a pixel.
fn link_adjustments(a: &gtk::Adjustment, b: &gtk::Adjustment) {
    let other = b.clone();
    a.connect_value_changed(move |adj| {
        if (other.value() - adj.value()).abs() > 1.0 {
            other.set_value(adj.value());
        }
    });

    let other = a.clone();
    b.connect_value_changed(move |adj| {
        if (other.value() - adj.value()).abs() > 1.0 {
            other.set_value(adj.value());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    use serial_test::serial;

    use crate::test_support;

    /// The two panes must travel together on both axes.
    #[test]
    #[serial]
    fn sync_scroll_links_both_axes() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }

        let (v, h) = test_support::on_gtk_thread(|| {
            let left = gtk::ScrolledWindow::new();
            let right = gtk::ScrolledWindow::new();
            for sw in [&left, &right] {
                for adj in [sw.vadjustment(), sw.hadjustment()] {
                    adj.set_upper(1000.0);
                    adj.set_page_size(100.0);
                }
            }
            sync_scroll(&left, &right);

            left.vadjustment().set_value(250.0);
            left.hadjustment().set_value(80.0);
            (right.vadjustment().value(), right.hadjustment().value())
        });

        assert_eq!(v, 250.0, "vertical scroll follows");
        assert_eq!(h, 80.0, "horizontal scroll follows");
    }

    #[test]
    fn palettes_differ_by_theme() {
        let light = diff_palette(false);
        let dark = diff_palette(true);
        assert_ne!(light.addition_bg, dark.addition_bg);
        assert_ne!(light.deletion_bg, dark.deletion_bg);
        assert_ne!(light.hunk_bg, dark.hunk_bg);
        // The light palette is the one this renderer shipped with.
        assert_eq!(light.addition_bg, "#d4edda");
    }
}
