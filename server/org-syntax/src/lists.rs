//! The bullets of ordered list items: `org-list-allow-alphabetical` and
//! `org-plain-list-ordered-item-terminator`, as `org-item-re` reads them.
//! Like Org's, they are the server's for its life, set once the options
//! are read; a thread may use others for a while (tests do).

use std::cell::Cell;
use std::sync::RwLock;

/// `org-plain-list-ordered-item-terminator`: what may end the number of
/// an ordered item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Terminator {
    /// `t`: `.` or `)`.
    #[default]
    Both,
    /// `?.`
    Dot,
    /// `?\)`
    Paren,
}

impl Terminator {
    /// Whether `byte` ends a number.
    pub fn ends(self, byte: u8) -> bool {
        match self {
            Terminator::Both => byte == b'.' || byte == b')',
            Terminator::Dot => byte == b'.',
            Terminator::Paren => byte == b')',
        }
    }
}

/// How ordered items are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ListSyntax {
    /// `org-list-allow-alphabetical`: a letter for a number (`a.`, `B)`).
    pub alphabetical: bool,
    pub terminator: Terminator,
}

impl ListSyntax {
    /// The count and terminator of `org-item-re`, as a regular
    /// expression: `(?:[0-9]+|[A-Za-z])[.)]`, the count in a group of its
    /// own when `group` is on.
    pub fn ordered_pattern(self, group: bool) -> String {
        let count = if self.alphabetical { "[0-9]+|[A-Za-z]" } else { "[0-9]+" };
        let terminator = match self.terminator {
            Terminator::Both => "[.)]",
            Terminator::Dot => r"\.",
            Terminator::Paren => r"\)",
        };
        if group {
            format!("({count}){terminator}")
        } else {
            format!("(?:{count}){terminator}")
        }
    }

    /// The length of an ordered bullet at the start of `bytes`: its count
    /// and terminator.
    pub fn ordered_len(self, bytes: &[u8]) -> Option<usize> {
        let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
        let count = if digits > 0 {
            digits
        } else if self.alphabetical && bytes.first().is_some_and(u8::is_ascii_alphabetic) {
            1
        } else {
            return None;
        };
        bytes.get(count).filter(|b| self.terminator.ends(**b)).map(|_| count + 1)
    }
}

static GLOBAL: RwLock<ListSyntax> = RwLock::new(ListSyntax { alphabetical: false, terminator: Terminator::Both });

thread_local! {
    static LOCAL: Cell<Option<ListSyntax>> = const { Cell::new(None) };
}

/// Set how ordered items are written, for the process.
pub fn set_list_syntax(syntax: ListSyntax) {
    if let Ok(mut global) = GLOBAL.write() {
        *global = syntax;
    }
}

/// How ordered items are written now: this thread's while it has its
/// own, else the process's.
pub fn list_syntax() -> ListSyntax {
    LOCAL.with(Cell::get).unwrap_or_else(|| GLOBAL.read().map_or_else(|_| ListSyntax::default(), |global| *global))
}

/// Run `action` with `syntax` for this thread.
pub fn with_list_syntax<T>(syntax: ListSyntax, action: impl FnOnce() -> T) -> T {
    let before = LOCAL.with(|local| local.replace(Some(syntax)));
    let result = action();
    LOCAL.with(|local| local.set(before));
    result
}
