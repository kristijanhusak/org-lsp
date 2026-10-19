//! Semantic tokens: the highlighting the server offers to every editor.
//!
//! Tokens never overlap. Nested markup is flattened: emphasis becomes
//! modifiers on the text it covers, so `*bold /both/*` yields tokens that
//! carry `bold` and `bold + italic`. Editors map token types and modifiers
//! to their own highlight groups; see `docs/protocol.org` for the legend.

use lsp_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokensLegend};
use org_syntax::{SyntaxElement, SyntaxKind, SyntaxKind::*, SyntaxNode, TextRange, TextSize, TodoKeywords};

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
    DocumentTitle => "documentTitle",
    DocumentInfo => "documentInfo",
    AgendaStructure => "agendaStructure",
    AgendaDate => "agendaDate",
    AgendaScheduled => "agendaScheduled",
    AgendaDeadline => "agendaDeadline",
    AgendaEvent => "agendaEvent",
    AgendaDone => "agendaDone",
    AgendaTimeGrid => "agendaTimeGrid",
    AgendaClocking => "agendaClocking",
    AgendaDimmed => "agendaDimmed",
    AgendaHabit => "agendaHabit",
    CalendarHeader => "calendarHeader",
    CalendarWeekday => "calendarWeekday",
    CalendarSelected => "calendarSelected",
    CalendarPrompt => "calendarPrompt",
    CalendarJournal => "calendarJournal",
    CalendarDailies => "calendarDailies",
    Quote => "quote",
    Verse => "verse",
    // Never sent: VS Code makes it of headline and stars tokens, with their
    // level, for `org-lsp.headlineLevelColors`. A type and not a modifier,
    // as the modifiers fill the 32 bits LSP has for them.
    Outline => "outline",
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
        "today",
        "weekend",
        "past",
        "imminent",
        "distant",
        "range",
        "current",
        "hidden",
        "clocking",
        "archived",
        "ready",
        "alert",
        "overdue",
        "future",
        "todo",
        "inlinetask",
    ];

    /// The bits of `level1` to `level8`.
    pub const LEVELS: u32 = 0xff;
    pub const DONE: u32 = 1 << 8;
    pub const INACTIVE: u32 = 1 << 9;
    pub const CHECKED: u32 = 1 << 10;
    pub const PARTIAL: u32 = 1 << 11;
    pub const BOLD: u32 = 1 << 12;
    pub const ITALIC: u32 = 1 << 13;
    pub const UNDERLINE: u32 = 1 << 14;
    pub const STRIKETHROUGH: u32 = 1 << 15;
    pub const TODAY: u32 = 1 << 16;
    pub const WEEKEND: u32 = 1 << 17;
    pub const PAST: u32 = 1 << 18;
    pub const IMMINENT: u32 = 1 << 19;
    pub const DISTANT: u32 = 1 << 20;
    pub const RANGE: u32 = 1 << 21;
    pub const CURRENT: u32 = 1 << 22;
    /// Text Org hides: link brackets and targets, emphasis markers. An
    /// editor that can conceal text conceals it.
    pub const HIDDEN: u32 = 1 << 23;
    /// Agenda: the entry the clock runs on (`org-agenda-clocking`).
    pub const CLOCKING: u32 = 1 << 24;
    /// A headline with `:ARCHIVE:` in it (`org-archived`).
    pub const ARCHIVED: u32 = 1 << 25;
    /// Agenda: a day of a habit's graph that is in its time to be done
    /// (`org-habit-ready-face`); without this or the next two, before it
    /// (`org-habit-clear-face`).
    pub const READY: u32 = 1 << 26;
    /// The day it is due by (`org-habit-alert-face`).
    pub const ALERT: u32 = 1 << 27;
    /// Past it (`org-habit-overdue-face`).
    pub const OVERDUE: u32 = 1 << 28;
    /// A day of the graph to come (the `-future` faces).
    pub const FUTURE: u32 = 1 << 29;
    /// A headline marked with a TODO keyword not done, past the keyword
    /// (`org-headline-todo`, with `org-fontify-todo-headline`).
    pub const TODO: u32 = 1 << 30;
    /// The stars and title of an inline task (`org-inlinetask`, which
    /// inherits `shadow`).
    pub const INLINETASK: u32 = 1 << 31;

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

/// The token types and modifiers of the LSP specification, which every
/// editor colors without being told: what a client gets that does not know
/// org-lsp's own (`experimental.orgClient`). Org's tokens become the
/// standard ones whose colors come closest to Emacs's faces, the heading
/// levels the faces of `outline-1` and on; what has no standard kind (bold,
/// italic, a habit's graph) is left out.
pub mod standard {
    use super::{modifier, SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokensLegend, TokenType};

    /// The standard types used, in legend order.
    const TYPES: &[&str] = &[
        "namespace",
        "type",
        "function",
        "method",
        "property",
        "variable",
        "parameter",
        "enumMember",
        "keyword",
        "string",
        "number",
        "comment",
        "operator",
        "decorator",
        "macro",
    ];
    /// `deprecated`, which editors strike through: struck-through text.
    const DEPRECATED: u32 = 1;

    pub fn legend() -> SemanticTokensLegend {
        SemanticTokensLegend {
            token_types: TYPES.iter().map(|name| SemanticTokenType::new(name)).collect(),
            token_modifiers: vec![SemanticTokenModifier::DEPRECATED],
        }
    }

    fn index(name: &str) -> u32 {
        TYPES.iter().position(|known| *known == name).expect("a standard type") as u32
    }

