//! Radio targets: `<<<My target>>>` turns every "My target" of the text
//! into a link to it (`org-update-radio-target-regexp`). Org finds them in
//! plain text, emphasis included, wherever links may be: paragraphs,
//! headline titles and table cells; not in code, verbatim, links and the
//! like. The words around must not be letters or digits; blanks in a
//! target stand for any blanks. Org reads them ignoring case, but finds
//! the target only as written.

use org_syntax::{Parse, SyntaxKind, SyntaxNode};
use regex::{Regex, RegexBuilder};

/// A radio target: its text and where it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub value: String,
    pub offset: usize,
}

/// A word of the text a radio target makes a link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadioLink {
    pub start: usize,
    pub end: usize,
    /// Where it leads: the start of the radio target of the same words,
    /// as written; none when only one differing in case is there.
    pub target: Option<usize>,
}

/// The radio targets of the text, in its order.
pub fn targets(root: &SyntaxNode) -> Vec<Target> {
    root.descendants()
        .filter(|node| node.kind() == SyntaxKind::RADIO_TARGET)
        .filter_map(|node| {
            let text = node.text().to_string();
            let value = text.strip_prefix("<<<")?.strip_suffix(">>>")?;
            Some(Target { value: value.to_string(), offset: usize::from(node.text_range().start()) })
        })
        .collect()
}

/// The parts of the text radio links may be in: plain text of paragraphs,
/// titles and table cells, inside emphasis or not.
fn plain_ranges(root: &SyntaxNode) -> Vec<(usize, usize)> {
    use SyntaxKind::*;
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for token in root.descendants_with_tokens().filter_map(|element| element.into_token()) {
        if token.kind() != TEXT {
            continue;
        }
        let container = token
            .parent_ancestors()
            .find(|node| !matches!(node.kind(), BOLD | ITALIC | UNDERLINE | STRIKE_THROUGH))
            .map(|node| node.kind());
        if !matches!(container, Some(PARAGRAPH | TITLE | TABLE_CELL)) {
            continue;
        }
        let range = token.text_range();
        let (start, end) = (usize::from(range.start()), usize::from(range.end()));
        match ranges.last_mut() {
            Some(last) if last.1 == start => last.1 = end,
            _ => ranges.push((start, end)),
        }
    }
    ranges
}

/// `org-target-link-regexp`: the targets, longest first.
fn regexp(targets: &[Target]) -> Option<Regex> {
    let mut values: Vec<&str> = Vec::new();
    for target in targets {
        if !values.contains(&target.value.as_str()) {
            values.push(&target.value);
        }
    }
    if values.is_empty() {
        return None;
    }
    values.sort_by_key(|value| std::cmp::Reverse(value.chars().count()));
    let words: Vec<String> = values
        .iter()
        .map(|value| {
            value.split(' ').filter(|word| !word.is_empty()).map(regex::escape).collect::<Vec<_>>().join(r"\s+")
        })
        .collect();
    // `[:alnum:]` is a word character but `_`.
    RegexBuilder::new(&format!(r"(?:^|[^\w]|_)({})(?:$|[^\w]|_)", words.join("|")))
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .ok()
}

/// The radio links of the text.
pub fn links(text: &str, parse: &Parse) -> Vec<RadioLink> {
    let root = parse.syntax();
    let targets = targets(&root);
    let Some(regexp) = regexp(&targets) else { return Vec::new() };
    let ranges = plain_ranges(&root);
    let normal = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = Vec::new();
    let mut at = 0;
    // A match may begin with the character before the words; the next
    // search starts on the character after them, which may begin one.
    while let Some(found) = regexp.captures_at(text, at) {
        let words = found.get(1).expect("a group");
        at = words.end();
        let (start, end) = (words.start(), words.end());
        if !ranges.iter().any(|(from, to)| *from <= start && end <= *to) {
            continue;
        }
        let wanted = normal(words.as_str());
        let target = targets.iter().find(|target| normal(&target.value) == wanted).map(|target| target.offset);
        out.push(RadioLink { start, end, target });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_radio_links() {
        let text =
            "* About new york\n<<<new york>>> and <<<york>>>\n- In =york= and york.\n| york |\nNewyork, New York.\n";
        let parse = org_syntax::parse(text, &Default::default());
        let found: Vec<(&str, Option<usize>)> =
            links(text, &parse).iter().map(|link| (&text[link.start..link.end], link.target)).collect();
        assert_eq!(
            found,
            [("new york", Some(17)), ("york", Some(36)), ("york", Some(36)), ("New York", None)],
            "{found:?}"
        );
    }
}
