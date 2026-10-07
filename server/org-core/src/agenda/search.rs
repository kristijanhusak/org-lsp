//! What `org-search-view` searches for: the words, phrases and regexps
//! of its search string, read as Org reads them.
//!
//! The string is a phrase whose spaces stand for any whitespace, unless it
//! is Boolean (`org-agenda-search-view-always-boolean`, or a first word
//! that starts with `+`, `-` or `{`): then each word must (`+word`, `word`)
//! or must not (`-word`) occur in the entry, `{regexp}` is a regexp, and
//! `"two words"` is one. A leading `*` searches headlines only, then `!`
//! open TODO entries only, then `:` full words only.

use regex::{Regex, RegexBuilder};

use super::matcher;

/// The settings of the search view.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchSettings {
    /// `org-agenda-search-view-always-boolean`
    pub always_boolean: bool,
    /// `org-agenda-search-view-force-full-words`
    pub full_words: bool,
    /// `org-agenda-search-view-max-outline-level`: 0 for every level.
    pub max_level: usize,
}

/// A search string, read.
#[derive(Debug, Clone)]
pub struct Search {
    /// What finds the entries to look at: the longest of the words that
    /// must occur, or else every headline.
    pub regexp: Pattern,
    /// The other words that must occur in the entry.
    pub plus: Vec<Pattern>,
    /// The words that must not.
    pub minus: Vec<Pattern>,
    /// `*`: only the headline counts.
    pub headline_only: bool,
    /// `!`: only open TODO entries.
    pub todo_only: bool,
}

/// A word, phrase or regexp of a search string.
#[derive(Debug, Clone)]
pub struct Pattern {
    regex: Regex,
    /// A full word: `\<word\>`, which the regex alone does not check.
    word: bool,
    /// Only in a headline: `^\*+ .*?word`, which the regex has unless it
    /// is a full word.
    headline: bool,
}

impl Pattern {
    /// Where the pattern matches first, from `at` on.
    pub fn find_at(&self, text: &str, at: usize) -> Option<(usize, usize)> {
        let mut from = at;
        loop {
            let found = self.regex.find_at(text, from)?;
            let (start, end) = (found.start(), found.end());
            from = start + text[start..].chars().next().map_or(1, char::len_utf8);
            if !self.word {
                return Some((start, end));
            }
            if !word_bounded(text, start, end) {
                continue;
            }
            if !self.headline {
                return Some((start, end));
            }
            // The headline the word is in, from its beginning.
            let line = text[..start].rfind('\n').map_or(0, |newline| newline + 1);
            let stars = text[line..].bytes().take_while(|byte| *byte == b'*').count();
            if line >= at && stars > 0 && text[line + stars..].starts_with(' ') && start > line + stars {
                return Some((line, end));
            }
        }
    }

    /// Whether the pattern matches `text`.
    pub fn is_match(&self, text: &str) -> bool {
        self.find_at(text, 0).is_some()
    }
}

/// Whether `text[start..end]` is a word as `\<...\>` has it: it starts
/// and ends with a word character, with none just before or after.
fn word_bounded(text: &str, start: usize, end: usize) -> bool {
    let word = |c: Option<char>| c.is_some_and(is_word);
    word(text[start..end].chars().next())
        && word(text[start..end].chars().next_back())
        && !word(text[..start].chars().next_back())
        && !word(text[end..].chars().next())
}

/// Whether `c` is a word character in Org's search syntax table.
fn is_word(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric() || c == '$' || c == '%';
    }
    let code = u32::from(c);
    !NOT_WORD.iter().any(|(first, last)| (*first..=*last).contains(&code))
}

