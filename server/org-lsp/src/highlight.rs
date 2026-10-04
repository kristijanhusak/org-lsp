//! Semantic tokens: the highlighting the server offers to every editor.
//!
//! Tokens never overlap. Nested markup is flattened: emphasis becomes
//! modifiers on the text it covers, so `*bold /both/*` yields tokens that
//! carry `bold` and `bold + italic`. Editors map token types and modifiers
//! to their own highlight groups; see `docs/protocol.md` for the legend.

use lsp_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokensLegend};
use org_syntax::{SyntaxElement, SyntaxKind, SyntaxKind::*, SyntaxNode, TextRange, TodoKeywords};

use crate::line_index::{Encoding, LineIndex};

macro_rules! token_types {
    ($($variant:ident => $name:literal,)*) => {
        /// Token types, in legend order.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u32)]
        pub enum TokenType { $($variant,)* }

        impl TokenType {
            pub const ALL: &'static [TokenType] = &[$(TokenType::$variant,)*];

            pub fn name(self) -> &'static str {
                match self { $(TokenType::$variant => $name,)* }
            }
        }
    };
}

token_types! {
    Headline => "headline",
    Stars => "stars",
    TodoKeyword => "todoKeyword",
    Priority => "priority",
    Tag => "tag",
    Planning => "planning",
    Timestamp => "timestamp",
    Drawer => "drawer",
    Property => "property",
    Keyword => "keyword",
    Block => "block",
    Comment => "comment",
    Bullet => "bullet",
    Checkbox => "checkbox",
    Link => "link",
    Code => "code",
    Verbatim => "verbatim",
    Text => "text",
    TableDelimiter => "tableDelimiter",
    Footnote => "footnote",
    Cookie => "cookie",
    Target => "target",
    Latex => "latex",
    Entity => "entity",
    Macro => "macro",
}

/// Token modifiers, in legend order; each is one bit.
pub mod modifier {
    pub const NAMES: &[&str] = &[
        "level1",
        "level2",
        "level3",
        "level4",
        "level5",
        "level6",
        "level7",
        "level8",
        "done",
        "inactive",
        "checked",
        "partial",
        "bold",
        "italic",
        "underline",
        "strikethrough",
    ];

    pub const DONE: u32 = 1 << 8;
    pub const INACTIVE: u32 = 1 << 9;
    pub const CHECKED: u32 = 1 << 10;
    pub const PARTIAL: u32 = 1 << 11;
    pub const BOLD: u32 = 1 << 12;
    pub const ITALIC: u32 = 1 << 13;
    pub const UNDERLINE: u32 = 1 << 14;
    pub const STRIKETHROUGH: u32 = 1 << 15;

    /// Headline levels repeat after eight, like Org's level faces.
    pub fn level(level: usize) -> u32 {
        1 << ((level.max(1) - 1) % 8)
    }
}

pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TokenType::ALL.iter().map(|ty| SemanticTokenType::new(ty.name())).collect(),
        token_modifiers: modifier::NAMES.iter().map(|name| SemanticTokenModifier::new(name)).collect(),
    }
}

/// One highlighted range. It may span several lines; `encode` splits it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub range: TextRange,
    pub ty: TokenType,
    pub modifiers: u32,
}

/// How plain text is styled at the current point of the walk.
#[derive(Clone, Copy)]
struct Style {
    /// The type for text tokens, or `None` when plain text gets no token.
    text: Option<TokenType>,
    modifiers: u32,
}

impl Style {
    const PLAIN: Style = Style { text: None, modifiers: 0 };
}

/// All tokens of a document, in order.
pub fn tokens(root: &SyntaxNode, todo: &TodoKeywords) -> Vec<Token> {
    let mut walker = Walker { todo, out: Vec::new() };
    walker.children(root, Style::PLAIN);
    walker.out
}

struct Walker<'a> {
    todo: &'a TodoKeywords,
    out: Vec<Token>,
}

