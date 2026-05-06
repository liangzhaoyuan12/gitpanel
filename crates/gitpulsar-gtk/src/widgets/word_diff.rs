//! Cheap intra-line word-level diff used to highlight which parts of a
//! deletion/addition pair actually changed. Operates on byte offsets within
//! each line and does not allocate beyond the LCS table itself.

#[derive(Debug, Clone, Copy)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Default, Clone)]
pub struct WordDiff {
    /// Byte ranges within `old` that were removed.
    pub deleted: Vec<ByteRange>,
    /// Byte ranges within `new` that were inserted.
    pub inserted: Vec<ByteRange>,
}

/// Tokenise `s` into runs that are either word characters, single non-word
/// characters, or whitespace. Returns each token's byte offset and length.
fn tokenize(s: &str) -> Vec<(usize, &str)> {
    let mut tokens = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let b = bytes[i];
        // Whitespace run
        if b.is_ascii_whitespace() {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
        // Word run (letters, digits, underscore — ASCII only for speed; non-ASCII chars become single tokens)
        } else if b.is_ascii_alphanumeric() || b == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
        // Single non-ASCII byte sequence (UTF-8) treated as one token
        } else if b >= 0x80 {
            let char_len = utf8_char_len(b);
            i += char_len.min(bytes.len() - i);
        } else {
            i += 1;
        }
        tokens.push((start, &s[start..i]));
    }
    tokens
}

fn utf8_char_len(first_byte: u8) -> usize {
    if first_byte < 0x80 {
        1
    } else if first_byte < 0xC0 {
        1 // invalid continuation byte; advance one to avoid infinite loop
    } else if first_byte < 0xE0 {
        2
    } else if first_byte < 0xF0 {
        3
    } else {
        4
    }
}

/// Compute a word-level diff between two single lines.
///
/// Returns `None` when either line is empty or the diff is too noisy to be
/// useful (the lines barely share any tokens). In that case the caller should
/// fall back to plain line-level highlighting.
pub fn diff_lines(old: &str, new: &str) -> Option<WordDiff> {
    if old.is_empty() || new.is_empty() {
        return None;
    }

    // Skip pairs that are too long — full LCS becomes O(n*m) and we don't want
    // to spend more than ~a millisecond per pair.
    const MAX_TOKENS: usize = 256;
    let old_tokens = tokenize(old);
    let new_tokens = tokenize(new);
    if old_tokens.len() > MAX_TOKENS || new_tokens.len() > MAX_TOKENS {
        return None;
    }

    // Standard LCS DP table.
    let n = old_tokens.len();
    let m = new_tokens.len();
    let mut dp = vec![0u16; (n + 1) * (m + 1)];
    let stride = m + 1;
    for i in 0..n {
        for j in 0..m {
            let v = if old_tokens[i].1 == new_tokens[j].1 {
                dp[i * stride + j] + 1
            } else {
                dp[(i + 1) * stride + j].max(dp[i * stride + j + 1])
            };
            dp[(i + 1) * stride + j + 1] = v;
        }
    }

    let lcs_len = dp[n * stride + m] as usize;
    // If the lines share fewer than 30% of tokens with the longer one, intra-line highlighting is just noise.
    let shared_ratio = lcs_len as f32 / n.max(m) as f32;
    if shared_ratio < 0.3 {
        return None;
    }

    // Walk back to produce inserted/deleted ranges. We coalesce adjacent
    // ranges so the highlighter draws one rectangle per contiguous change.
    let mut deleted: Vec<ByteRange> = Vec::new();
    let mut inserted: Vec<ByteRange> = Vec::new();

    let mut i = n;
    let mut j = m;
    while i > 0 && j > 0 {
        if old_tokens[i - 1].1 == new_tokens[j - 1].1 {
            i -= 1;
            j -= 1;
        } else if dp[(i - 1) * stride + j] >= dp[i * stride + j - 1] {
            // deletion of old_tokens[i-1]
            let (start, tok) = old_tokens[i - 1];
            push_range(&mut deleted, ByteRange { start, end: start + tok.len() });
            i -= 1;
        } else {
            let (start, tok) = new_tokens[j - 1];
            push_range(&mut inserted, ByteRange { start, end: start + tok.len() });
            j -= 1;
        }
    }
    while i > 0 {
        let (start, tok) = old_tokens[i - 1];
        push_range(&mut deleted, ByteRange { start, end: start + tok.len() });
        i -= 1;
    }
    while j > 0 {
        let (start, tok) = new_tokens[j - 1];
        push_range(&mut inserted, ByteRange { start, end: start + tok.len() });
        j -= 1;
    }

    deleted.reverse();
    inserted.reverse();
    coalesce(&mut deleted);
    coalesce(&mut inserted);

    Some(WordDiff { deleted, inserted })
}

fn push_range(out: &mut Vec<ByteRange>, r: ByteRange) {
    if let Some(last) = out.last_mut() {
        // We push in reverse (right-to-left), so adjacent ranges abut at start/end.
        if last.start == r.end {
            last.start = r.start;
            return;
        }
    }
    out.push(r);
}

fn coalesce(out: &mut Vec<ByteRange>) {
    if out.len() < 2 {
        return;
    }
    let mut merged: Vec<ByteRange> = Vec::with_capacity(out.len());
    merged.push(out[0]);
    for r in &out[1..] {
        let last = merged.last_mut().unwrap();
        if last.end == r.start {
            last.end = r.end;
        } else {
            merged.push(*r);
        }
    }
    *out = merged;
}
