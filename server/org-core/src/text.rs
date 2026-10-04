//! String helpers that behave the way the corresponding Emacs primitives do.

use unicode_width::UnicodeWidthChar;

/// Display width of a character, as Emacs's `char-width` counts it.
pub fn char_width(c: char) -> usize {
    match c {
        '\t' => 8,
        _ => c.width().unwrap_or(0),
    }
}

/// Display width of a string, as `string-width` counts it.
pub fn width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

/// `format "%-Ns"`: pad on the right to `min` columns.
pub fn pad_right(text: &str, min: usize) -> String {
    let mut out = text.to_string();
    out.extend(std::iter::repeat_n(' ', min.saturating_sub(width(text))));
    out
}

/// `format "%Ns"`: pad on the left to `min` columns.
pub fn pad_left(text: &str, min: usize) -> String {
    let mut out: String = std::iter::repeat_n(' ', min.saturating_sub(width(text))).collect();
    out.push_str(text);
    out
}

/// `org-trim`: strip blanks, tabs and line breaks on both sides.
pub fn trim(text: &str) -> &str {
    text.trim_matches([' ', '\t', '\n', '\r'])
}

/// Whether `c` has word syntax in an Org buffer.
pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '$' | '%' | '\'')
}

/// Whether two adjacent word characters belong to the same word. Emacs
/// puts a boundary between an ASCII word and most other scripts.
fn joins(before: char, after: char) -> bool {
    let latin = |c: char| c.is_ascii() || matches!(c as u32, 0xAA..=0x24F | 0x1E00..=0x1EFF);
    latin(before) == latin(after)
}

/// `\<`: whether a word starts at byte `at`.
pub fn word_start(text: &str, at: usize) -> bool {
    let Some(next) = text[at..].chars().next().filter(|c| is_word(*c)) else { return false };
    match text[..at].chars().next_back() {
        Some(previous) if is_word(previous) => !joins(previous, next),
        _ => true,
    }
}

/// `\>`: whether a word ends at byte `at`.
pub fn word_end(text: &str, at: usize) -> bool {
    let Some(previous) = text[..at].chars().next_back().filter(|c| is_word(*c)) else { return false };
    match text[at..].chars().next() {
        Some(next) if is_word(next) => !joins(previous, next),
        _ => true,
    }
}

/// Byte offset of the `n`th character, or the length of the string.
pub fn char_offset(text: &str, n: usize) -> usize {
    text.char_indices().nth(n).map_or(text.len(), |(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths() {
        assert_eq!(width("abc"), 3);
        assert_eq!(width("日本"), 4);
        assert_eq!(width("é"), 1);
        assert_eq!(pad_right("日本", 6), "日本  ");
        assert_eq!(pad_left("ab", 5), "   ab");
        assert_eq!(pad_right("abcdef", 3), "abcdef");
    }

    #[test]
    fn word_boundaries() {
        assert!(word_start("at 10:00", 3));
        assert!(!word_start("x10", 1));
        assert!(!word_start("$10", 1));
        assert!(word_start("_10", 1));
        assert!(word_start("日10", "日".len()));
        assert!(word_end("10:00", 2));
        assert!(!word_end("10a", 2));
        assert!(word_end("10", 2));
        assert!(!word_end(" x", 0));
    }
}