/// The characters past ASCII that are not words in Org's search syntax
/// table, as Emacs 29 has them.
const NOT_WORD: &[(u32, u32)] = &[
    (0xA0, 0xA4),
    (0xA6, 0xB1),
    (0xB4, 0xB4),
    (0xB6, 0xB6),
    (0xB8, 0xB8),
    (0xBA, 0xBF),
    (0xD7, 0xD7),
    (0xF7, 0xF7),
    (0x2C7, 0x2C7),
    (0x2C9, 0x2C9),
    (0x2D0, 0x2D0),
    (0x2D8, 0x2DB),
    (0x2DD, 0x2DD),
    (0x384, 0x385),
    (0x5BE, 0x5BE),
    (0x5C0, 0x5C0),
    (0x5C3, 0x5C3),
    (0x5C6, 0x5C6),
    (0xE2F, 0xE2F),
    (0xE3F, 0xE3F),
    (0xE46, 0xE46),
    (0xE4F, 0xE4F),
    (0xE5A, 0xE5B),
    (0xEAF, 0xEAF),
    (0xEC6, 0xEC6),
    (0xF00, 0xF0B),
    (0xF0D, 0xF18),
    (0xF1A, 0xF1F),
    (0xF34, 0xF34),
    (0xF36, 0xF36),
    (0xF38, 0xF3F),
    (0xF7F, 0xF7F),
    (0xF85, 0xF85),
    (0xFBE, 0xFCF),
    (0x1361, 0x1368),
    (0x1800, 0x180A),
    (0x2000, 0x2026),
    (0x202F, 0x205F),
    (0x207D, 0x207E),
    (0x208D, 0x208E),
    (0x20AC, 0x20AC),
    (0x2103, 0x2103),
    (0x2109, 0x2109),
    (0x2116, 0x2116),
    (0x2121, 0x2122),
    (0x2153, 0x2154),
    (0x215B, 0x215E),
    (0x2190, 0x244F),
    (0x2460, 0x246E),
    (0x2474, 0x24B5),
    (0x2500, 0x254B),
    (0x2592, 0x2592),
    (0x25A0, 0x25A1),
    (0x25A3, 0x25A9),
    (0x25B2, 0x25B3),
    (0x25B6, 0x25B7),
    (0x25BC, 0x25BD),
    (0x25C0, 0x25C1),
    (0x25C6, 0x25C8),
    (0x25CB, 0x25CB),
    (0x25CE, 0x25D1),
    (0x25EF, 0x25EF),
    (0x2605, 0x2606),
    (0x260E, 0x260F),
    (0x261C, 0x261C),
    (0x261E, 0x261E),
    (0x2640, 0x2640),
    (0x2642, 0x2642),
    (0x2660, 0x2661),
    (0x2663, 0x2665),
    (0x2667, 0x266A),
    (0x266C, 0x266D),
    (0x266F, 0x266F),
    (0x2768, 0x276D),
    (0x2770, 0x2775),
    (0x27E6, 0x27EB),
    (0x2983, 0x2998),
    (0x29FC, 0x29FD),
    (0x2A00, 0x2BFF),
    (0x2E00, 0x2E7F),
    (0x3000, 0x3003),
    (0x3008, 0x301C),
    (0x30FB, 0x30FB),
    (0x3200, 0x321C),
    (0x3220, 0x3229),
    (0x3260, 0x327B),
    (0x327E, 0x327F),
    (0x3380, 0x3384),
    (0x3388, 0x33CA),
    (0x33CF, 0x33D0),
    (0x33D3, 0x33D3),
    (0x33D6, 0x33D6),
    (0x33D8, 0x33D8),
    (0x33DB, 0x33DD),
    (0xAADB, 0xAADF),
    (0xFD3E, 0xFD3F),
    (0xFE35, 0xFE44),
    (0xFE59, 0xFE5E),
    (0xFF01, 0xFF0F),
    (0xFF1B, 0xFF20),
    (0xFF3B, 0xFF40),
    (0xFF5B, 0xFF65),
    (0xFFE0, 0xFFE3),
    (0xFFE5, 0xFFE5),
    (0x11660, 0x1166C),
    (0x1FB00, 0x1FBFF),
];

/// Whitespace as `\s-` has it in Org's search syntax table.
const WHITESPACE: &str = r"[ \t\n\r\x0c]";

