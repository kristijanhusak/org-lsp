//! Syntax kinds for the lossless Org syntax tree.

/// Every token and node kind in the tree.
///
/// Tokens are leaves and carry text; nodes group tokens and other nodes.
/// Every byte of the source ends up in exactly one token.
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    // ----- tokens: generic
    WHITESPACE,
    NEWLINE,
    BLANK_LINE,
    TEXT,

    // ----- tokens: headline
    STARS,
    TODO_KEYWORD,
    PRIORITY,
    COLON,
    TAG,

    // ----- tokens: elements
    PLANNING_KEYWORD,
    DRAWER_BEGIN,
    DRAWER_END,
    PROPERTY_KEY,
    PROPERTY_VALUE,
    KEYWORD_KEY,
    KEYWORD_VALUE,
    BLOCK_BEGIN,
    BLOCK_PARAMETERS,
    BLOCK_END,
    COMMENT_TEXT,
    FIXED_WIDTH_TEXT,
    RULE,
    PIPE,
    TABLE_RULE,
    BULLET,
    COUNTER_SET,
    CHECKBOX,
    TAG_SEPARATOR,
    FOOTNOTE_LABEL,
    CLOCK_KEYWORD,

    // ----- tokens: objects
    MARKER,
    LINK_OPEN,
    LINK_PATH,
    LINK_SEPARATOR,
    LINK_CLOSE,
    ANGLE_OPEN,
    ANGLE_CLOSE,
    TIMESTAMP_TEXT,
    FOOTNOTE_OPEN,
    FOOTNOTE_CLOSE,
    COOKIE_TEXT,
    TARGET_TEXT,

    // ----- nodes: structure
    DOCUMENT,
    SECTION,
    HEADLINE,
    TITLE,
    TAGS,

    // ----- nodes: elements
    PLANNING,
    PROPERTY_DRAWER,
    NODE_PROPERTY,
    DRAWER,
    BLOCK,
    DYNAMIC_BLOCK,
    KEYWORD,
    COMMENT,
    FIXED_WIDTH,
    HORIZONTAL_RULE,
    TABLE,
    TABLE_ROW,
    TABLE_CELL,
    LIST,
    LIST_ITEM,
    ITEM_TAG,
    FOOTNOTE_DEFINITION,
    CLOCK,
    LATEX_ENVIRONMENT,
    DIARY_SEXP,
    PARAGRAPH,

    // ----- nodes: objects
    BOLD,
    ITALIC,
    UNDERLINE,
    STRIKE_THROUGH,
    CODE,
    VERBATIM,
    SUBSCRIPT,
    SUPERSCRIPT,
    LINK,
    PLAIN_LINK,
    ANGLE_LINK,
    TIMESTAMP,
    FOOTNOTE_REFERENCE,
    STATISTICS_COOKIE,
    TARGET,
    RADIO_TARGET,
    ENTITY,
    LATEX_FRAGMENT,
    EXPORT_SNIPPET,
    MACRO,
    INLINE_SRC_BLOCK,
    INLINE_BABEL_CALL,
    CITATION,

    #[doc(hidden)]
    __LAST,
}

impl SyntaxKind {
    /// Whether this kind is a leaf token (as opposed to a node).
    pub fn is_token(self) -> bool {
        (self as u16) < (SyntaxKind::DOCUMENT as u16)
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}

/// The rowan language marker for Org.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OrgLanguage {}

impl rowan::Language for OrgLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        assert!(raw.0 < SyntaxKind::__LAST as u16, "invalid syntax kind {}", raw.0);
        // SAFETY: `SyntaxKind` is `repr(u16)` with contiguous discriminants
        // starting at zero, and the value was just bounds-checked.
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<OrgLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<OrgLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<OrgLanguage>;
