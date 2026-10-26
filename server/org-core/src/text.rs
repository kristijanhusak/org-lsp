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

/// The widest field a width written in a format pads to, in columns.
/// Emacs pads to any width it can allocate, and fails past it (`Memory
/// exhausted`); a width written larger pads to this one here, rather
/// than ending the server.
pub const WIDEST: usize = 1 << 16;

/// The field width written as the ASCII digits `digits`: none is 0, and
/// it is at most [`WIDEST`].
pub fn field_width(digits: &str) -> usize {
    digits
        .bytes()
        .fold(0usize, |width, digit| width.saturating_mul(10).saturating_add(usize::from(digit - b'0')))
        .min(WIDEST)
}

/// `format` with strings only: each `%s`, with a `-` flag, a width and
/// a precision (`%-10s`, `%.3s`), takes the next of `args`; `%%` is `%`.
/// Anything else stays as it is.
pub fn format_strings(format: &str, args: &[&str]) -> String {
    let mut out = String::new();
    let mut args = args.iter();
    let mut rest = format;
    while let Some(at) = rest.find('%') {
        out.push_str(&rest[..at]);
        let spec = &rest[at + 1..];
        if let Some(after) = spec.strip_prefix('%') {
            out.push('%');
            rest = after;
            continue;
        }
        let left = spec.starts_with('-');
        let digits = |text: &str| text.bytes().take_while(u8::is_ascii_digit).count();
        let after_flag = &spec[usize::from(left)..];
        let width_len = digits(after_flag);
        let after_width = &after_flag[width_len..];
        let (precision, after_precision) = match after_width.strip_prefix('.') {
            Some(after_dot) => {
                let length = digits(after_dot);
                (Some(after_dot[..length].parse::<usize>().unwrap_or(0)), &after_dot[length..])
            }
            None => (None, after_width),
        };
        let Some(after) = after_precision.strip_prefix('s') else {
            out.push('%');
            rest = spec;
            continue;
        };
        let mut value = args.next().copied().unwrap_or_default().to_string();
        if let Some(precision) = precision {
            value = value.chars().take(precision).collect();
        }
        let min = field_width(&after_flag[..width_len]);
        out.push_str(&if left { pad_right(&value, min) } else { pad_left(&value, min) });
        rest = after;
    }
    out.push_str(rest);
    out
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

/// Whether `c` may appear in a tag.
fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '@' | '#' | '%')
}

/// `org-tag-group-re`: a tag group at the end of `text`, with the blanks
/// before it. Returns where the blanks start and where the tags start.
pub fn tag_group(text: &str) -> Option<(usize, usize)> {
    let body = text.trim_end_matches([' ', '\t']);
    let run: usize = body.chars().rev().take_while(|c| *c == ':' || is_tag_char(*c)).map(char::len_utf8).sum();
    let tags_start = body.len() - run;
    let tags = &body[tags_start..];
    let preceded_by_blank = body[..tags_start].ends_with([' ', '\t']);
    if !preceded_by_blank || tags.len() < 3 || !tags.starts_with(':') || !tags.ends_with(':') {
        return None;
    }
    let blanks = body[..tags_start].len() - body[..tags_start].trim_end_matches([' ', '\t']).len();
    Some((tags_start - blanks, tags_start))
}