    /// The headline faces of Emacs (`org-level-N`, from `outline-N`):
    /// function name, variable name, keyword, comment, type, constant,
    /// builtin, string. A variable is seldom colored, so level 2 is a
    /// property.
    fn level(modifiers: u32) -> &'static str {
        const LEVELS: [&str; 8] =
            ["function", "property", "keyword", "comment", "type", "enumMember", "macro", "string"];
        (0..8).find(|level| modifiers & (1 << level) != 0).map_or("function", |level| LEVELS[level])
    }

    /// The standard type and modifiers of an Org token; `None` for one
    /// that has none.
    fn map(ty: TokenType, modifiers: u32) -> Option<(&'static str, u32)> {
        let has = |bit: u32| modifiers & bit != 0;
        let struck = if has(modifier::STRIKETHROUGH) { DEPRECATED } else { 0 };
        // Link brackets, emphasis markers: what Org hides, faded.
        if has(modifier::HIDDEN) {
            return Some(("comment", 0));
        }
        let name = match ty {
            TokenType::Headline | TokenType::Stars if has(modifier::ARCHIVED | modifier::INLINETASK) => "comment",
            // `org-headline-done` over a done headline's title and tags; a
            // complete cookie in `org-done`'s color.
            TokenType::Cookie if has(modifier::DONE) => "string",
            TokenType::Headline | TokenType::Priority | TokenType::Tag if has(modifier::DONE) => "comment",
            // `org-headline-todo`, copied from the string face.
            TokenType::Headline | TokenType::Priority | TokenType::Tag if has(modifier::TODO) => "string",
            TokenType::Headline | TokenType::Stars | TokenType::Outline => level(modifiers),
            TokenType::TodoKeyword if has(modifier::DONE) => "string",
            TokenType::TodoKeyword | TokenType::Planning => "keyword",
            TokenType::Priority | TokenType::Cookie => "number",
            TokenType::Tag => "decorator",
            TokenType::Timestamp if has(modifier::INACTIVE) => "comment",
            TokenType::Timestamp => "enumMember",
            TokenType::Drawer | TokenType::Latex | TokenType::Entity | TokenType::Macro => "macro",
            TokenType::Property => "property",
            // `org-meta-line` and `org-block-begin-line` are comments.
            TokenType::Keyword | TokenType::Block | TokenType::Comment => "comment",
            TokenType::Bullet | TokenType::Checkbox | TokenType::TableDelimiter => "operator",
            TokenType::Link | TokenType::Target => "namespace",
            TokenType::Code | TokenType::Verbatim => "string",
            TokenType::Footnote => "parameter",
            TokenType::DocumentTitle => "type",
            TokenType::DocumentInfo => "variable",
            // Bold, italic and underline have no standard kind.
            TokenType::Text if struck != 0 => "variable",
            TokenType::Text => return None,
            TokenType::AgendaStructure | TokenType::CalendarHeader => "namespace",
            TokenType::AgendaDate if has(modifier::TODAY) => "keyword",
            TokenType::AgendaDate if has(modifier::WEEKEND) => "type",
            TokenType::AgendaDate => "function",
            TokenType::AgendaScheduled if has(modifier::PAST) => "number",
            TokenType::AgendaScheduled => "string",
            TokenType::AgendaDeadline if has(modifier::IMMINENT) => "keyword",
            TokenType::AgendaDeadline if has(modifier::DISTANT) => "variable",
            TokenType::AgendaDeadline => "enumMember",
            TokenType::AgendaEvent => "variable",
            TokenType::AgendaDone | TokenType::AgendaTimeGrid | TokenType::AgendaDimmed => "comment",
            TokenType::AgendaClocking => "decorator",
            TokenType::AgendaHabit => return None,
            TokenType::CalendarWeekday => "comment",
            TokenType::CalendarSelected | TokenType::CalendarPrompt => "keyword",
            TokenType::CalendarJournal | TokenType::CalendarDailies => "string",
            // `org-quote` and `org-verse` are `org-block`, which has no color.
            TokenType::Quote | TokenType::Verse if struck != 0 => "variable",
            TokenType::Quote | TokenType::Verse => return None,
        };
        Some((name, struck))
    }

    /// `tokens`, encoded with Org's legend, in the standard one: each
    /// token renamed, those without a standard kind left out, and the
    /// positions of the rest made relative again.
    pub fn convert(tokens: Vec<SemanticToken>) -> Vec<SemanticToken> {
        let mut out = Vec::with_capacity(tokens.len());
        let (mut line, mut column) = (0u32, 0u32);
        let mut previous = (0u32, 0u32);
        for token in tokens {
            line += token.delta_line;
            column = if token.delta_line == 0 { column + token.delta_start } else { token.delta_start };
            let Some(ty) = TokenType::ALL.get(token.token_type as usize) else { continue };
            let Some((name, modifiers)) = map(*ty, token.token_modifiers_bitset) else { continue };
            out.push(SemanticToken {
                delta_line: line - previous.0,
                delta_start: if line == previous.0 { column - previous.1 } else { column },
                length: token.length,
                token_type: index(name),
                token_modifiers_bitset: modifiers,
            });
            previous = (line, column);
        }
        out
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn token(delta_line: u32, delta_start: u32, ty: TokenType, modifiers: u32) -> SemanticToken {
            SemanticToken {
                delta_line,
                delta_start,
                length: 3,
                token_type: ty as u32,
                token_modifiers_bitset: modifiers,
            }
        }

        #[test]
        fn org_tokens_become_standard_ones() {
            let tokens = vec![
                token(0, 0, TokenType::Stars, modifier::level(3)),
                token(0, 2, TokenType::TodoKeyword, 0),
                token(0, 5, TokenType::Text, modifier::BOLD),
                token(0, 4, TokenType::Tag, 0),
                token(1, 2, TokenType::Text, modifier::STRIKETHROUGH),
                token(0, 4, TokenType::TodoKeyword, modifier::DONE),
            ];
            let names: Vec<(u32, u32, &str, u32)> = convert(tokens)
                .iter()
                .map(|token| {
                    (
                        token.delta_line,
                        token.delta_start,
                        TYPES[token.token_type as usize],
                        token.token_modifiers_bitset,
                    )
                })
                .collect();
            // The bold text is left out; the tag after it is placed from the
            // keyword before it.
            assert_eq!(
                names,
                [
                    (0, 0, "keyword", 0),
                    (0, 2, "keyword", 0),
                    (0, 9, "decorator", 0),
                    (1, 2, "variable", DEPRECATED),
                    (0, 4, "string", 0),
                ]
            );
        }

        #[test]
        fn every_org_token_has_its_place() {
            for ty in TokenType::ALL {
                if let Some((name, _)) = map(*ty, 0) {
                    assert!(TYPES.contains(&name), "{ty:?} → {name}");
                }
            }
            for level in 1..=8 {
                assert!(TYPES.contains(&super::level(modifier::level(level))));
            }
        }
    }
}

