// Logic layer — pure Rust, no GTK dependency.
//
// Every module here operates on data and returns data; it never touches a
// widget. That keeps the whole layer unit-testable with `cargo test` in a
// headless environment.

pub mod repository;
pub mod blame;
pub mod branch;
pub mod commit_ops;
pub mod conflict;
pub mod diff;
pub mod gitignore;
pub mod merge;
pub mod rebase;
pub mod remote;
pub mod staging;
pub mod stash;
pub mod submodules;
pub mod tags;
pub mod workspace;
pub mod worktrees;

pub mod config;
pub mod undo;
pub mod external_editor;
pub mod logging;