fn build(pattern: &str) -> Result<Regex, String> {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .map_err(|_| format!("Invalid regexp: \"{pattern}\""))
}

/// A word, phrase or regexp: the regex, and whether it is a full word.
type Wanted = (usize, String, bool);

fn pattern((_, regex, word): &Wanted, headline: bool) -> Result<Pattern, String> {
    let regex = if headline && !word { format!(r"^\*+ .*?{regex}") } else { regex.clone() };
    Ok(Pattern { regex: build(&regex)?, word: *word, headline })
}

/// The words of `string`, after `*`, `!` and `:`.
fn words(string: &str) -> &str {
    let words = string.strip_prefix('*').unwrap_or(string);
    let words = words.strip_prefix('!').unwrap_or(words);
    words.strip_prefix(':').unwrap_or(words)
}

/// Whether `string` is read as Boolean: each word must occur, or must not
/// with `-`.
pub fn is_boolean(string: &str, settings: &SearchSettings) -> bool {
    settings.always_boolean || words(string).starts_with(['-', '+', '{'])
}

/// `org-agenda-manipulate-query`: `string` with what an earlier call added
/// taken off (trailing spaces, or ` +`, ` -`, ` +{}`, ` -{}`), and then
/// `added` (one of those), or a space when `string` is not Boolean.
pub fn manipulate(string: &str, added: &str, settings: &SearchSettings) -> String {
    let mut kept = string.trim_end_matches(' ');
    if kept.len() == string.len() {
        let rest = string.strip_suffix("{}").unwrap_or(string);
        if let Some(rest) = rest.strip_suffix(['+', '-']) {
            if rest.ends_with(' ') {
                kept = rest.trim_end_matches(' ');
            }
        }
    }
    let added = if is_boolean(kept, settings) { added } else { " " };
    format!("{kept}{added}")
}

/// The search string `string` as `org-search-view` reads it, with
/// `todo_only` from the command (`S`).
pub fn parse(string: &str, todo_only: bool, settings: &SearchSettings) -> Result<Search, String> {
    let mut todo_only = todo_only;
    let mut full_words = settings.full_words;
    let (headline_only, mut words) = match string.strip_prefix('*') {
        Some(rest) => (true, rest),
        None => (false, string),
    };
    if let Some(rest) = words.strip_prefix('!') {
        todo_only = true;
        words = rest;
    }
    if let Some(rest) = words.strip_prefix(':') {
        full_words = true;
        words = rest;
    }
    let boolean = is_boolean(string, settings);
    // A word ending in a backslash goes on with the next one, after a
    // space.
    let mut joined: Vec<String> = Vec::new();
    let mut parts = words.split_whitespace();
    while let Some(word) = parts.next() {
        let mut word = word.to_string();
        while word.ends_with('\\') {
            let Some(next) = parts.next() else { break };
            word.pop();
            word = format!("{word} {next}");
        }
        joined.push(word);
    }
    // A regexp goes on to the word that closes its brace.
    let mut words: Vec<String> = Vec::new();
    let mut parts = joined.into_iter();
    while let Some(mut word) = parts.next() {
        let body = word.strip_prefix(['+', '-']).unwrap_or(&word);
        if body.starts_with('{') && !word.ends_with('}') {
            for next in parts.by_ref() {
                word = format!("{word} {next}");
                if word.ends_with('}') {
                    break;
                }
            }
        }
        words.push(word);
    }
    // Each pattern with the length of Emacs's regexp for it, which orders
    // them.
    let mut plus: Vec<Wanted> = Vec::new();
    let mut minus: Vec<Wanted> = Vec::new();
    if boolean {
        // A quoted phrase goes on to the word that closes it.
        let mut quoted: Vec<String> = Vec::new();
        let mut parts = words.into_iter();
        while let Some(mut word) = parts.next() {
            let body = word.strip_prefix(['+', '-']).filter(|_| word.chars().count() > 1).unwrap_or(&word);
            if word.starts_with('"') || body.starts_with('"') {
                while !word.ends_with('"') {
                    let Some(next) = parts.next() else { break };
                    word = format!("{word} {next}");
                }
            }
            let sign = usize::from(word.starts_with(['+', '-']));
            if word[sign..].starts_with('"') {
                word.remove(sign);
            }
            if word.ends_with('"') {
                word.pop();
            }
            quoted.push(word);
        }
        for word in quoted {
            let (negative, word) = match word.chars().next() {
                Some('-') => (true, &word[1..]),
                Some('+') => (false, &word[1..]),
                _ => (false, word.as_str()),
            };
            let lower = word.to_lowercase();
            let pattern = if word.len() >= 2 && word.starts_with('{') && word.ends_with('}') {
                let inner = &word[1..word.len() - 1];
                (inner.chars().count(), matcher::translate(inner), false)
            } else if full_words {
                (quoted_length(&lower) + 4, regex::escape(&lower), true)
            } else {
                (quoted_length(&lower), regex::escape(&lower), false)
            };
            if negative {
                minus.insert(0, pattern);
            } else {
                plus.insert(0, pattern);
            }
        }
    } else {
        let length = words.iter().map(|word| quoted_length(word)).sum::<usize>() + 4 * words.len().saturating_sub(1);
        let phrase: Vec<String> = words.iter().map(|word| regex::escape(word)).collect();
        plus.insert(0, (length, phrase.join(&format!("{WHITESPACE}+")), false));
    }
    // The longest first: it finds the entries to look at.
    plus.sort_by_key(|(length, _, _)| std::cmp::Reverse(*length));
    let regexp = if plus.is_empty() {
        Pattern { regex: build(r"^\*+ ")?, word: false, headline: false }
    } else {
        pattern(&plus.remove(0), headline_only)?
    };
    Ok(Search {
        regexp,
        plus: plus.iter().map(|wanted| pattern(wanted, false)).collect::<Result<_, _>>()?,
        minus: minus.iter().map(|wanted| pattern(wanted, false)).collect::<Result<_, _>>()?,
        headline_only,
        todo_only,
    })
}