/// One highlighted range. It may span several lines; `encode` splits it."""

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

/// What Org hides, and marks `hidden` here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Conceal {
    /// `org-link-descriptive`: a link shows its description, or its target
    /// when it has none; the brackets, and a target with a description,
    /// are hidden.
    pub links: bool,
    /// `org-hide-emphasis-markers`: the markers around `*bold*`, `/italic/`,
    /// `_underline_`, `+strike+`, `=verbatim=` and `~code~`.
    pub emphasis: bool,
}

/// How headlines and blocks are shown, as Org's options say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fontify {
    /// `org-fontify-done-headline`: past a done keyword, the headline is
    /// `org-headline-done`.
    pub done_headline: bool,
    /// `org-fontify-todo-headline`: past a keyword not done, the headline
    /// is `org-headline-todo`.
    pub todo_headline: bool,
    /// `org-fontify-quote-and-verse-blocks`: the lines of quote and verse
    /// blocks are `org-quote` and `org-verse`.
    pub quote_and_verse: bool,
    /// The looks of `*bold*`, `/italic/`, `_underline_`, `=verbatim=`,
    /// `~code~` and `+strike-through+`, in that order
    /// (`org-emphasis-alist`); none with `org-fontify-emphasized-text`
    /// off.
    pub emphasis: Option<[Emphasis; 6]>,
    /// `org-hidden-keywords`: which of the keywords of [`HIDDEN_KEYWORDS`]
    /// are hidden, a bit for each in its order.
    pub hidden_keywords: u8,
    /// `org-hide-macro-markers`: the braces of `{{{macro}}}` are hidden.
    pub hide_macro_markers: bool,
    /// `org-odd-levels-only`: a headline's face is that of its reduced
    /// level (`org-get-level-face`), `***` the second.
    pub odd_levels: bool,
    /// The stars of the shortest inline task (`org-inlinetask-min-level`),
    /// if there are inline tasks: their stars and title are
    /// `org-inlinetask`.
    pub inlinetask: Option<usize>,
}

impl Default for Fontify {
    fn default() -> Self {
        Fontify {
            done_headline: true,
            todo_headline: false,
            quote_and_verse: false,
            emphasis: Some(Emphasis::DEFAULTS),
            hidden_keywords: 0,
            hide_macro_markers: false,
            odd_levels: false,
            inlinetask: None,
        }
    }
}

/// The keywords `org-hidden-keywords` can hide, before their values.
pub const HIDDEN_KEYWORDS: [&str; 5] = ["title", "subtitle", "author", "email", "date"];

/// The look of a kind of emphasis: a token type of its own (`org-code`,
/// `org-verbatim`), or the text around it with modifiers (`bold`,
/// `italic`, `underline`, `:strike-through t`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Emphasis {
    pub ty: Option<TokenType>,
    pub modifiers: u32,
}

impl Emphasis {
    /// The markers, in the order of [`Fontify::emphasis`].
    pub const MARKERS: [&'static str; 6] = ["*", "/", "_", "=", "~", "+"];

    /// The default of `org-emphasis-alist`.
    pub const DEFAULTS: [Emphasis; 6] = [
        Emphasis { ty: None, modifiers: modifier::BOLD },
        Emphasis { ty: None, modifiers: modifier::ITALIC },
        Emphasis { ty: None, modifiers: modifier::UNDERLINE },
        Emphasis { ty: Some(TokenType::Verbatim), modifiers: 0 },
        Emphasis { ty: Some(TokenType::Code), modifiers: 0 },
        Emphasis { ty: None, modifiers: modifier::STRIKETHROUGH },
    ];

    /// No face: the text looks as the text around it.
    pub const NONE: Emphasis = Emphasis { ty: None, modifiers: 0 };
}

/// All tokens of a document, in order.
pub fn tokens(root: &SyntaxNode, todo: &TodoKeywords, conceal: Conceal) -> Vec<Token> {
    tokens_with(root, todo, conceal, Fontify::default())
}

/// [`tokens`], headlines and blocks shown as `fontify` says.
pub fn tokens_with(root: &SyntaxNode, todo: &TodoKeywords, conceal: Conceal, fontify: Fontify) -> Vec<Token> {
    let mut walker = Walker { todo, conceal, fontify, out: Vec::new() };
    walker.children(root, Style::PLAIN);
    walker.out
}

/// `tokens` with the words radio targets make links (`ranges`, byte
/// offsets) as links, over what was there; emphasis stays.
/// The tokens without the statistics cookies: Org colors them only with
/// the `checkbox` rule of `org-list-automatic-rules`.
pub fn without_cookies(mut tokens: Vec<Token>) -> Vec<Token> {
    tokens.retain(|token| token.ty != TokenType::Cookie);
    tokens
}

pub fn with_radio_links(tokens: Vec<Token>, ranges: &[(usize, usize)]) -> Vec<Token> {
    if ranges.is_empty() {
        return tokens;
    }
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len() + ranges.len());
    let emphasis = modifier::BOLD | modifier::ITALIC | modifier::UNDERLINE | modifier::STRIKETHROUGH;
    let mut links: Vec<Token> = ranges
        .iter()
        .map(|(start, end)| Token {
            range: TextRange::new((*start as u32).into(), (*end as u32).into()),
            ty: TokenType::Link,
            modifiers: 0,
        })
        .collect();
    for token in tokens {
        let (start, end) = (token.range.start(), token.range.end());
        let mut at = start;
        for link in links.iter_mut().filter(|link| link.range.start() < end && link.range.end() > start) {
            link.modifiers |= token.modifiers & emphasis;
            if link.range.start() > at {
                out.push(Token { range: TextRange::new(at, link.range.start()), ..token });
            }
            at = at.max(link.range.end());
        }
        if at < end {
            out.push(Token { range: TextRange::new(at, end), ..token });
        }
    }
    out.extend(links);
    out.sort_by_key(|token| token.range.start());
    out
}

