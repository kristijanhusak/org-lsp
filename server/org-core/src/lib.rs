//! Org mode logic on top of the parser.
//!
//! `org-syntax` answers "what is written here"; this crate answers "what
//! does it mean": which day a timestamp falls on, what an entry inherits
//! from its parents, what the agenda shows. Nothing here knows about LSP.

pub mod agenda;
pub mod archive;
pub mod calc;
pub mod calendar;
pub mod capture;
pub mod checkbox;
pub mod cite;
pub mod clock;
pub mod clock_range;
pub mod clocktable;
pub mod date;
pub mod edit;
pub mod format;
pub mod indent;
pub mod link;
pub mod list;
pub mod meta_return;
pub mod newline;
pub mod open;
pub mod outline;
pub mod planning;
pub mod preview;
pub mod priority;
pub mod properties;
pub mod read_date;
pub mod refile;
pub mod sort;
pub mod sparse;
pub mod stamp;
pub mod structure;
pub mod table;
pub mod tags;
pub mod tangle;
pub mod tblfm;
pub mod text;
pub mod timestr;
pub mod todo;

/// Why a command changed nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The command has no meaning where the cursor is. An editor may give
    /// the key its ordinary meaning instead.
    NotHere(String),
    /// The command applies here but cannot be carried out.
    Failed(String),
}

impl From<String> for Refusal {
    fn from(message: String) -> Self {
        Refusal::Failed(message)
    }
}

impl From<&str> for Refusal {
    fn from(message: &str) -> Self {
        Refusal::Failed(message.to_string())
    }
}

impl Refusal {
    pub fn message(&self) -> &str {
        match self {
            Refusal::NotHere(message) | Refusal::Failed(message) => message,
        }
    }
}