/// The length of `regexp-quote`'s regexp for `word`: a backslash before
/// each of `[*.\?+^$`.
fn quoted_length(word: &str) -> usize {
    word.chars().map(|c| if "[*.\\?+^$".contains(c) { 2 } else { 1 }).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manipulates_the_query_as_org_does() {
        let settings = SearchSettings::default();
        assert_eq!(manipulate("+a", " +", &settings), "+a +");
        assert_eq!(manipulate("+a +", " -{}", &settings), "+a -{}");
        assert_eq!(manipulate("+a -{}  ", " -", &settings), "+a -{} -");
        assert_eq!(manipulate("+a   ", " +{}", &settings), "+a +{}");
        assert_eq!(manipulate("+a x+", " +", &settings), "+a x+ +");
        assert_eq!(manipulate("a phrase", " +", &settings), "a phrase ");
        let boolean = SearchSettings { always_boolean: true, ..SearchSettings::default() };
        assert_eq!(manipulate("a", " -", &boolean), "a -");
    }

    #[test]
    fn full_words_are_words_of_the_search_syntax_table() {
        let search = |string: &str| parse(string, false, &SearchSettings::default()).unwrap();
        // `%` and `$` are word characters, `'` and `_` are not.
        assert!(search(":+100%").regexp.find_at("x 100% y", 0).is_some());
        assert!(search(":+100").regexp.find_at("x 100% y", 0).is_none());
        assert!(search(":+$5").regexp.find_at("a $5", 0).is_some());
        assert!(search(":+it").regexp.find_at("it's", 0).is_some());
        assert!(search(":+a").regexp.find_at("a_b", 0).is_some());
        assert!(search(":+(c)").regexp.find_at("(c)", 0).is_none());
        // Headlines only: the first headline with the word, from its
        // beginning.
        let found = search("*:+b").regexp.find_at("* ab\nb\n** x b b\n", 0);
        assert_eq!(found, Some((7, 13)));
    }
}
