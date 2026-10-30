//! A lossless, hand-written parser for Org mode documents.
//!
//! [`parse`] never fails: any text produces a tree, and concatenating the
//! tree's tokens gives back the input byte for byte. The tree is a
//! [`rowan`] syntax tree, so every node knows its exact source range.
//!
//! ```
//! use org_syntax::{parse, ast, SyntaxKind, TodoKeywords};
//!
//! let parsed = parse("* TODO Write the parser :work:\n", &TodoKeywords::default());
//! let root = parsed.syntax();
//! assert_eq!(root.kind(), SyntaxKind::DOCUMENT);
//!
//! let headline = ast::headlines(&root).next().unwrap();
//! assert_eq!(headline.todo_keyword().unwrap().text(), "TODO");
//! assert_eq!(headline.title_text(), "Write the parser");
//! assert_eq!(headline.tags(), ["work"]);
//! ```

pub mod ast;
mod config;
mod entities;
mod kind;
mod lists;
mod objects;
mod parser;

pub use config::{Interpretation, Logging, TodoKeywords};
use parser::headline_level;

/// The names of the entities Org knows (`org-entities`), sorted.
/// The link types Org knows (`org-link-types`): `https`, `file`, `id`…
pub fn link_types() -> &'static [&'static str] {
    objects::LINK_TYPES
}

/// Where a plain link (`https://example.com`) starting at byte `at` of
/// `text` ends, as `org-link-plain-re` matches it there.
pub fn plain_link_end(text: &str, at: usize) -> Option<usize> {
    objects::match_plain_link(text, at)
}

pub use entities::set_user_entities;
pub use lists::{list_syntax, set_list_syntax, with_list_syntax, ListSyntax, Terminator};

/// The names of the entities: the user's (`org-entities-user`), then
/// Org's.
pub fn entity_names() -> Vec<&'static str> {
    let user = entities::user();
    let mut names: Vec<&'static str> = user.iter().map(|(name, _)| *name).collect();
    names.extend(entities::ENTITIES.iter().filter(|name| !user.iter().any(|(known, _)| known == *name)));
    names
}

/// What the entity `name` (`alpha`) is in UTF-8 (`α`), as
/// `org-entity-get` has it: the user's entity of that name first.
pub fn entity_utf8(name: &str) -> Option<&'static str> {
    if let Some((_, utf8)) = entities::user().iter().find(|(known, _)| *known == name) {
        return Some(utf8);
    }
    entities::ENTITIES.binary_search(&name).ok().map(|index| entities::UTF8[index])
}
pub use kind::{OrgLanguage, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};
pub use rowan::{TextRange, TextSize, TokenAtOffset};

/// The token at `offset` under `node`, as rowan's `token_at_offset` has it
/// (both tokens where two meet), found by halving the children of each
/// node on the way down: rowan's looks at every child of each, which over
/// a document of thousands of headlines is slow enough to count.
/// `TokenAtOffset::None` for an offset outside the node.
pub fn token_at_offset(node: &SyntaxNode, offset: TextSize) -> TokenAtOffset<SyntaxToken> {
    let range = node.text_range();
    if range.is_empty() || offset < range.start() || offset > range.end() {
        return TokenAtOffset::None;
    }
    // The children that hold the byte before the offset and the byte at it.
    let before = (offset > range.start())
        .then(|| node.child_or_token_at_range(TextRange::new(offset - TextSize::from(1), offset)))
        .flatten();
    let at = (offset < range.end()).then(|| node.child_or_token_at_range(TextRange::at(offset, 1.into()))).flatten();
    let down = |element: SyntaxElement| match element {
        rowan::NodeOrToken::Token(token) => TokenAtOffset::Single(token),
        rowan::NodeOrToken::Node(node) => token_at_offset(&node, offset),
    };
    match (before, at) {
        (Some(before), Some(at)) if before == at => down(at),
        (Some(before), Some(at)) => match (down(before), down(at)) {
            (TokenAtOffset::Single(left), TokenAtOffset::Single(right)) => TokenAtOffset::Between(left, right),
            (_, right) => right,
        },
        (Some(only), None) | (None, Some(only)) => down(only),
        (None, None) => TokenAtOffset::None,
    }
}

