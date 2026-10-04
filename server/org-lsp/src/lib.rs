//! A language server for Org mode documents.
//!
//! The server speaks standard LSP only: any editor with an LSP client gets
//! highlighting (semantic tokens), folding, the outline and TODO cycling.
//! See `docs/protocol.md` in the repository for the token legend and the
//! code action kinds editors can bind keys to.

pub mod actions;
pub mod cli;
pub mod config;
pub mod document;
pub mod folding;
pub mod highlight;
pub mod line_index;
pub mod server;
pub mod symbols;

pub use server::run;
