use std::sync::OnceLock;

use syntect::highlighting::ThemeSet;
use syntect::parsing::{SyntaxSet, SyntaxReference};
use syntect::easy::HighlightLines;

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// Detect syntax for a file path based on extension.
pub fn detect_syntax(path: &str) -> Option<&'static SyntaxReference> {
    let ss = syntax_set();
    let ext = std::path::Path::new(path).extension()?.to_str()?;
    ss.find_syntax_by_extension(ext)
}

/// Highlighted segment: byte range + RGBA foreground color.
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
    pub fg: (u8, u8, u8),
}

/// Highlight all lines for a file. Returns per-line spans.
/// `lines` should contain the raw content (without +/- prefix).
pub fn highlight_lines(syntax: &SyntaxReference, lines: &[&str], is_dark: bool) -> Vec<Vec<HighlightSpan>> {
    let ss = syntax_set();
    let ts = theme_set();
    let theme_name = if is_dark { "base16-ocean.dark" } else { "base16-ocean.light" };
    let theme = ts.themes.get(theme_name).unwrap_or_else(|| &ts.themes["base16-ocean.dark"]);

    let mut h = HighlightLines::new(syntax, theme);
    let mut result = Vec::with_capacity(lines.len());

    for line in lines {
        let spans = h.highlight_line(line, ss);
        match spans {
            Ok(ranges) => {
                let mut line_spans = Vec::new();
                let mut offset = 0;
                for (style, text) in &ranges {
                    let len = text.len();
                    if len > 0 {
                        line_spans.push(HighlightSpan {
                            start: offset,
                            end: offset + len,
                            fg: (style.foreground.r, style.foreground.g, style.foreground.b),
                        });
                    }
                    offset += len;
                }
                result.push(line_spans);
            }
            Err(_) => result.push(Vec::new()),
        }
    }

    result
}
