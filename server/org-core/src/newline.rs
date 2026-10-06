//! `org-return`, Org's RET: in a table, the same column of the next row;
//! in a headline, the rest of the title on a line of its own with the
//! tags kept; in a list item, the rest of the line kept in the item; else
//! a new line. The new line is indented as `electric-indent-mode` does in
//! Org buffers: `org-indent-line` on it, the blanks at the end of the line
//! left removed. Checked against Emacs in `tests/return`.

use org_syntax::{SyntaxKind::*, SyntaxNode, TodoKeywords};

use crate::format;
use crate::indent;
use crate::text::tag_group;
use crate::todo::{headline_level, tags_padding};
use crate::Refusal;

pub struct Settings {
    pub indent: indent::Settings,
    /// `org-tags-column`.
    pub tags_column: i64,
}

/// The document after the key, and where the cursor is in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub text: String,
    pub cursor: usize,
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |newline| newline + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |newline| at + newline)
}

/// The innermost element around `at`, by the parser's tree.
fn ancestors_at(root: &SyntaxNode, at: usize) -> Vec<SyntaxNode> {
    let offset = (at.min(u32::MAX as usize) as u32).into();
    let mut node = root.clone();
    let mut out = vec![node.clone()];
    loop {
        let next = node.children().find(|child| {
            let range = child.text_range();
            range.start() <= offset && (offset < range.end() || (offset == range.end() && range.is_empty()))
        });
        match next {
            Some(child) => {
                out.push(child.clone());
                node = child;
            }
            None => return out,
        }
    }
}

fn is_table_line(line: &str) -> bool {
    line.trim_start_matches([' ', '\t']).starts_with('|')
}

/// Whether `at` is in the code of a source block, between its first and
/// last lines.
fn in_code(nodes: &[SyntaxNode], text: &str, at: usize) -> bool {
    nodes.iter().filter(|node| node.kind() == BLOCK).any(|block| {
        let range = block.text_range();
        let (begin, end) = (usize::from(range.start()), usize::from(range.end()));
        let first_end = line_end(text, begin);
        let body = text[begin..end].trim_end_matches(['\n', ' ', '\t']);
        let last_start = line_start(text, begin + body.len());
        text[begin..first_end].trim_start().to_ascii_lowercase().starts_with("#+begin_src")
            && at >= begin
            && at < last_start
    })
}

/// Whether `org-element-lineage` of the element at `at` (not that element
/// itself) has an item: the cursor is in an element inside an item, as
/// its paragraph, not on the item's own bullet, checkbox or tag.
fn in_item(nodes: &[SyntaxNode]) -> bool {
    let element = |kind| {
        matches!(
            kind,
            PARAGRAPH
                | LIST
                | LIST_ITEM
                | TABLE
                | BLOCK
                | DRAWER
                | DYNAMIC_BLOCK
                | KEYWORD
                | COMMENT
                | FIXED_WIDTH
                | HORIZONTAL_RULE
                | FOOTNOTE_DEFINITION
                | CLOCK
                | LATEX_ENVIRONMENT
                | DIARY_SEXP
                | PROPERTY_DRAWER
                | PLANNING
        )
    };
    nodes.iter().position(|node| node.kind() == LIST_ITEM).is_some_and(|item| {
        let deepest = nodes.iter().rposition(|node| element(node.kind()));
        deepest.is_some_and(|deepest| deepest > item)
    })
}

/// RET at byte `at` of `text`. In the code of a source block, which the
/// language's mode indents in Emacs, it refuses: the editor's own RET
/// applies there.
pub fn org_return(text: &str, at: usize, keywords: &TodoKeywords, settings: &Settings) -> Result<Outcome, Refusal> {
    let root = org_syntax::parse(text, keywords).syntax();
    let nodes = ancestors_at(&root, at);
    if in_code(&nodes, text, at) {
        return Err(Refusal::NotHere("In the code of a source block".to_string()));
    }
    let kinds: Vec<_> = nodes.iter().map(SyntaxNode::kind).collect();
    let start = line_start(text, at);
    let end = line_end(text, at);
    let line = &text[start..end];

    if kinds.contains(&TABLE) && is_table_line(line) {
        let before = &text[start..at];
        if text[at..end].trim_matches([' ', '\t']).is_empty() || before.trim_matches([' ', '\t']).is_empty() {
            return Ok(insert(text, at, "\n", at + 1));
        }
        return Ok(next_row(text, at));
    }

    if at > start && headline_level(line).is_some() {
        if let Some(outcome) = split_headline(text, at, keywords, settings) {
            return Ok(outcome);
        }
    }

    if at < end && in_item(&nodes) {
        let trailing = text[at..end].to_string();
        let removed = format!("{}{}", &text[..at], &text[end..]);
        let (mut outcome, _) = newline(&removed, at, settings);
        outcome.text.insert_str(outcome.cursor, &trailing);
        return Ok(outcome);
    }

    Ok(newline(text, at, settings).0)
}

fn insert(text: &str, at: usize, what: &str, cursor: usize) -> Outcome {
    Outcome { text: format!("{}{}{}", &text[..at], what, &text[at..]), cursor }
}

