pub mod window;
pub mod changed_file_object;
pub mod commit_list;
pub mod commit_graph;
pub mod commit_object;
pub mod repo_tree;
pub mod branches_tags_panel;
pub mod changes_view;
// Wired into the commits tab by the next commit; until then nothing in the
// binary calls it.
#[allow(dead_code)]
pub mod commit_diff_dialog;
pub mod diff_view;
pub mod header_chrome;
pub mod preferences_dialog;
pub mod gitignore_editor;
pub mod syntax;
pub mod rebase_editor;
pub mod commit_templates;
pub mod blame_view;
pub mod conflict_editor;
pub mod file_history_dialog;
pub mod clone_dialog;
pub mod reflog_dialog;
pub mod remotes_dialog;
pub mod branch_compare_dialog;
pub mod word_diff;