/// The result of parsing a document.
#[derive(Debug, Clone)]
pub struct Parse {
    green: rowan::GreenNode,
    todo: TodoKeywords,
    /// Whether `todo` are the defaults, the document declaring none.
    default: bool,
}

impl Parse {
    /// The root `DOCUMENT` node.
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    /// The TODO keywords this document was parsed with: the ones it declares
    /// itself, or else the defaults passed to [`parse`].
    pub fn todo_keywords(&self) -> &TodoKeywords {
        &self.todo
    }
}

/// The objects of `text` as in a paragraph (emphasis, links, entities…),
/// whatever the text would be at the start of a line: a `PARAGRAPH` node
/// of object nodes and `TEXT` tokens.
pub fn parse_objects(text: &str) -> SyntaxNode {
    let mut builder = rowan::GreenNodeBuilder::new();
    builder.start_node(SyntaxKind::PARAGRAPH.into());
    objects::parse_objects(&mut builder, text, objects::Mode::STANDARD);
    builder.finish_node();
    SyntaxNode::new_root(builder.finish())
}

/// Parse `text`. `default_todo` applies unless the document declares its own
/// keywords with `#+TODO:` lines.
pub fn parse(text: &str, default_todo: &TodoKeywords) -> Parse {
    let declared = TodoKeywords::from_document(text);
    let default = declared.is_none();
    let todo = declared.unwrap_or_else(|| default_todo.clone());
    let green = parser::parse(text, &todo);
    Parse { green, todo, default }
}

impl Parse {
    /// The parse of `new`, which is `old` (the text this was parsed from)
    /// with the bytes from `start` to `end` replaced. A headline's line and
    /// its section are parsed from its own lines alone, and the outline
    /// from the levels of the headlines: when the change stays within one
    /// headline's lines (or the lines before the first headline), keeps
    /// them so and declares no TODO keywords, those lines alone are parsed
    /// again and take the place of the old ones in the tree. Otherwise the
    /// whole of `new` is parsed. Either way the tree is the one [`parse`]
    /// gives.
    pub fn edit(&self, old: &str, start: usize, end: usize, new: &str, default_todo: &TodoKeywords) -> Parse {
        // Other default keywords than the ones it was parsed with.
        if self.default && self.todo != *default_todo {
            return parse(new, default_todo);
        }
        self.edit_in_place(old, start, end, new).unwrap_or_else(|| parse(new, default_todo))
    }

