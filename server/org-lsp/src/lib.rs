//! A language server for Org mode documents.
//!
//! The server speaks standard LSP: any editor with an LSP client gets
//! highlighting (semantic tokens), folding, the outline, TODO state changes
//! and the agenda. What an editor integration still has to know (commands,
//! default keys, default looks) the server announces itself; see
//! `manifest` and `docs/protocol.org` in the repository.

pub mod actions;
pub mod cli;
pub mod commands;
pub mod completion;
pub mod config;
pub mod custom;
pub mod document;
pub mod emacs;
pub mod faces;
pub mod folding;
pub mod highlight;
pub mod legend;
pub mod line_index;
pub mod manifest;
pub mod port;
pub mod process;
pub mod prompt;
pub mod protocol;
pub mod server;
pub mod settings;
pub mod symbols;
pub mod uri;
pub mod views;
pub mod workspace;

pub use server::run;