/// `newline` from a key, with `electric-indent-mode` in an Org buffer: the
/// blanks before the cursor go, and the new line, with what was after the
/// cursor, is indented by `org-indent-line`. The cursor is at its
/// indentation. Also returns the start of the new line.
fn newline(text: &str, at: usize, settings: &Settings) -> (Outcome, usize) {
    let start = line_start(text, at);
    let kept = text[start..at].trim_end_matches([' ', '\t']).len() + start;
    let joined = format!("{}\n{}", &text[..kept], &text[at..]);
    let new_line = kept + 1;
    // Where `org-indent-line` declines (a list's first line, code), the
    // line and the cursor stay; else the cursor goes to the indentation.
    if indent::line_column(&joined, new_line, &settings.indent).is_none() {
        return (Outcome { text: joined, cursor: new_line }, new_line);
    }
    let indented = match indent::indent_line(&joined, new_line, &settings.indent) {
        Some(edit) => indent::apply(&joined, std::slice::from_ref(&edit)),
        None => joined,
    };
    let blanks = indented[new_line..].bytes().take_while(|b| matches!(b, b' ' | b'\t')).count();
    (Outcome { text: indented, cursor: new_line + blanks }, new_line)
}

/// A headline split at the cursor: the rest of the title goes to a new
/// line, the tags stay (aligned again). On the stars' line before the
/// title, in the keyword, the priority or the tags, nothing is split: the
/// new line comes after the headline.
fn split_headline(text: &str, at: usize, keywords: &TodoKeywords, settings: &Settings) -> Option<Outcome> {
    let start = line_start(text, at);
    let end = line_end(text, at);
    let line = &text[start..end];
    let stars = headline_level(line)?;
    // `org-complex-heading-regexp`: stars, keyword, priority, title, tags.
    let mut title_start = stars + 1;
    let rest = &line[title_start..];
    let word = rest.split([' ', '\t']).next().unwrap_or("");
    if !word.is_empty() && keywords.contains(word) && (rest.len() == word.len() || rest[word.len()..].starts_with(' '))
    {
        title_start += word.len();
        title_start += line[title_start..].len() - line[title_start..].trim_start_matches(' ').len();
    }
    let rest = &line[title_start..];
    if rest.len() >= 4 && rest.starts_with("[#") && rest.as_bytes()[3] == b']' {
        title_start += 4;
        title_start += line[title_start..].len() - line[title_start..].trim_start_matches(' ').len();
    }
    let tags = tag_group(line);
    let title_end = match tags {
        Some((blanks, _)) => blanks.max(title_start),
        None => line.trim_end_matches([' ', '\t']).len().max(title_start),
    };
    let title_end = line[..title_end].trim_end_matches([' ', '\t']).len().max(title_start);
    let cursor = at - start;
    // Only within the title (group 4 of the regexp) is the line split.
    let string = (title_end > title_start && cursor >= title_start && cursor <= title_end)
        .then(|| line[cursor..title_end].to_string());
    let mut headline = line.to_string();
    if let Some(string) = &string {
        headline.replace_range(cursor..cursor + string.len(), "");
        if tags.is_some() {
            if let Some((blanks, tags, padding)) = tags_padding(&headline, settings.tags_column) {
                headline = format!("{}{}{}", &headline[..blanks], padding, &headline[tags..]);
            }
        }
    }
    let rebuilt = format!("{}{}{}", &text[..start], headline, &text[end..]);
    let (mut outcome, _) = newline(&rebuilt, start + headline.len(), settings);
    if let Some(string) = string {
        outcome.text.insert_str(outcome.cursor, string.trim_matches([' ', '\t', '\n']));
    }
    Some(outcome)
}

/// `org-table-next-row` after `org-table-align`: the cursor in the same
/// column of the next row, in a new row when there is none or the next is
/// a rule.
fn next_row(text: &str, at: usize) -> Outcome {
    let start = line_start(text, at);
    // The table: the lines that start with `|` around the cursor.
    let mut first = start;
    while first > 0 {
        let previous = line_start(text, first - 1);
        if !is_table_line(&text[previous..first - 1]) {
            break;
        }
        first = previous;
    }
    let mut last_end = line_end(text, at);
    while last_end < text.len() {
        let next_end = line_end(text, last_end + 1);
        if !is_table_line(&text[last_end + 1..next_end]) {
            break;
        }
        last_end = next_end;
    }
    let lines: Vec<&str> = text[first..last_end].split('\n').collect();
    let row = text[first..start].matches('\n').count();
    let column = text[start..at].matches('|').count();
    let mut aligned = format::align_table(&lines);
    let next_is_row = aligned.get(row + 1).is_some_and(|line| !line.trim_start().starts_with("|-"));
    if !next_is_row {
        // `org-table-clean-line`: the row with blanks in place of its
        // fields, its indentation kept; from a rule, every character but
        // the bars and `+` (which become bars) is a blank.
        let source = &aligned[row];
        let rule = source.trim_start().starts_with("|-");
        let bar = source.find('|').unwrap_or(0);
        let clean: String = if rule {
            source.chars().map(|c| if c == '|' || c == '+' { '|' } else { ' ' }).collect()
        } else {
            source[..bar].to_string()
                + &source[bar..].chars().map(|c| if c == '|' { '|' } else { ' ' }).collect::<String>()
        };
        aligned.insert(row + 1, clean);
    }
    let target = &aligned[row + 1];
    let bars: Vec<usize> = target.match_indices('|').map(|(index, _)| index).collect();
    let mut cursor = bars.get(column.saturating_sub(1)).or(bars.last()).map_or(0, |bar| bar + 1);
    if target[cursor..].starts_with(' ') {
        cursor += 1;
    }
    let table = aligned.join("\n");
    let mut out = format!("{}{}{}", &text[..first], table, &text[last_end..]);
    let row_start = first + aligned[..=row].iter().map(|line| line.len() + 1).sum::<usize>();
    // A table at the end of a file without a final newline gets one.
    if last_end == text.len() && !next_is_row && !out.ends_with('\n') {
        out.push('\n');
    }
    Outcome { text: out, cursor: row_start + cursor }
}
