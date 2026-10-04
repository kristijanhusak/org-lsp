//! Document symbols: the outline of headlines.

use lsp_types::{DocumentSymbol, Location, SymbolInformation, SymbolKind, Uri};
use org_syntax::{ast, ast::Headline, SyntaxNode, TextRange, TextSize};

use crate::line_index::{Encoding, LineIndex};

/// Headlines have no natural symbol kind; `String` is what editors' own
/// Markdown outlines use.
const KIND: SymbolKind = SymbolKind::STRING;

/// The outline as a tree, for clients with hierarchical symbol support.
pub fn document_symbols(root: &SyntaxNode, text: &str, index: &LineIndex, encoding: Encoding) -> Vec<DocumentSymbol> {
    ast::headlines(root).map(|headline| symbol(&headline, text, index, encoding)).collect()
}

/// The outline as a flat list, for clients without it.
pub fn symbol_information(
    root: &SyntaxNode,
    text: &str,
    index: &LineIndex,
    encoding: Encoding,
    uri: &Uri,
) -> Vec<SymbolInformation> {
    fn collect(
        headline: &Headline,
        parent: Option<&str>,
        context: (&str, &LineIndex, Encoding, &Uri),
        out: &mut Vec<SymbolInformation>,
    ) {
        let (text, index, encoding, uri) = context;
        let name = name(headline);
        #[allow(deprecated)]
        out.push(SymbolInformation {
            name: name.clone(),
            kind: KIND,
            tags: None,
            deprecated: None,
            location: Location { uri: uri.clone(), range: index.range(text, subtree_range(headline, text), encoding) },
            container_name: parent.map(str::to_string),
        });
        for child in headline.children() {
            collect(&child, Some(&name), context, out);
        }
    }
    let mut out = Vec::new();
    for headline in ast::headlines(root) {
        collect(&headline, None, (text, index, encoding, uri), &mut out);
    }
    out
}

fn symbol(headline: &Headline, text: &str, index: &LineIndex, encoding: Encoding) -> DocumentSymbol {
    let children: Vec<DocumentSymbol> =
        headline.children().map(|child| symbol(&child, text, index, encoding)).collect();
    let selection = headline.title().map_or_else(|| headline.line_range(), |title| title.text_range());
    #[allow(deprecated)]
    DocumentSymbol {
        name: name(headline),
        detail: detail(headline),
        kind: KIND,
        tags: None,
        deprecated: None,
        range: index.range(text, subtree_range(headline, text), encoding),
        selection_range: index.range(text, selection, encoding),
        children: (!children.is_empty()).then_some(children),
    }
}

/// The title, or the whole headline line when there is no title. Symbol
/// names must not be empty.
fn name(headline: &Headline) -> String {
    let title = headline.title_text();
    let title = title.trim();
    if !title.is_empty() {
        return title.to_string();
    }
    let line = headline.syntax().text().slice(headline.line_range() - headline.syntax().text_range().start());
    line.to_string().trim().to_string()
}

/// The TODO keyword and tags, shown next to the name by most editors.
fn detail(headline: &Headline) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(keyword) = headline.todo_keyword() {
        parts.push(keyword.text().to_string());
    }
    let tags = headline.tags();
    if !tags.is_empty() {
        parts.push(format!(":{}:", tags.join(":")));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// The subtree without its trailing blank lines.
fn subtree_range(headline: &Headline, text: &str) -> TextRange {
    let range = headline.syntax().text_range();
    let (start, end) = (usize::from(range.start()), usize::from(range.end()));
    let trimmed = start + text[start..end].trim_end().len();
    TextRange::new(range.start(), TextSize::new(trimmed as u32))
}

#[cfg(test)]
mod tests {
    use org_syntax::TodoKeywords;

    use super::*;

    fn outline(symbols: &[DocumentSymbol], depth: usize, out: &mut String) {
        for symbol in symbols {
            let range = (symbol.range.start.line, symbol.range.end.line);
            let detail = symbol.detail.as_deref().unwrap_or("-");
            out.push_str(&format!(
                "{}{} [{}] lines {}..{}\n",
                "  ".repeat(depth),
                symbol.name,
                detail,
                range.0,
                range.1
            ));
            outline(symbol.children.as_deref().unwrap_or(&[]), depth + 1, out);
        }
    }

    const TEXT: &str = "\
intro
* TODO [#A] First *bold* :a:b:
body

** Child
*** DONE Grandchild
* 
* TODO
* :only:tags:
";

    #[test]
    fn hierarchy_names_and_details() {
        let parse = org_syntax::parse(TEXT, &TodoKeywords::default());
        let index = LineIndex::new(TEXT);
        let symbols = document_symbols(&parse.syntax(), TEXT, &index, Encoding::Utf16);
        let mut rendered = String::new();
        outline(&symbols, 0, &mut rendered);
        assert_eq!(
            rendered,
            "\
First *bold* [TODO :a:b:] lines 1..5
  Child [-] lines 4..5
    Grandchild [DONE] lines 5..5
* [-] lines 6..6
* TODO [TODO] lines 7..7
* :only:tags: [:only:tags:] lines 8..8
"
        );
        // The selection is the title and lies inside the symbol's range.
        let first = &symbols[0];
        assert_eq!((first.selection_range.start.character, first.selection_range.end.character), (12, 24));
    }

    #[test]
    fn flat_list_names_containers() {
        let parse = org_syntax::parse(TEXT, &TodoKeywords::default());
        let index = LineIndex::new(TEXT);
        let uri: Uri = "file:///tmp/a.org".parse().unwrap();
        let flat = symbol_information(&parse.syntax(), TEXT, &index, Encoding::Utf16, &uri);
        let pairs: Vec<(&str, Option<&str>)> =
            flat.iter().map(|symbol| (symbol.name.as_str(), symbol.container_name.as_deref())).collect();
        assert_eq!(
            pairs,
            [
                ("First *bold*", None),
                ("Child", Some("First *bold*")),
                ("Grandchild", Some("Child")),
                ("*", None),
                ("* TODO", None),
                ("* :only:tags:", None),
            ]
        );
    }
}
