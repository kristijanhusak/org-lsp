//! Org mode logic on top of the parser.
//!
//! `org-syntax` answers "what is written here"; this crate answers "what
//! does it mean": which day a timestamp falls on, what an entry inherits
//! from its parents, what the agenda shows. Nothing here knows about LSP.

pub mod agenda;
pub mod date;
pub mod outline;
pub mod text;
pub mod timestr;
