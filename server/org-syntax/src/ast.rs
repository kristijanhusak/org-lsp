//! Typed views over the syntax tree.

use rowan::TextRange;

use crate::kind::SyntaxKind::*;
use crate::kind::{SyntaxNode, SyntaxToken};

/// A headline together with its section and child headlines.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Headline(SyntaxNode);

impl Headline {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        (node.kind() == HEADLINE).then_some(Headline(node))
    }

    pub fn syntax(&self) -> &SyntaxNode {
        &self.0
    }

    fn token(&self, kind: crate::SyntaxKind) -> Option<SyntaxToken> {
        self.0.children_with_tokens().filter_map(|child| child.into_token()).find(|token| token.kind() == kind)
    }

    pub fn stars(&self) -> SyntaxToken {
        self.token(STARS).expect("a headline starts with stars")
    }

    /// Number of stars.
    pub fn level(&self) -> usize {
        self.stars().text().len()
    }

    pub fn todo_keyword(&self) -> Option<SyntaxToken> {
        self.token(TODO_KEYWORD)
    }

    pub fn priority(&self) -> Option<SyntaxToken> {
        self.token(PRIORITY)
    }

    pub fn title(&self) -> Option<SyntaxNode> {
        self.0.children().find(|child| child.kind() == TITLE)
    }

    /// The title as written, without keyword, priority or tags.
    pub fn title_text(&self) -> String {
        self.title().map(|title| title.text().to_string()).unwrap_or_default()
    }

    pub fn tags(&self) -> Vec<String> {
        self.0
            .children()
            .find(|child| child.kind() == TAGS)
            .into_iter()
            .flat_map(|tags| tags.children_with_tokens())
            .filter_map(|child| child.into_token())
            .filter(|token| token.kind() == TAG)
            .map(|token| token.text().to_string())
            .collect()
    }

    pub fn section(&self) -> Option<SyntaxNode> {
        self.0.children().find(|child| child.kind() == SECTION)
    }

    pub fn children(&self) -> impl Iterator<Item = Headline> {
        self.0.children().filter_map(Headline::cast)
    }

    /// The headline line itself, without its line terminator.
    pub fn line_range(&self) -> TextRange {
        let start = self.0.text_range().start();
        let end = self
            .0
            .children_with_tokens()
            .take_while(|child| !matches!(child.kind(), SECTION | HEADLINE))
            .filter(|child| child.kind() != NEWLINE)
            .last()
            .map_or(start, |child| child.text_range().end());
        TextRange::new(start, end)
    }
}

/// Top-level headlines of a document.
pub fn headlines(root: &SyntaxNode) -> impl Iterator<Item = Headline> {
    root.children().filter_map(Headline::cast)
}

/// The innermost headline whose subtree contains `offset`.
pub fn headline_at(root: &SyntaxNode, offset: rowan::TextSize) -> Option<Headline> {
    let mut best = None;
    let mut candidates: Vec<Headline> = headlines(root).collect();
    while let Some(index) = candidates.iter().position(|headline| {
        let range = headline.syntax().text_range();
        range.contains(offset) || (range.end() == offset && offset == root.text_range().end())
    }) {
        let headline = candidates.swap_remove(index);
        candidates = headline.children().collect();
        best = Some(headline);
    }
    best
}

/// Whether a `KEYWORD` node is an affiliated keyword (`#+NAME:`, `#+CAPTION:`
/// …) attached to its parent element, rather than an element of its own.
/// Affiliated keywords are the leading children of the element they belong to.
pub fn is_affiliated_keyword(node: &SyntaxNode) -> bool {
    node.kind() == KEYWORD
        && node.parent().is_some_and(|parent| parent.kind() != SECTION)
        && std::iter::successors(node.prev_sibling_or_token(), |previous| previous.prev_sibling_or_token())
            .all(|previous| previous.kind() == KEYWORD)
}