/// `tokens` with the text of `ranges` hidden (`org-pretty-entities`: an
/// entity shown as its character, by an inlay hint): the parts of tokens
/// there, and an entity token for what no token covers.
pub fn with_hidden(tokens: Vec<Token>, ranges: &[(usize, usize)]) -> Vec<Token> {
    if ranges.is_empty() {
        return tokens;
    }
    let ranges: Vec<TextRange> =
        ranges.iter().map(|(start, end)| TextRange::new((*start as u32).into(), (*end as u32).into())).collect();
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len() + ranges.len());
    for token in &tokens {
        let (start, end) = (token.range.start(), token.range.end());
        let mut at = start;
        for range in ranges.iter().filter(|range| range.start() < end && range.end() > start) {
            let (from, to) = (range.start().max(start), range.end().min(end));
            if from > at {
                out.push(Token { range: TextRange::new(at, from), ..*token });
            }
            out.push(Token {
                range: TextRange::new(from, to),
                modifiers: token.modifiers | modifier::HIDDEN,
                ..*token
            });
            at = to;
        }
        if at < end {
            out.push(Token { range: TextRange::new(at, end), ..*token });
        }
    }
    // What no token covers.
    for range in &ranges {
        let mut at = range.start();
        let mut covering: Vec<&Token> = tokens
            .iter()
            .filter(|token| token.range.start() < range.end() && token.range.end() > range.start())
            .collect();
        covering.sort_by_key(|token| token.range.start());
        for token in covering {
            if token.range.start() > at {
                out.push(Token {
                    range: TextRange::new(at, token.range.start()),
                    ty: TokenType::Entity,
                    modifiers: modifier::HIDDEN,
                });
            }
            at = at.max(token.range.end());
        }
        if at < range.end() {
            out.push(Token {
                range: TextRange::new(at, range.end()),
                ty: TokenType::Entity,
                modifiers: modifier::HIDDEN,
            });
        }
    }
    out.sort_by_key(|token| token.range.start());
    out
}

