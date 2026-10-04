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
mod objects;
mod parser;

pub use config::TodoKeywords;
pub use kind::{OrgLanguage, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};
pub use rowan::{TextRange, TextSize, TokenAtOffset};

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

/// Parse `text`. `default_todo` applies unless the document declares its own
/// keywords with `#+TODO:` lines.
pub fn parse(text: &str, default_todo: &TodoKeywords) -> Parse {
    let todo = TodoKeywords::from_document(text).unwrap_or_else(|| default_todo.clone());
    let green = parser::parse(text, &todo);
    Parse { green, todo }
}