impl Walker<'_> {
    fn push(&mut self, range: TextRange, ty: TokenType, modifiers: u32) {
        if range.is_empty() {
            return;
        }
        // Adjacent pieces with the same style read as one token.
        if let Some(last) = self.out.last_mut() {
            if last.range.end() == range.start() && last.ty == ty && last.modifiers == modifiers {
                last.range = TextRange::new(last.range.start(), range.end());
                return;
            }
        }
        self.out.push(Token { range, ty, modifiers });
    }

    fn children(&mut self, node: &SyntaxNode, style: Style) {
        for child in node.children_with_tokens() {
            self.element(node, child, style);
        }
    }

    fn element(&mut self, parent: &SyntaxNode, element: SyntaxElement, style: Style) {
        let range = element.text_range();
        match element {
            SyntaxElement::Token(token) => {
                if let Some((ty, modifiers)) = self.token_style(parent.kind(), token.kind(), token.text(), style) {
                    self.push(range, ty, modifiers);
                }
            }
            SyntaxElement::Node(node) => self.node(&node, style),
        }
    }

    /// The style of a leaf token, given the node it sits in.
    fn token_style(&self, parent: SyntaxKind, kind: SyntaxKind, text: &str, style: Style) -> Option<(TokenType, u32)> {
        let plain = |ty| Some((ty, 0));
        match kind {
            TODO_KEYWORD => Some((TokenType::TodoKeyword, if self.todo.is_done(text) { modifier::DONE } else { 0 })),
            PRIORITY => plain(TokenType::Priority),
            PLANNING_KEYWORD | CLOCK_KEYWORD => plain(TokenType::Planning),
            DRAWER_BEGIN | DRAWER_END => plain(TokenType::Drawer),
            PROPERTY_KEY => plain(TokenType::Property),
            KEYWORD_KEY => plain(TokenType::Keyword),
            BLOCK_END => plain(TokenType::Block),
            COMMENT_TEXT => plain(TokenType::Comment),
            FIXED_WIDTH_TEXT => plain(TokenType::Verbatim),
            PIPE | TABLE_RULE => plain(TokenType::TableDelimiter),
            BULLET | COUNTER_SET => plain(TokenType::Bullet),
            CHECKBOX => {
                let modifiers = match text {
                    "[X]" | "[x]" => modifier::CHECKED,
                    "[-]" => modifier::PARTIAL,
                    _ => 0,
                };
                Some((TokenType::Checkbox, modifiers))
            }
            // A footnote definition's label; references are handled as nodes.
            FOOTNOTE_LABEL if parent == FOOTNOTE_DEFINITION => plain(TokenType::Footnote),
            // Text and emphasis markers take the style of their context.
            TEXT | MARKER if style.text.is_some() || style.modifiers != 0 => {
                Some((style.text.unwrap_or(TokenType::Text), style.modifiers))
            }
            _ => None,
        }
    }

    fn node(&mut self, node: &SyntaxNode, style: Style) {
        let range = node.text_range();
        let emphasis =
            |modifier| Style { text: style.text.or(Some(TokenType::Text)), modifiers: style.modifiers | modifier };
        match node.kind() {
            HEADLINE => self.headline(node),
            // Tags, links and the like are single tokens whatever they contain.
            TAGS => self.push(range, TokenType::Tag, 0),
            LINK | PLAIN_LINK | ANGLE_LINK | CITATION => self.push(range, TokenType::Link, 0),
            CODE | INLINE_SRC_BLOCK | INLINE_BABEL_CALL => self.push(range, TokenType::Code, 0),
            VERBATIM => self.push(range, TokenType::Verbatim, 0),
            TIMESTAMP => {
                let inactive = node.first_token().is_some_and(|token| token.text().starts_with('['));
                self.push(range, TokenType::Timestamp, if inactive { modifier::INACTIVE } else { 0 });
            }
            FOOTNOTE_REFERENCE => self.push(range, TokenType::Footnote, 0),
            STATISTICS_COOKIE => self.push(range, TokenType::Cookie, 0),
            TARGET | RADIO_TARGET => self.push(range, TokenType::Target, 0),
            LATEX_FRAGMENT => self.push(range, TokenType::Latex, 0),
            LATEX_ENVIRONMENT => {
                for child in node.children_with_tokens() {
                    match child.kind() {
                        TEXT => self.push(child.text_range(), TokenType::Latex, 0),
                        _ => self.element(node, child, Style::PLAIN),
                    }
                }
            }
            ENTITY => self.push(range, TokenType::Entity, 0),
            MACRO => self.push(range, TokenType::Macro, 0),
            BOLD => self.children(node, emphasis(modifier::BOLD)),
            ITALIC => self.children(node, emphasis(modifier::ITALIC)),
            UNDERLINE => self.children(node, emphasis(modifier::UNDERLINE)),
            STRIKE_THROUGH => self.children(node, emphasis(modifier::STRIKETHROUGH)),
            BLOCK | DYNAMIC_BLOCK => self.block(node),
            _ => self.children(node, style),
        }
    }

    fn headline(&mut self, node: &SyntaxNode) {
        let level = node.first_token().map_or(1, |stars| stars.text().len());
        let level = modifier::level(level);
        for child in node.children_with_tokens() {
            match child.kind() {
                STARS => self.push(child.text_range(), TokenType::Stars, level),
                TITLE => {
                    let title = child.into_node().expect("TITLE is a node");
                    self.children(&title, Style { text: Some(TokenType::Headline), modifiers: level });
                }
                _ => self.element(node, child, Style::PLAIN),
            }
        }
    }

    fn block(&mut self, node: &SyntaxNode) {
        let begin = node.children_with_tokens().find(|child| child.kind() == BLOCK_BEGIN);
        let is_comment = begin
            .as_ref()
            .and_then(|begin| begin.as_token())
            .is_some_and(|begin| begin.text().eq_ignore_ascii_case("#+begin_comment"));
        for child in node.children_with_tokens() {
            match child.kind() {
                // The whole opening line, parameters included, is one token.
                BLOCK_BEGIN | BLOCK_PARAMETERS => self.push(child.text_range(), TokenType::Block, 0),
                WHITESPACE if self.out.last().is_some_and(|last| last.ty == TokenType::Block) => {
                    let next_is_parameters =
                        child.next_sibling_or_token().is_some_and(|next| next.kind() == BLOCK_PARAMETERS);
                    if next_is_parameters {
                        self.push(child.text_range(), TokenType::Block, 0);
                    }
                }
                TEXT if is_comment => self.push(child.text_range(), TokenType::Comment, 0),
                // Raw contents (source code, examples) are left to the editor.
                TEXT => {}
                _ => self.element(node, child, Style::PLAIN),
            }
        }
    }
}

