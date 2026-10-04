//! Conformance against Emacs: each fixture has a `.elements` file produced by
//! `org-element-parse-buffer` (see `scripts/org-element-dump.el`). The parser
//! must find the same elements and objects at the same positions.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use org_syntax::{ast, parse, SyntaxKind, SyntaxKind::*, SyntaxNode, TodoKeywords};

/// org-element types this parser does not produce. They are dropped from the
/// reference before comparing.
const NOT_PARSED_YET: &[&str] = &["org-data", "line-break", "citation-reference", "inlinetask"];

const OBJECTS: &[&str] = &[
    "bold",
    "italic",
    "underline",
    "strike-through",
    "code",
    "verbatim",
    "subscript",
    "superscript",
    "link",
    "timestamp",
    "footnote-reference",
    "statistics-cookie",
    "target",
    "radio-target",
    "entity",
    "latex-fragment",
    "export-snippet",
    "macro",
    "inline-src-block",
    "inline-babel-call",
    "citation",
];

type Row = (usize, usize, String);

struct Chars {
    chars: Vec<char>,
    /// Byte offset -> char offset, for every char boundary.
    by_byte: Vec<usize>,
}

impl Chars {
    fn new(text: &str) -> Self {
        let mut by_byte = vec![0; text.len() + 1];
        for (count, (byte, _)) in text.char_indices().enumerate() {
            by_byte[byte] = count;
        }
        by_byte[text.len()] = text.chars().count();
        Chars { chars: text.chars().collect(), by_byte }
    }

    /// Skip whitespace-only lines forward.
    fn element_begin(&self, mut begin: usize) -> usize {
        loop {
            let mut k = begin;
            while k < self.chars.len() && matches!(self.chars[k], ' ' | '\t' | '\r') {
                k += 1;
            }
            if k < self.chars.len() && self.chars[k] == '\n' {
                begin = k + 1;
            } else {
                return begin;
            }
        }
    }

    /// Drop trailing blank lines, keeping the last line's terminator.
    fn element_end(&self, end: usize) -> usize {
        let mut k = end;
        while k > 0 && self.chars[k - 1].is_whitespace() {
            k -= 1;
        }
        while k < self.chars.len() && self.chars[k] != '\n' {
            k += 1;
        }
        (k + 1).min(self.chars.len())
    }
}

fn reference_rows(dump: &str, chars: &Chars) -> BTreeSet<Row> {
    let mut rows = BTreeSet::new();
    for line in dump.lines() {
        let fields: Vec<&str> = line.split(' ').collect();
        let ty = fields[0];
        if NOT_PARSED_YET.contains(&ty) {
            continue;
        }
        let number = |index: usize| fields[index].parse::<i64>().unwrap();
        let (begin, end, post_blank) = (number(1) as usize, number(2) as usize, number(3) as usize);
        let (contents_begin, contents_end) = (number(4), number(5));
        let row = if ty == "table-cell" {
            if contents_begin < 0 || contents_begin == contents_end {
                continue;
            }
            (contents_begin as usize, contents_end as usize, ty.to_string())
        } else if OBJECTS.contains(&ty) {
            (begin, end - post_blank, ty.to_string())
        } else {
            (chars.element_begin(begin), chars.element_end(end), ty.to_string())
        };
        if row.0 < row.1 {
            rows.insert(row);
        }
    }
    rows
}