struct Walker<'a> {
    todo: &'a TodoKeywords,
    conceal: Conceal,
    fontify: Fontify,
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
            // Timestamps in a property's value, as `org-activate-dates`
            // colors them wherever they are.
            SyntaxElement::Token(token) if token.kind() == PROPERTY_VALUE => {
                for (start, end) in timestamps_in(token.text()) {
                    let at = |offset: usize| range.start() + TextSize::from(offset as u32);
                    let inactive = token.text()[start..].starts_with('[');
                    self.push(
                        TextRange::new(at(start), at(end)),
                        TokenType::Timestamp,
                        if inactive { modifier::INACTIVE } else { 0 },
                    );
                }
            }
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
            PLANNING_KEYWORD | CLOCK_KEYWORD | COMMENT_KEYWORD => plain(TokenType::Planning),
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
            // The markers of emphasis, which Org can hide.
            MARKER
                if self.conceal.emphasis
                    && matches!(parent, BOLD | ITALIC | UNDERLINE | STRIKE_THROUGH | CODE | VERBATIM) =>
            {
                Some((style.text.unwrap_or(TokenType::Text), style.modifiers | modifier::HIDDEN))
            }
            // Text and emphasis markers take the style of their context.
            TEXT | MARKER if style.text.is_some() || style.modifiers != 0 => {
                Some((style.text.unwrap_or(TokenType::Text), style.modifiers))
            }
            _ => None,
        }
    }

    /// A bracket link as `org-link-descriptive` shows it: the description,
    /// or the target when there is none, with the rest hidden.
    fn link(&mut self, node: &SyntaxNode) {
        let described = node.children_with_tokens().any(|child| child.kind() == LINK_SEPARATOR);
        let (mut shown_start, mut shown_end) = (None, None);
        for child in node.children_with_tokens() {
            let range = child.text_range();
            match child.kind() {
                LINK_OPEN | LINK_SEPARATOR | LINK_CLOSE => {
                    if let (Some(start), Some(end)) = (shown_start.take(), shown_end.take()) {
                        self.push(TextRange::new(start, end), TokenType::Link, 0);
                    }
                    self.push(range, TokenType::Link, modifier::HIDDEN);
                }
                LINK_PATH if described => self.push(range, TokenType::Link, modifier::HIDDEN),
                _ => {
                    shown_start.get_or_insert(range.start());
                    shown_end = Some(range.end());
                }
            }
        }
        if let (Some(start), Some(end)) = (shown_start, shown_end) {
            self.push(TextRange::new(start, end), TokenType::Link, 0);
        }
    }

    fn node(&mut self, node: &SyntaxNode, style: Style) {
        let range = node.text_range();
        match node.kind() {
            HEADLINE => self.headline(node),
            // Tags, links and the like are single tokens whatever they contain.
            TAGS => self.push(range, TokenType::Tag, 0),
            LINK if self.conceal.links => self.link(node),
            LINK | PLAIN_LINK | ANGLE_LINK | CITATION => self.push(range, TokenType::Link, 0),
            // The code of `src_LANG{CODE}` is the editor's to color in its
            // language, as Emacs does with `org-src-fontify-natively`.
            INLINE_SRC_BLOCK => match inline_src_body(&node.text().to_string()) {
                Some((start, end)) => {
                    let at = |offset: usize| range.start() + TextSize::from(offset as u32);
                    self.push(TextRange::new(range.start(), at(start)), TokenType::Code, 0);
                    self.push(TextRange::new(at(end), range.end()), TokenType::Code, 0);
                }
                None => self.push(range, TokenType::Code, 0),
            },
            BOLD | ITALIC | UNDERLINE | VERBATIM | CODE | STRIKE_THROUGH => self.emphasis(node, style),
            INLINE_BABEL_CALL => self.push(range, TokenType::Code, 0),
            TIMESTAMP => {
                let inactive = node.first_token().is_some_and(|token| token.text().starts_with('['));
                self.push(range, TokenType::Timestamp, if inactive { modifier::INACTIVE } else { 0 });
            }
            FOOTNOTE_REFERENCE => self.push(range, TokenType::Footnote, 0),
            // `org-get-checkbox-statistics-face`: a complete cookie is done.
            STATISTICS_COOKIE => {
                let done = cookie_complete(&node.text().to_string());
                self.push(range, TokenType::Cookie, if done { modifier::DONE } else { 0 })
            }
            KEYWORD => self.keyword(node),
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
            MACRO => self.macro_call(node),
            BLOCK | DYNAMIC_BLOCK => self.block(node),
            _ => self.children(node, style),
        }
    }

    /// Emphasis in the look `org-emphasis-alist` gives its marker; the
    /// text as around it with `org-fontify-emphasized-text` off.
    fn emphasis(&mut self, node: &SyntaxNode, style: Style) {
        let kind = node.kind();
        let Some(looks) = self.fontify.emphasis else {
            return self.children(node, style);
        };
        let index = [BOLD, ITALIC, UNDERLINE, VERBATIM, CODE, STRIKE_THROUGH]
            .iter()
            .position(|other| *other == kind)
            .unwrap_or(0);
        let look = looks[index];
        let style = match look.ty {
            Some(ty) => Style { text: Some(ty), modifiers: look.modifiers },
            None => Style { text: style.text.or(Some(TokenType::Text)), modifiers: style.modifiers | look.modifiers },
        };
        match kind {
            // Code and verbatim text hold no other objects: one token, or
            // with their markers apart to hide them.
            CODE | VERBATIM if !self.conceal.emphasis => {
                self.push(node.text_range(), style.text.unwrap_or(TokenType::Text), style.modifiers)
            }
            _ => self.children(node, style),
        }
    }

    /// `{{{name(args)}}}`, its braces hidden with `org-hide-macro-markers`.
    fn macro_call(&mut self, node: &SyntaxNode) {
        let range = node.text_range();
        let text = node.text().to_string();
        if !self.fontify.hide_macro_markers || !text.starts_with("{{{") || !text.ends_with("}}}") || text.len() < 6 {
            return self.push(range, TokenType::Macro, 0);
        }
        let three = TextSize::from(3);
        self.push(TextRange::new(range.start(), range.start() + three), TokenType::Macro, modifier::HIDDEN);
        self.push(TextRange::new(range.start() + three, range.end() - three), TokenType::Macro, 0);
        self.push(TextRange::new(range.end() - three, range.end()), TokenType::Macro, modifier::HIDDEN);
    }

    fn headline(&mut self, node: &SyntaxNode) {
        let level = node.first_token().map_or(1, |stars| stars.text().len());
        let level = modifier::level(org_core::structure::reduced_level(level, self.fontify.odd_levels));
        // `org-archived`: what follows the stars of a headline with
        // `:ARCHIVE:` in it.
        let line = node.text().to_string();
        let archived = line.split('\n').next().is_some_and(|line| line.contains(":ARCHIVE:"));
        // `org-fontify-done-headline`: after a done keyword, the rest of
        // the headline is `org-headline-done`; `org-fontify-todo-headline`:
        // after one not done, `org-headline-todo`.
        let keyword = node.children_with_tokens().find(|child| child.kind() == TODO_KEYWORD);
        let state = keyword.map(|keyword| match self.todo.is_done(&keyword.to_string()) {
            true if self.fontify.done_headline => modifier::DONE,
            false if self.fontify.todo_headline => modifier::TODO,
            _ => 0,
        });
        let mut after_stars = self.out.len();
        for child in node.children_with_tokens() {
            if child.kind() == STARS {
                self.push(child.text_range(), TokenType::Stars, level);
                after_stars = self.out.len();
                continue;
            }
            match child.kind() {
                TITLE => {
                    let title = child.into_node().expect("TITLE is a node");
                    self.children(&title, Style { text: Some(TokenType::Headline), modifiers: level });
                }
                _ => self.element(node, child, Style::PLAIN),
            }
        }
        let headline_end = node.text_range().start() + TextSize::from(line.find('\n').unwrap_or(line.len()) as u32);
        if let Some(state) = state.filter(|state| *state != 0) {
            for token in &mut self.out[after_stars..] {
                if token.range.start() < headline_end && token.ty != TokenType::TodoKeyword {
                    // In place of the level, which would color it as well.
                    token.modifiers = (token.modifiers & !modifier::LEVELS) | state;
                }
            }
        }
        if archived {
            for token in &mut self.out[after_stars..] {
                if token.range.start() < headline_end {
                    // In place of the level, as `org-archived` is
                    // prepended over the level's face: an editor that
                    // colors a type with a modifier (`headline.level1`)
                    // over a modifier alone (`*.archived`) would show the
                    // level.
                    token.modifiers = (token.modifiers & !modifier::LEVELS) | modifier::ARCHIVED;
                }
            }
        }
        // `org-inlinetask-fontify`: the stars and the rest of the line of an
        // inline task are `org-inlinetask`, under the faces of its keyword,
        // priority and tags.
        let stars = node.first_token().map_or(0, |stars| stars.text().trim_end().len());
        if self.fontify.inlinetask.is_some_and(|min| stars >= min) {
            for token in &mut self.out[after_stars.saturating_sub(1)..] {
                let own = matches!(token.ty, TokenType::TodoKeyword | TokenType::Priority | TokenType::Tag);
                if token.range.start() < headline_end && !own {
                    token.modifiers = (token.modifiers & !modifier::LEVELS) | modifier::INLINETASK;
                }
            }
        }
    }

    /// A keyword line; the values of `#+TITLE:`, and of `#+SUBTITLE:`,
    /// `#+AUTHOR:`, `#+EMAIL:` and `#+DATE:`, in `org-document-title` and
    /// `org-document-info`.
    fn keyword(&mut self, node: &SyntaxNode) {
        let key = node.children_with_tokens().find(|child| child.kind() == KEYWORD_KEY).map(|key| key.to_string());
        let value = match key.as_ref().map(|key| key.to_ascii_lowercase()).as_deref() {
            Some("#+title:") => Some(TokenType::DocumentTitle),
            Some("#+subtitle:" | "#+author:" | "#+email:" | "#+date:") => Some(TokenType::DocumentInfo),
            _ => None,
        };
        let hidden = key.as_deref().is_some_and(|key| {
            let name = key.trim().trim_start_matches("#+").trim_end_matches(':').to_ascii_lowercase();
            let bit = HIDDEN_KEYWORDS.iter().position(|hidden| *hidden == name);
            value.is_some() && bit.is_some_and(|bit| self.fontify.hidden_keywords & (1 << bit) != 0)
        });
        for child in node.children_with_tokens() {
            match (child.kind(), value) {
                (KEYWORD_VALUE, Some(ty)) => self.push(child.text_range(), ty, 0),
                // `org-hidden-keywords`: `#+TITLE:` hidden, its value shown.
                (KEYWORD_KEY, _) if hidden => self.push(child.text_range(), TokenType::Keyword, modifier::HIDDEN),
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
        // `org-fontify-quote-and-verse-blocks`: the lines inside quote and
        // verse blocks, under what they contain.
        let quoted = begin
            .as_ref()
            .and_then(|begin| begin.as_token())
            .and_then(|begin| match begin.text().to_ascii_lowercase().as_str() {
                "#+begin_quote" => Some(TokenType::Quote),
                "#+begin_verse" => Some(TokenType::Verse),
                _ => None,
            })
            .filter(|_| self.fontify.quote_and_verse);
        let inside = Style { text: quoted, modifiers: 0 };
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
                TEXT if quoted.is_some() => self.push(child.text_range(), quoted.expect("quoted"), 0),
                // Raw contents (source code, examples) are left to the editor.
                TEXT => {}
                _ => self.element(node, child, inside),
            }
        }
    }
}