/// How many bytes `old` and `new` have in common at the start and at the
/// end, each a whole number of characters.
pub fn common_ends(old: &str, new: &str) -> (usize, usize) {
    let mut prefix = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
    while !old.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let max_suffix = old.len().min(new.len()) - prefix;
    let mut suffix = old.bytes().rev().zip(new.bytes().rev()).take(max_suffix).take_while(|(a, b)| a == b).count();
    while !old.is_char_boundary(old.len() - suffix) {
        suffix -= 1;
    }
    (prefix, suffix)
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

/// Whether every line of `text` ends with `\r\n`, as a Windows file's
/// do. Org reads lines ended by `\n`: such a text is worked on without its
/// `\r`s ([`to_lf`]), which go back on what comes of it ([`from_lf`]). A
/// text with both endings is left as it is.
pub fn crlf(text: &str) -> bool {
    text.contains("\r\n") && text.matches('\n').count() == text.matches("\r\n").count()
}

/// `text` of [`crlf`] lines without their `\r`s, and where byte `offset`
/// is in it.
pub fn to_lf(text: &str, offset: usize) -> (String, usize) {
    let offset = offset.min(text.len());
    (text.replace("\r\n", "\n"), offset - text[..offset].matches("\r\n").count())
}

/// The reverse of [`to_lf`]: the `\r`s back, and where byte `offset` of
/// `text` is then.
pub fn from_lf(text: &str, offset: usize) -> (String, usize) {
    let offset = offset.min(text.len());
    (text.replace('\n', "\r\n"), offset + text[..offset].matches('\n').count())
}

#[cfg(test)]
mod line_end_tests {
    use super::*;

    #[test]
    fn takes_the_returns_out_and_puts_them_back() {
        assert!(crlf("a\r\nb\r\n"));
        assert!(!crlf("a\r\nb\n"));
        assert!(!crlf("a\nb\n"));
        let (lf, at) = to_lf("ab\r\ncd\r\n", 5);
        assert_eq!((lf.as_str(), at), ("ab\ncd\n", 4));
        assert_eq!(from_lf(&lf, at), ("ab\r\ncd\r\n".to_string(), 5));
    }
}

/// The words of the file's `#+STARTUP:` lines, as `org-collect-keywords`
/// finds them: keywords, so not lines inside a block whose contents Org
/// does not parse (source, example, export, comment and verse blocks).
pub fn startup_words(text: &str) -> Vec<&str> {
    let mut words = Vec::new();
    // The line that ends the block the line is in (`#+end_src`), written
    // once for the block, not for each of its lines.
    let mut inside: Option<String> = None;
    for line in text.lines() {
        let line = line.trim_start_matches([' ', '\t']);
        match &inside {
            Some(end) => {
                if line.get(..end.len()).is_some_and(|start| start.eq_ignore_ascii_case(end)) {
                    inside = None;
                }
            }
            None if line.get(..8).is_some_and(|start| start.eq_ignore_ascii_case("#+begin_")) => {
                let name: String =
                    line[8..].chars().take_while(|c| !c.is_whitespace()).collect::<String>().to_ascii_lowercase();
                if matches!(name.as_str(), "src" | "example" | "export" | "comment" | "verse") {
                    inside = Some(format!("#+end_{name}"));
                }
            }
            None => {
                if let Some(rest) =
                    line.get(..10).filter(|key| key.eq_ignore_ascii_case("#+startup:")).map(|_| &line[10..])
                {
                    words.extend(rest.split_whitespace());
                }
            }
        }
    }
    words
}

#[cfg(test)]
mod startup_tests {
    #[test]
    fn startup_words_are_keywords_only() {
        let text = "#+STARTUP: overview indent\n#+begin_example\n#+STARTUP: showall\n#+end_example\n\
                    #+BEGIN_SRC org\n#+startup: noindent\n#+END_SRC\n#+begin_quote\n#+STARTUP: hidestars\n#+end_quote\n";
        assert_eq!(super::startup_words(text), ["overview", "indent", "hidestars"]);
    }
}

/// `org-replace-escapes`: each `%c` of `template` whose `c` `value` knows
/// replaced by its value, as `format` writes it with the field width and
/// precision written between (`%-12s`): padded to the width in columns,
/// on the left unless the width has a `-`.
pub fn replace_escapes(template: &str, value: impl Fn(char) -> Option<String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(at) = rest.find('%') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let left = after.starts_with('-');
        let spec = &after[usize::from(left)..];
        let digits = spec.bytes().take_while(|b| b.is_ascii_digit() || *b == b'.').count();
        let replaced = spec[digits..].chars().next().and_then(|c| Some((c, value(c)?)));
        let Some((c, mut text)) = replaced else {
            out.push('%');
            rest = after;
            continue;
        };
        let (width, precision) = match spec[..digits].split_once('.') {
            Some((width, precision)) => (width, precision.parse::<usize>().ok()),
            None => (&spec[..digits], None),
        };
        if let Some(precision) = precision {
            text = text.chars().take(precision).collect();
        }
        let width = field_width(width);
        out.push_str(&if left { pad_right(&text, width) } else { pad_left(&text, width) });
        rest = &spec[digits + c.len_utf8()..];
    }
    out.push_str(rest);
    out
}

/// A regexp written as Emacs writes it, matched as `string-match` does,
/// ignoring case (`case-fold-search`).
#[derive(Debug, Clone)]
pub struct EmacsRegexp(regex::Regex);

impl EmacsRegexp {
    /// `None` when it is not a regexp.
    pub fn new(emacs: &str) -> Option<EmacsRegexp> {
        let translated = crate::agenda::matcher::translate(emacs);
        regex::RegexBuilder::new(&translated).case_insensitive(true).build().ok().map(EmacsRegexp)
    }

    pub fn is_match(&self, text: &str) -> bool {
        self.0.is_match(text)
    }
}

impl PartialEq for EmacsRegexp {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl Eq for EmacsRegexp {}