fn org_element_type(node: &SyntaxNode) -> Option<&'static str> {
    Some(match node.kind() {
        SECTION => "section",
        HEADLINE => "headline",
        PLANNING => "planning",
        PROPERTY_DRAWER => "property-drawer",
        NODE_PROPERTY => "node-property",
        DRAWER => "drawer",
        BLOCK => {
            let begin = node
                .children_with_tokens()
                .filter_map(|child| child.into_token())
                .find(|token| token.kind() == BLOCK_BEGIN)
                .unwrap();
            match begin.text()[8..].to_ascii_uppercase().as_str() {
                "SRC" => "src-block",
                "EXAMPLE" => "example-block",
                "EXPORT" => "export-block",
                "COMMENT" => "comment-block",
                "VERSE" => "verse-block",
                "QUOTE" => "quote-block",
                "CENTER" => "center-block",
                _ => "special-block",
            }
        }
        DYNAMIC_BLOCK => "dynamic-block",
        KEYWORD => {
            // Table formulas and affiliated keywords are properties of
            // their element in org-element.
            if node.parent().is_some_and(|parent| parent.kind() == TABLE) || ast::is_affiliated_keyword(node) {
                return None;
            }
            let is_call = node
                .first_token()
                .is_some_and(|key| key.kind() == KEYWORD_KEY && key.text().eq_ignore_ascii_case("#+CALL:"));
            if is_call {
                "babel-call"
            } else {
                "keyword"
            }
        }
        COMMENT => "comment",
        FIXED_WIDTH => "fixed-width",
        HORIZONTAL_RULE => "horizontal-rule",
        TABLE => "table",
        TABLE_ROW => "table-row",
        TABLE_CELL => "table-cell",
        LIST => "plain-list",
        LIST_ITEM => "item",
        FOOTNOTE_DEFINITION => "footnote-definition",
        CLOCK => "clock",
        LATEX_ENVIRONMENT => "latex-environment",
        DIARY_SEXP => "diary-sexp",
        PARAGRAPH => "paragraph",
        BOLD => "bold",
        ITALIC => "italic",
        UNDERLINE => "underline",
        STRIKE_THROUGH => "strike-through",
        CODE => "code",
        VERBATIM => "verbatim",
        SUBSCRIPT => "subscript",
        SUPERSCRIPT => "superscript",
        LINK | PLAIN_LINK | ANGLE_LINK => "link",
        TIMESTAMP => {
            // Planning and clock timestamps are properties in org-element.
            if node.parent().is_some_and(|parent| matches!(parent.kind(), PLANNING | CLOCK)) {
                return None;
            }
            "timestamp"
        }
        FOOTNOTE_REFERENCE => "footnote-reference",
        STATISTICS_COOKIE => "statistics-cookie",
        TARGET => "target",
        RADIO_TARGET => "radio-target",
        ENTITY => "entity",
        LATEX_FRAGMENT => "latex-fragment",
        EXPORT_SNIPPET => "export-snippet",
        MACRO => "macro",
        INLINE_SRC_BLOCK => "inline-src-block",
        INLINE_BABEL_CALL => "inline-babel-call",
        CITATION => "citation",
        _ => return None,
    })
}

fn parser_rows(text: &str, chars: &Chars) -> BTreeSet<Row> {
    let root = parse(text, &TodoKeywords::default()).syntax();
    let mut rows = BTreeSet::new();
    for node in root.descendants() {
        let Some(ty) = org_element_type(&node) else { continue };
        let range = node.text_range();
        let (mut begin, mut end) = (usize::from(range.start()), usize::from(range.end()));
        if node.kind() == TABLE_CELL {
            // Compare cell contents: the reference includes the closing pipe.
            let inner: Vec<_> =
                node.children_with_tokens().filter(|child| child.kind() != SyntaxKind::WHITESPACE).collect();
            let (Some(first), Some(last)) = (inner.first(), inner.last()) else { continue };
            begin = first.text_range().start().into();
            end = last.text_range().end().into();
        }
        let (begin, end) = (chars.by_byte[begin], chars.by_byte[end]);
        let row = if OBJECTS.contains(&ty) || ty == "table-cell" {
            (begin, end, ty.to_string())
        } else {
            (chars.element_begin(begin), chars.element_end(end), ty.to_string())
        };
        if row.0 < row.1 {
            rows.insert(row);
        }
    }
    rows
}

fn fixtures() -> Vec<PathBuf> {
    // `ORG_FIXTURES` points the tests at another directory, which is how
    // `scripts/fuzz-against-emacs.py` feeds them generated documents.
    let dir = match std::env::var_os("ORG_FIXTURES") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures"),
    };
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "org"))
        .collect();
    files.sort();
    files
}

fn describe(text: &Chars, row: &Row) -> String {
    let snippet: String = text.chars[row.0..row.1.min(row.0 + 40)].iter().collect();
    format!("  {} {}..{} {:?}", row.2, row.0, row.1, snippet)
}

#[test]
fn matches_org_element() {
    let mut failures = String::new();
    let mut compared = 0;
    for path in fixtures() {
        let reference = path.with_extension("elements");
        let Ok(dump) = std::fs::read_to_string(&reference) else { continue };
        let text = std::fs::read_to_string(&path).unwrap();
        let chars = Chars::new(&text);
        let expected = reference_rows(&dump, &chars);
        let actual = parser_rows(&text, &chars);
        compared += 1;
        let missing: Vec<_> = expected.difference(&actual).collect();
        let extra: Vec<_> = actual.difference(&expected).collect();
        if missing.is_empty() && extra.is_empty() {
            continue;
        }
        failures.push_str(&format!("\n{}\n", path.file_name().unwrap().to_string_lossy()));
        for row in missing {
            failures.push_str(&format!("only in org-element:{}\n", describe(&chars, row)));
        }
        for row in extra {
            failures.push_str(&format!("only in org-syntax: {}\n", describe(&chars, row)));
        }
    }
    assert!(compared > 0, "no fixture has a .elements reference");
    assert!(failures.is_empty(), "parser disagrees with org-element:{failures}");
}

#[test]
fn round_trips_every_fixture() {
    for path in fixtures() {
        let text = std::fs::read_to_string(&path).unwrap();
        let root = parse(&text, &TodoKeywords::default()).syntax();
        assert_eq!(root.text().to_string(), text, "{}", path.display());
    }
}
