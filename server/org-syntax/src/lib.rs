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
    let todo = TodoKeywords::from_document(text).unwrap_or_else(|| default_todo.clone());
    let green = parser::parse(text, &todo);
    Parse { green, todo }
}