    fn edit_in_place(&self, old: &str, start: usize, end: usize, new: &str) -> Option<Parse> {
        if start > end || end > old.len() || !old.is_char_boundary(start) || !old.is_char_boundary(end) {
            return None;
        }
        let shift = new.len() as isize - old.len() as isize;
        // The lines of the change, out to the headline lines around them.
        let line_start = |at: usize| old[..at].rfind('\n').map_or(0, |newline| newline + 1);
        let mut from = line_start(start);
        while from > 0 && headline_level(line_of(old, from)).is_none() {
            from = line_start(from - 1);
        }
        let mut to = old[end..].find('\n').map_or(old.len(), |newline| end + newline + 1);
        while to < old.len() && headline_level(line_of(old, to)).is_none() {
            to = old[to..].find('\n').map_or(old.len(), |newline| to + newline + 1);
        }
        let new_to = usize::try_from(to as isize + shift).ok()?;
        if new_to < from || new_to > new.len() || !new.is_char_boundary(new_to) {
            return None;
        }
        let (before, after) = (&old[from..to], &new[from..new_to]);
        // The same headline lines, the one they start with alone: the
        // outline around them stays as it was.
        let level = headline_level(line_of(old, from));
        if headline_levels(before) != headline_levels(after)
            || headline_levels(before).len() > usize::from(level.is_some())
        {
            return None;
        }
        if level.is_some() && headline_level(line_of(after, 0)) != level {
            return None;
        }
        if declares_todo(before) || declares_todo(after) {
            return None;
        }
        let parsed = parser::parse(after, &self.todo);
        let root = self.syntax();
        let green = match level {
            // The headline that starts at `from`: its line and section
            // anew, its subheadlines as they were.
            Some(_) => {
                let headline =
                    root.token_at_offset(TextSize::new(from as u32)).right_biased()?.parent_ancestors().find(
                        |node| node.kind() == SyntaxKind::HEADLINE && usize::from(node.text_range().start()) == from,
                    )?;
                let fresh = parsed.children().find(|child| child.kind() == SyntaxKind::HEADLINE.into())?;
                let fresh = fresh.into_node()?;
                let children = fresh
                    .children()
                    .map(|child| child.to_owned())
                    .chain(headline.children_with_tokens().filter(|child| child.kind() == SyntaxKind::HEADLINE).map(
                        |child| match child {
                            rowan::NodeOrToken::Node(node) => rowan::NodeOrToken::Node(node.green().into_owned()),
                            rowan::NodeOrToken::Token(token) => rowan::NodeOrToken::Token(token.green().to_owned()),
                        },
                    ))
                    .collect::<Vec<_>>();
                headline.replace_with(rowan::GreenNode::new(SyntaxKind::HEADLINE.into(), children))
            }
            // The lines before the first headline: the document's section
            // anew, its headlines as they were.
            None => {
                let children = parsed.children().map(|child| child.to_owned()).chain(
                    root.children()
                        .filter(|child| child.kind() == SyntaxKind::HEADLINE)
                        .map(|child| rowan::NodeOrToken::Node(child.green().into_owned())),
                );
                rowan::GreenNode::new(SyntaxKind::DOCUMENT.into(), children.collect::<Vec<_>>())
            }
        };
        Some(Parse { green, todo: self.todo.clone(), default: self.default })
    }
}

/// The line of `text` that starts at `at`.
fn line_of(text: &str, at: usize) -> &str {
    let rest = &text[at..];
    &rest[..rest.find('\n').unwrap_or(rest.len())]
}

/// The levels of the headline lines of `text`, in order.
fn headline_levels(text: &str) -> Vec<usize> {
    text.split('\n').filter_map(headline_level).collect()
}

/// Whether a line of `text` declares TODO keywords, as
/// [`TodoKeywords::from_document`] reads them.
fn declares_todo(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("#+")
            && line.find(':').is_some_and(|colon| {
                ["TODO", "SEQ_TODO", "TYP_TODO"].iter().any(|key| line[2..colon].eq_ignore_ascii_case(key))
            })
    })
}

#[cfg(test)]
mod edit_tests {
    use super::*;

    /// A small, fixed sequence of numbers, for edits that are the same on
    /// every run.
    struct Numbers(u64);