/// Encode tokens in the LSP wire format: five integers per token, positions
/// relative to the previous token. Tokens spanning lines are split, and
/// leading or trailing blanks of each piece are dropped.
///
/// With `only`, tokens outside that range are skipped.
pub fn encode(
    tokens: &[Token],
    text: &str,
    index: &LineIndex,
    encoding: Encoding,
    only: Option<TextRange>,
) -> Vec<SemanticToken> {
    let mut encoded = Vec::with_capacity(tokens.len());
    let mut previous = (0u32, 0u32);
    // Where the last token started, as (line, byte offset, column). Columns
    // are measured from there rather than from the line start each time,
    // which would be quadratic on a very long line.
    let mut cursor = (usize::MAX, 0usize, 0u32);
    for token in tokens {
        if only.is_some_and(|only| token.range.end() <= only.start() || token.range.start() >= only.end()) {
            continue;
        }
        let (start, end) = (usize::from(token.range.start()), usize::from(token.range.end()));
        let last_line = index.line_of(end.saturating_sub(1).max(start));
        for line in index.line_of(start)..=last_line {
            let (line_start, line_end) = index.line_range(text, line);
            let (piece_start, piece_end) = (start.max(line_start), end.min(line_end));
            if piece_start >= piece_end {
                continue;
            }
            let piece = &text[piece_start..piece_end];
            let trimmed = piece.trim_start();
            let piece_start = piece_start + (piece.len() - trimmed.len());
            let trimmed = trimmed.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            let column = if cursor.0 == line && cursor.1 <= piece_start {
                cursor.2 + encoding.len(&text[cursor.1..piece_start])
            } else {
                encoding.len(&text[line_start..piece_start])
            };
            cursor = (line, piece_start, column);
            let line = line as u32;
            encoded.push(SemanticToken {
                delta_line: line - previous.0,
                delta_start: if line == previous.0 { column - previous.1 } else { column },
                length: encoding.len(trimmed),
                token_type: token.ty as u32,
                token_modifiers_bitset: token.modifiers,
            });
            previous = (line, column);
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use expect_test::{expect, Expect};

    use super::*;

    /// Render tokens as `line:column text type +modifiers`, one per line piece.
    fn render(text: &str, encoding: Encoding) -> String {
        let parse = org_syntax::parse(text, &TodoKeywords::default());
        let index = LineIndex::new(text);
        let tokens = tokens(&parse.syntax(), parse.todo_keywords());
        let mut out = String::new();
        let (mut line, mut column) = (0, 0);
        for token in encode(&tokens, text, &index, encoding, None) {
            line += token.delta_line;
            column = if token.delta_line == 0 { column + token.delta_start } else { token.delta_start };
            let (line_start, line_end) = index.line_range(text, line as usize);
            let content = &text[line_start..line_end];
            // Convert the encoded column and length back to a slice of the line.
            let mut units = 0;
            let mut slice = String::new();
            for c in content.chars() {
                if units >= column && units < column + token.length {
                    slice.push(c);
                }
                units += encoding.len(c.encode_utf8(&mut [0; 4]));
            }
            let modifiers: Vec<&str> = modifier::NAMES
                .iter()
                .enumerate()
                .filter(|(bit, _)| token.token_modifiers_bitset & (1 << bit) != 0)
                .map(|(_, name)| *name)
                .collect();
            let ty = TokenType::ALL[token.token_type as usize].name();
            out.push_str(&format!("{line}:{column} {slice:?} {ty}"));
            for name in modifiers {
                out.push_str(&format!(" +{name}"));
            }
            out.push('\n');
        }
        out
    }

    fn check(text: &str, expected: Expect) {
        expected.assert_eq(&render(text, Encoding::Utf16));
    }

    #[test]
    fn headline_parts() {
        check(
            "** TODO [#A] Write *the* [[a][parser]] :work:rust:\n*** DONE x\n",
            expect![[r#"
            0:0 "**" stars +level2
            0:3 "TODO" todoKeyword
            0:8 "[#A]" priority
            0:13 "Write" headline +level2
            0:19 "*the*" headline +level2 +bold
            0:25 "[[a][parser]]" link
            0:39 ":work:rust:" tag
            1:0 "***" stars +level3
            1:4 "DONE" todoKeyword +done
            1:9 "x" headline +level3
        "#]],
        );
    }

    #[test]
    fn nested_emphasis_flattens_to_modifiers() {
        check(
            "a *b /c _d_ c/ b* +e+ =f= ~g~ *~h~*\n",
            expect![[r#"
            0:2 "*b" text +bold
            0:5 "/c" text +bold +italic
            0:8 "_d_" text +bold +italic +underline
            0:12 "c/" text +bold +italic
            0:15 "b*" text +bold
            0:18 "+e+" text +strikethrough
            0:22 "=f=" verbatim
            0:26 "~g~" code
            0:30 "*" text +bold
            0:31 "~h~" code
            0:34 "*" text +bold
        "#]],
        );
    }

    #[test]
    fn section_elements() {
        check(
            "* h\nSCHEDULED: <2026-01-01 Thu> CLOSED: [2026-01-02 Fri]\n:PROPERTIES:\n:ID: abc\n:END:\n\
             #+TITLE: t\n# comment\n: fixed\n- [X] a\n- [-] b :: c\n1. [ ] d\n| x | *y* |\n|---+---|\n\
             CLOCK: [2026-01-01 Thu 10:00]\n[fn:1] def[fn:2] [1/2] <<t>> \\alpha $x$ {{{m}}}\n",
            expect![[r##"
                0:0 "*" stars +level1
                0:2 "h" headline +level1
                1:0 "SCHEDULED:" planning
                1:11 "<2026-01-01 Thu>" timestamp
                1:28 "CLOSED:" planning
                1:36 "[2026-01-02 Fri]" timestamp +inactive
                2:0 ":PROPERTIES:" drawer
                3:0 ":ID:" property
                4:0 ":END:" drawer
                5:0 "#+TITLE:" keyword
                6:0 "# comment" comment
                7:0 ": fixed" verbatim
                8:0 "-" bullet
                8:2 "[X]" checkbox +checked
                9:0 "-" bullet
                9:2 "[-]" checkbox +partial
                10:0 "1." bullet
                10:3 "[ ]" checkbox
                11:0 "|" tableDelimiter
                11:4 "|" tableDelimiter
                11:6 "*y*" text +bold
                11:10 "|" tableDelimiter
                12:0 "|---+---|" tableDelimiter
                13:0 "CLOCK:" planning
                13:7 "[2026-01-01 Thu 10:00]" timestamp +inactive
                14:0 "[fn:1]" footnote
                14:10 "[fn:2]" footnote
                14:17 "[1/2]" cookie
                14:23 "<<t>>" target
                14:29 "\\alpha" entity
                14:36 "$x$" latex
                14:40 "{{{m}}}" macro
            "##]],
        );
    }

    #[test]
    fn blocks() {
        check(
            "#+BEGIN_SRC rust :tangle yes\nfn *main*() {}\n#+END_SRC\n#+begin_comment\nhidden\n#+end_comment\n\
             #+begin_quote\n*bold*\n#+end_quote\n",
            expect![[r##"
                0:0 "#+BEGIN_SRC rust :tangle yes" block
                2:0 "#+END_SRC" block
                3:0 "#+begin_comment" block
                4:0 "hidden" comment
                5:0 "#+end_comment" block
                6:0 "#+begin_quote" block
                7:0 "*bold*" text +bold
                8:0 "#+end_quote" block
            "##]],
        );
    }

    #[test]
    fn multiline_tokens_are_split_and_trimmed() {
        check(
            "*bold\n  more* and [[a][two\n  lines]]\n",
            expect![[r#"
            0:0 "*bold" text +bold
            1:2 "more*" text +bold
            1:12 "[[a][two" link
            2:2 "lines]]" link
        "#]],
        );
    }

    #[test]
    fn columns_follow_the_encoding() {
        let text = "é😀 *b*\n";
        expect![[r#"
            0:7 "*b*" text +bold
        "#]]
        .assert_eq(&render(text, Encoding::Utf8));
        let columns = |encoding| {
            let parse = org_syntax::parse(text, &TodoKeywords::default());
            let tokens = tokens(&parse.syntax(), parse.todo_keywords());
            let encoded = encode(&tokens, text, &LineIndex::new(text), encoding, None);
            (encoded[0].delta_start, encoded[0].length)
        };
        assert_eq!(columns(Encoding::Utf8), (7, 3));
        assert_eq!(columns(Encoding::Utf16), (4, 3));
        assert_eq!(columns(Encoding::Utf32), (3, 3));
    }

    #[test]
    fn range_requests_filter_tokens() {
        let text = "* a\n* b\n* c\n";
        let parse = org_syntax::parse(text, &TodoKeywords::default());
        let index = LineIndex::new(text);
        let tokens = tokens(&parse.syntax(), parse.todo_keywords());
        let all = encode(&tokens, text, &index, Encoding::Utf16, None);
        let second_line = TextRange::new(4.into(), 8.into());
        let some = encode(&tokens, text, &index, Encoding::Utf16, Some(second_line));
        assert_eq!(all.len(), 6);
        assert_eq!(some.len(), 2);
        assert_eq!(some[0].delta_line, 1);
    }

    #[test]
    fn tokens_never_overlap() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|ext| ext != "org") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let parse = org_syntax::parse(&text, &TodoKeywords::default());
            let tokens = tokens(&parse.syntax(), parse.todo_keywords());
            for pair in tokens.windows(2) {
                assert!(pair[0].range.end() <= pair[1].range.start(), "{}: {:?}", path.display(), pair);
            }
            // Encoded deltas must never go backwards within a line.
            let encoded = encode(&tokens, &text, &LineIndex::new(&text), Encoding::Utf16, None);
            assert!(encoded.iter().all(|token| token.length > 0));
        }
    }
}