/// Whether a statistics cookie is complete: `[100%]`, or `[n/n]` with both
/// numbers the same (`[0/0]` too).
fn cookie_complete(cookie: &str) -> bool {
    let inner = cookie.trim_start_matches('[').trim_end_matches(']');
    match inner.split_once('/') {
        Some((done, all)) => !done.is_empty() && done == all,
        None => inner == "100%",
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
            // A token that goes on to another line leaves out the blanks
            // that indent the line or end it; blanks within the line, as
            // between the words of emphasis, keep its style.
            let piece = &text[piece_start..piece_end];
            let trimmed = if piece_start == line_start { piece.trim_start() } else { piece };
            let piece_start = piece_start + (piece.len() - trimmed.len());
            let trimmed = if piece_end == line_end { trimmed.trim_end() } else { trimmed };
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

/// Where the code of an inline source block (`src_LANG[HEADERS]{CODE}`)
/// starts and ends in its text, when it has any.
fn inline_src_body(text: &str) -> Option<(usize, usize)> {
    let name = text.get(4..)?.find(['[', '{'])? + 4;
    let mut at = name;
    if text[at..].starts_with('[') {
        let mut depth = 0;
        for (offset, c) in text[at..].char_indices() {
            match c {
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                at += offset + 1;
                break;
            }
        }
    }
    (text[at..].starts_with('{') && text.ends_with('}') && at + 1 < text.len() - 1).then(|| (at + 1, text.len() - 1))
}

/// Where `org-tsr-regexp-both` matches in `text`: a timestamp
/// (`<2026-05-18 Mon>`, `[2026-05-18]`) or a range of two, not right
/// after a `[`, as `org-activate-dates` finds them.
fn timestamps_in(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    // One timestamp starting at `at`: where it ends.
    let stamp = |at: usize| -> Option<usize> {
        if !matches!(bytes.get(at), Some(b'<' | b'[')) {
            return None;
        }
        let date = bytes.get(at + 1..at + 11)?;
        let digits = |range: std::ops::Range<usize>| date[range].iter().all(u8::is_ascii_digit);
        if !(digits(0..4) && date[4] == b'-' && digits(5..7) && date[7] == b'-' && digits(8..10)) {
            return None;
        }
        let mut end = at + 11;
        if !matches!(bytes.get(end), Some(b'>' | b']')) {
            // ` ` and the fewest characters up to a closing bracket.
            if bytes.get(end) != Some(&b' ') {
                return None;
            }
            end += 1 + text[end + 1..].find(['>', ']', '\n'])?;
            if bytes[end] == b'\n' {
                return None;
            }
        }
        Some(end + 1)
    };
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let after_bracket = at > 0 && bytes[at - 1] == b'[';
        match stamp(at).filter(|_| !after_bracket) {
            Some(mut end) => {
                // `--?-?` and a second timestamp make a range.
                let dashes = bytes[end..].iter().take(3).take_while(|b| **b == b'-').count();
                if dashes > 0 {
                    if let Some(second) = stamp(end + dashes) {
                        end = second;
                    }
                }
                found.push((at, end));
                at = end;
            }
            None => at += text[at..].chars().next().map_or(1, char::len_utf8),
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use expect_test::{expect, Expect};

    use super::*;

    /// Render tokens as `line:column text type +modifiers`, one per line piece.
    fn render(text: &str, encoding: Encoding) -> String {
        render_concealed(text, encoding, Conceal::default())
    }

    fn render_concealed(text: &str, encoding: Encoding, conceal: Conceal) -> String {
        render_with(text, encoding, conceal, Fontify::default())
    }

    fn render_with(text: &str, encoding: Encoding, conceal: Conceal, fontify: Fontify) -> String {
        let parse = org_syntax::parse(text, &TodoKeywords::default());
        let index = LineIndex::new(text);
        let tokens = tokens_with(&parse.syntax(), parse.todo_keywords(), conceal, fontify);
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
    fn markup_options() {
        // `org-emphasis-alist` with bold italic and code as verbatim, no
        // look for underline (left out); `org-hidden-keywords` hiding the
        // title's keyword, `org-hide-macro-markers` the braces.
        let mut emphasis = [Emphasis::NONE; 6];
        emphasis[0] = Emphasis { ty: None, modifiers: modifier::BOLD | modifier::ITALIC };
        emphasis[4] = Emphasis { ty: Some(TokenType::Verbatim), modifiers: 0 };
        let fontify =
            Fontify { emphasis: Some(emphasis), hidden_keywords: 1, hide_macro_markers: true, ..Fontify::default() };
        let text = "#+TITLE: Doc\n#+AUTHOR: Me\n*b* _u_ ~c~ {{{m(1)}}}\n";
        expect![[r##"
            0:0 "#+TITLE:" keyword +hidden
            0:9 "Doc" documentTitle
            1:0 "#+AUTHOR:" keyword
            1:10 "Me" documentInfo
            2:0 "*b*" text +bold +italic
            2:4 "_u_" text
            2:8 "~c~" verbatim
            2:12 "{{{" macro +hidden
            2:15 "m(1)" macro
            2:19 "}}}" macro +hidden
        "##]]
        .assert_eq(&render_with(text, Encoding::Utf16, Conceal::default(), fontify));
        // `org-fontify-emphasized-text` off: no looks.
        let plain = Fontify { emphasis: None, ..Fontify::default() };
        expect![[""]].assert_eq(&render_with("*b* ~c~\n", Encoding::Utf16, Conceal::default(), plain));
    }

    #[test]
    fn fontify_options() {
        // `org-fontify-todo-headline` on, `org-fontify-done-headline` off,
        // `org-fontify-quote-and-verse-blocks` on: the text of quote and
        // verse blocks, under the emphasis it carries.
        let fontify =
            Fontify { done_headline: false, todo_headline: true, quote_and_verse: true, ..Fontify::default() };
        let text = "* TODO [#A] Call :x:\n* DONE Done\n#+begin_quote\nSaid *this*\n#+end_quote\n\
                    #+begin_verse\nA line\n#+end_verse\n#+begin_example\nraw\n#+end_example\n";
        expect![[r##"
            0:0 "*" stars +level1
            0:2 "TODO" todoKeyword
            0:7 "[#A]" priority +todo
            0:12 "Call" headline +todo
            0:17 ":x:" tag +todo
            1:0 "*" stars +level1
            1:2 "DONE" todoKeyword +done
            1:7 "Done" headline +level1
            2:0 "#+begin_quote" block
            3:0 "Said " quote
            3:5 "*this*" quote +bold
            4:0 "#+end_quote" block
            5:0 "#+begin_verse" block
            6:0 "A line" verse
            7:0 "#+end_verse" block
            8:0 "#+begin_example" block
            10:0 "#+end_example" block
        "##]]
        .assert_eq(&render_with(text, Encoding::Utf16, Conceal::default(), fontify));
    }

    #[test]
    fn timestamps_in_property_values() {
        check(
            ":PROPERTIES:\n:CREATED: [2026-05-18 Mon 10:30]\n:SPAN: <2026-05-18>--<2026-05-20> and [[2026-01-01]]\n:END:\n",
            expect![[r#"
                0:0 ":PROPERTIES:" drawer
                1:0 ":CREATED:" property
                1:10 "[2026-05-18 Mon 10:30]" timestamp +inactive
                2:0 ":SPAN:" property
                2:7 "<2026-05-18>--<2026-05-20>" timestamp
                3:0 ":END:" drawer
            "#]],
        );
    }

    #[test]
    fn the_code_of_inline_source_blocks_is_left_to_the_editor() {
        check(
            "src_lua{print({1})} src_sh[:results none]{ls} src_c{}\n",
            expect![[r#"
                0:0 "src_lua{" code
                0:18 "}" code
                0:20 "src_sh[:results none]{" code
                0:44 "}" code
                0:46 "src_c{}" code
            "#]],
        );
    }

    #[test]
    fn hidden_parts_of_links_as_org_link_descriptive_shows_them() {
        let links = Conceal { links: true, emphasis: false };
        expect![[r#"
            0:0 "[[https://x.org][" link +hidden
            0:17 "the *site*" link
            0:27 "]]" link +hidden
            0:30 "[[" link +hidden
            0:32 "file:a.org" link
            0:42 "]]" link +hidden
            1:0 "[[a][" link +hidden
            1:5 "two" link
            2:0 "lines" link
            2:5 "]]" link +hidden
        "#]]
        .assert_eq(&render_concealed(
            "[[https://x.org][the *site*]] [[file:a.org]]\n[[a][two\nlines]]\n",
            Encoding::Utf16,
            links,
        ));
    }

    #[test]
    fn hidden_emphasis_markers_as_org_hide_emphasis_markers_hides_them() {
        let emphasis = Conceal { links: false, emphasis: true };
        expect![[r#"
            0:0 "*" text +bold +hidden
            0:1 "b " text +bold
            0:3 "/" text +bold +italic +hidden
            0:4 "i" text +bold +italic
            0:5 "/" text +bold +italic +hidden
            0:6 "*" text +bold +hidden
            0:8 "=" verbatim +hidden
            0:9 "v" verbatim
            0:10 "=" verbatim +hidden
            0:12 "~" code +hidden
            0:13 "c" code
            0:14 "~" code +hidden
            0:16 "_" text +underline +hidden
            0:17 "u" text +underline
            0:18 "_" text +underline +hidden
            0:20 "+" text +strikethrough +hidden
            0:21 "s" text +strikethrough
            0:22 "+" text +strikethrough +hidden
            1:0 "*" stars +level1
            1:2 "A " headline +level1
            1:4 "*" headline +level1 +bold +hidden
            1:5 "b" headline +level1 +bold
            1:6 "*" headline +level1 +bold +hidden
        "#]]
        .assert_eq(&render_concealed("*b /i/* =v= ~c~ _u_ +s+\n* A *b*\n", Encoding::Utf16, emphasis));
        // Off, the markers are text like the rest.
        assert!(!render("*b* =v=\n", Encoding::Utf16).contains("hidden"));
    }

    #[test]
    fn headline_parts() {
        check(
            "** TODO [#A] Write *the* [[a][parser]] :work:rust:\n*** DONE x\n",
            expect![[r#"
                0:0 "**" stars +level2
                0:3 "TODO" todoKeyword
                0:8 "[#A]" priority
                0:13 "Write " headline +level2
                0:19 "*the*" headline +level2 +bold
                0:24 " " headline +level2
                0:25 "[[a][parser]]" link
                0:39 ":work:rust:" tag
                1:0 "***" stars +level3
                1:4 "DONE" todoKeyword +done
                1:9 "x" headline +done
            "#]],
        );
    }

    #[test]
    fn done_headlines_cookies_and_document_keywords() {
        // `org-fontify-done-headline`, `org-checkbox-statistics-done` for a
        // complete cookie only, `org-document-title` and `-info`.
        check(
            "#+TITLE: Notes\n#+AUTHOR: Me\n#+OPTIONS: toc:nil\n* DONE [#B] Task [2/2] :a:\n* TODO Other [1/2] [100%] [0/0]\n",
            expect![[r##"
                0:0 "#+TITLE:" keyword
                0:9 "Notes" documentTitle
                1:0 "#+AUTHOR:" keyword
                1:10 "Me" documentInfo
                2:0 "#+OPTIONS:" keyword
                3:0 "*" stars +level1
                3:2 "DONE" todoKeyword +done
                3:7 "[#B]" priority +done
                3:12 "Task " headline +done
                3:17 "[2/2]" cookie +done
                3:23 ":a:" tag +done
                4:0 "*" stars +level1
                4:2 "TODO" todoKeyword
                4:7 "Other " headline +level1
                4:13 "[1/2]" cookie
                4:18 " " headline +level1
                4:19 "[100%]" cookie +done
                4:25 " " headline +level1
                4:26 "[0/0]" cookie +done
            "##]],
        );
    }

    #[test]
    fn archived_headlines() {
        check(
            "* TODO Old :x:ARCHIVE:\nbody\n* Plain\n",
            expect![[r#"
                0:0 "*" stars +level1
                0:2 "TODO" todoKeyword +archived
                0:7 "Old" headline +archived
                0:11 ":x:ARCHIVE:" tag +archived
                2:0 "*" stars +level1
                2:2 "Plain" headline +level1
            "#]],
        );
    }

    /// `org-inlinetask-fontify`: the stars and the title of an inline task
    /// are `org-inlinetask`, its keyword and tags keep their faces.
    #[test]
    fn inline_tasks() {
        let text = "* A\n***** TODO Task :t:\n***** END\n**** Deep\n";
        let fontify = Fontify { inlinetask: Some(5), ..Fontify::default() };
        expect![[r#"
            0:0 "*" stars +level1
            0:2 "A" headline +level1
            1:0 "*****" stars +inlinetask
            1:6 "TODO" todoKeyword
            1:11 "Task" headline +inlinetask
            1:16 ":t:" tag
            2:0 "*****" stars +inlinetask
            2:6 "END" headline +inlinetask
            3:0 "****" stars +level4
            3:5 "Deep" headline +level4
        "#]]
        .assert_eq(&render_with(text, Encoding::Utf16, Conceal::default(), fontify));
    }

    #[test]
    fn nested_emphasis_flattens_to_modifiers() {
        check(
            "a *b /c _d_ c/ b* +e+ =f= ~g~ *~h~*\n",
            expect![[r#"
                0:2 "*b " text +bold
                0:5 "/c " text +bold +italic
                0:8 "_d_" text +bold +italic +underline
                0:11 " c/" text +bold +italic
                0:14 " b*" text +bold
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
                5:9 "t" documentTitle
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
            let tokens = tokens(&parse.syntax(), parse.todo_keywords(), Conceal::default());
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
        let tokens = tokens(&parse.syntax(), parse.todo_keywords(), Conceal::default());
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
            let tokens = tokens(&parse.syntax(), parse.todo_keywords(), Conceal::default());
            for pair in tokens.windows(2) {
                assert!(pair[0].range.end() <= pair[1].range.start(), "{}: {:?}", path.display(), pair);
            }
            // Encoded deltas must never go backwards within a line.
            let encoded = encode(&tokens, &text, &LineIndex::new(&text), Encoding::Utf16, None);
            assert!(encoded.iter().all(|token| token.length > 0));
        }
    }
    #[test]
    fn radio_links_go_over_what_is_there() {
        let token = |start: u32, end: u32, ty: TokenType, modifiers: u32| Token {
            range: TextRange::new(start.into(), end.into()),
            ty,
            modifiers,
        };
        let tokens = vec![token(0, 10, TokenType::Text, modifier::BOLD), token(12, 14, TokenType::Tag, 0)];
        assert_eq!(
            with_radio_links(tokens, &[(3, 6)]),
            [
                token(0, 3, TokenType::Text, modifier::BOLD),
                token(3, 6, TokenType::Link, modifier::BOLD),
                token(6, 10, TokenType::Text, modifier::BOLD),
                token(12, 14, TokenType::Tag, 0),
            ]
        );
    }
}