    impl Numbers {
        fn below(&mut self, limit: usize) -> usize {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as usize) % limit.max(1)
        }
    }

    const INSERTS: &[&str] = &[
        "a",
        " ",
        "\n",
        "* ",
        "** ",
        "*",
        "\n* New\n",
        "\n** Sub\n",
        "#+TODO: A | B\n",
        "#+begin_src emacs-lisp\n",
        "#+end_src\n",
        ":PROPERTIES:\n",
        ":END:\n",
        ":LOGBOOK:\n",
        "- ",
        "  - [ ] x\n",
        "| x | y |\n",
        "|-\n",
        "SCHEDULED: <2026-01-01 Thu>\n",
        "CLOSED: [2026-01-01 Thu 10:00]",
        "[[",
        "]]",
        "\\begin{equation}\n",
        "\\end{equation}\n",
        "#+begin_quote\n",
        "#+end_quote\n",
        "#+NAME: x\n",
        "*bold*",
        "TODO ",
        "DONE ",
        "[#A] ",
        ":tag:",
        "\r\n",
        "é",
        "😀",
        "",
        "#+STARTUP: indent\n",
        "<<<radio>>>",
        "[fn:1]",
        "#+begin:\n",
        "#+end:\n",
        "*************** Inline\n",
        "*************** END\n",
    ];

    fn char_boundary(text: &str, mut at: usize) -> usize {
        at = at.min(text.len());
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        at
    }

    /// Other default TODO keywords than the parse had: the whole text is
    /// parsed with them.
    #[test]
    fn an_edit_with_other_default_keywords_parses_the_whole_text() {
        let old = "* WAIT a\n* b\n";
        let new = "* WAIT a\n* bc\n";
        let wait: TodoKeywords = TodoKeywords::from_document("#+TODO: WAIT | DONE\n").unwrap();
        let edited = parse(old, &TodoKeywords::default()).edit(old, 13, 13, new, &wait);
        assert_eq!(edited.green, parse(new, &wait).green);
        // With keywords of its own, the file keeps them.
        let declared = "#+TODO: WAIT\n* WAIT a\n";
        let edited =
            parse(declared, &TodoKeywords::default()).edit(declared, 21, 21, "#+TODO: WAIT\n* WAIT ab\n", &wait);
        assert_eq!(edited.todo, parse("#+TODO: WAIT\n", &wait).todo);
    }

    /// Edits anywhere in the repository's Org files, each parsed by
    /// [`Parse::edit`] and by [`parse`]: the same tree every time, and
    /// most of them without parsing the whole text.
    #[test]
    fn an_edit_parses_as_the_whole_text_does() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut files: Vec<std::path::PathBuf> = ["tests/fixtures", "docs", "tests"]
            .iter()
            .filter_map(|dir| std::fs::read_dir(root.join(dir)).ok())
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "org"))
            .collect();
        files.sort();
        assert!(files.len() > 5, "{files:?}");
        let todo = TodoKeywords::default();
        // `ORG_SYNTAX_EDITS` edits a file (60), from `ORG_SYNTAX_SEED` (7).
        let count = std::env::var("ORG_SYNTAX_EDITS").ok().and_then(|count| count.parse().ok()).unwrap_or(60);
        let seed = std::env::var("ORG_SYNTAX_SEED").ok().and_then(|seed| seed.parse().ok()).unwrap_or(7);
        let mut numbers = Numbers(seed);
        let (mut edits, mut in_place) = (0, 0);
        for file in files {
            let mut text = std::fs::read_to_string(&file).unwrap();
            // Long files cut short, so that every file has its turn.
            text.truncate(char_boundary(&text, 40_000));
            let mut parsed = parse(&text, &todo);
            for _ in 0..count {
                let start = char_boundary(&text, numbers.below(text.len() + 1));
                let end = char_boundary(&text, start + numbers.below(12));
                let end = end.max(start);
                let insert = INSERTS[numbers.below(INSERTS.len())];
                let mut new = text.clone();
                new.replace_range(start..end, insert);
                let edited = parsed.edit(&text, start, end, &new, &todo);
                let whole = parse(&new, &todo);
                assert_eq!(
                    edited.green,
                    whole.green,
                    "{}: {:?} at {start}..{end}, in {:?}",
                    file.display(),
                    insert,
                    &text[text[..start].rfind('\n').map_or(0, |at| at + 1)..end]
                );
                assert_eq!(edited.todo, whole.todo);
                in_place += usize::from(parsed.edit_in_place(&text, start, end, &new).is_some());
                edits += 1;
                text = new;
                parsed = edited;
            }
        }
        eprintln!("{in_place} of {edits} edits parsed in place");
        assert!(in_place * 2 > edits, "only {in_place} of {edits} edits parsed in place");
    }
}
