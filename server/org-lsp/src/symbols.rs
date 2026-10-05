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
    let range = subtree_range(headline, text);
    let selection = headline.title().map_or_else(|| headline.line_range(), |title| title.text_range());
    #[allow(deprecated)]
    DocumentSymbol {
        name: name(headline),
        detail: detail(headline),
        kind: KIND,
        tags: None,
        deprecated: None,
        range: index.range(text, range, encoding),
        selection_range: index.range(text, trim_end(selection, text).intersect(range).unwrap_or(range), encoding),
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
    trim_end(headline.syntax().text_range(), text)
}

/// `range` without the blanks it ends in. VS Code refuses a selection
/// that is not inside the symbol's range, so the selection, which can end
/// in the blanks just typed, is trimmed as the range is.
fn trim_end(range: TextRange, text: &str) -> TextRange {
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

    fn contains(outer: &lsp_types::Range, inner: &lsp_types::Range) -> bool {
        let key = |p: &lsp_types::Position| (p.line, p.character);
        key(&outer.start) <= key(&inner.start) && key(&inner.end) <= key(&outer.end)
    }

    fn check(symbols: &[DocumentSymbol], text: &str) {
        for symbol in symbols {
            assert!(contains(&symbol.range, &symbol.selection_range), "{symbol:?} in {text:?}");
            let children = symbol.children.as_deref().unwrap_or(&[]);
            for child in children {
                assert!(contains(&symbol.range, &child.range), "{child:?} in {text:?}");
            }
            check(children, text);
        }
    }

    /// VS Code refuses a symbol whose selection is not inside its range,
    /// so check it for every text met while typing a document.
    #[test]
    fn selection_lies_inside_the_range_while_typing() {
        let documents = [
            "* foo \n",
            "* TODO   \n** \t\n*** DONE x :a: \n",
            "* a\n\n  \n* b  \n  \n",
            "* TODO [#A] title  :tag:   \nbody   \n** child\t\n",
            "* \u{00e9}t\u{00e9} \u{1F600} \n",
            "* a\r\n** b \r\n",
        ];
        for document in documents {
            for (end, _) in document.char_indices().chain([(document.len(), ' ')]) {
                let text = &document[..end];
                let parse = org_syntax::parse(text, &TodoKeywords::default());
                let index = LineIndex::new(text);
                for encoding in [Encoding::Utf8, Encoding::Utf16] {
                    check(&document_symbols(&parse.syntax(), text, &index, encoding), text);
                }
            }
        }
    }
}
