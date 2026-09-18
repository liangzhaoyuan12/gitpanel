//! Lightweight i18n — all translations live in this single file.
//!
//! Usage: call `crate::i18n::t(Key::Whatever)` to get the current-locale string.

use std::cell::Cell;
use std::collections::HashMap;

/// Current application language, set once at startup.
static CURRENT_LANG: std::sync::LazyLock<Language> = std::sync::LazyLock::new(|| {
    Language::detect()
});

/// Global runtime language override — stores a `Language` discriminant as a
/// `u8`. Using a global atomic ensures background threads (e.g. git commands)
/// see the same language as the UI thread.
static LANG_OVERRIDE: std::sync::OnceLock<AtomicU8> = std::sync::OnceLock::new();

use std::sync::atomic::{AtomicU8, Ordering};

fn lang_override_store() -> &'static AtomicU8 {
    LANG_OVERRIDE.get_or_init(|| AtomicU8::new(0))
}

/// Language discriminant stored as u8 (0 = None/auto-detect).
fn lang_override_u8() -> Option<Language> {
    let v = lang_override_store().load(Ordering::Relaxed);
    if v == 0 {
        None
    } else {
        Some(Language::from_index(v as u32 - 1))
    }
}

fn set_lang_override_u8(lang: Language) {
    lang_override_store().store((lang.index() + 1) as u8, Ordering::Relaxed);
}

/// All translatable keys used throughout the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(non_camel_case_types)]
pub enum Key {
    // === Preferences ===
    pref_general,
    pref_display,
    pref_date_format,
    pref_commit_files_limit,
    pref_commit_files_limit_sub,
    pref_sidebar_items_limit,
    pref_sidebar_items_limit_sub,
    pref_updates,
    pref_auto_refresh,
    pref_auto_refresh_sub,
    pref_refresh_now,
    pref_refresh_now_sub,
    pref_external_tools,
    pref_external_tools_desc,
    pref_open_with,
    pref_open_with_sub,
    pref_custom_command,
    pref_not_configured,
    pref_custom_command_option,
    pref_language,
    pref_lang_system,
    pref_lang_zh_cn,
    pref_lang_en,

    // === Main window ===
    app_name,
    btn_fetch,
    btn_pull,
    btn_push,
    btn_menu,
    btn_toggle_tree,
    btn_open_workspace,
    btn_toggle_branches,
    btn_search_commits,
    btn_sync,
    btn_stash,
    branch_label_default,
    open_folder_hint,
    no_commits_yet,
    select_repo_hint,
    branches_and_tags,
    close_panel,
    stash_list,
    no_stashes,
    no_repo_selected,
    copy_message,
    checkout_this_commit,
    cherry_pick,
    revert_commit,
    reset_soft,
    reset_mixed,
    reset_hard,
    create_tag_ellipsis,
    export_patch_ellipsis,
    bisect_start_title,
    bisect_start_btn,
    bisect_desc,
    bisect_bad_ref,
    bisect_good_ref,
    bisect_bad,
    bisect_good,
    bisect_skip,
    bisect_reset,
    bisect_continue,
    bisect_abort,
    bisecting,
    export_graph_title,
    select_patch_file,
    save_patch_as,
    export_archive,
    remotes_label,
    copy_sha,

    // === Menu items ===
    menu_recent,
    menu_clone_repo,
    menu_fetch,
    menu_fetch_from,
    menu_pull,
    menu_pull_from,
    menu_push,
    menu_push_to,
    menu_push_all,
    menu_force_push,
    menu_force_push_to,
    menu_force_push_all,
    menu_manage_remotes,
    menu_stash,
    menu_compare_branches,
    menu_reflog,
    menu_bisect,
    menu_apply_patch,
    menu_edit_gitignore,
    menu_export_graph,
    menu_add_license,
    menu_remote,
    menu_tools,
    menu_preferences,
    menu_logs,
    menu_about,
    menu_open_in_editor,
    menu_interactive_rebase,

    // === Dialog strings ===
    dialog_error,
    dialog_cancel,
    dialog_delete_branch,
    dialog_delete_tag,
    dialog_delete_remote_tag,
    dialog_discard_changes,
    dialog_restore_file,
    dialog_revert_commit,
    dialog_move_to_trash,
    dialog_push_rejected,
    dialog_push_rejected_hint,
    dialog_cannot_delete_current,
    dialog_force_delete_hint,
    dialog_create_tag,
    dialog_enter_branch_name,
    dialog_rename_branch,
    dialog_nothing_to_commit,
    dialog_commit_type,

    // === Clone dialog ===
    clone_title,
    clone_url,
    clone_dest,
    clone_desc,
    clone_btn,
    clone_cloning,
    clone_cloned_to,

    // === Remotes dialog ===
    remotes_title,
    remotes_existing,
    remotes_add,
    remotes_no_configured,
    remotes_name,
    remotes_url,
    remotes_edit_url,

    // === Branch compare ===
    compare_title,
    compare_refs,
    compare_base,
    compare_target,
    compare_btn,
    compare_no_diff,

    // === Reflog dialog ===
    reflog_title,
    reflog_no_entries,

    // === File history dialog ===
    file_history_no_history,
    file_history_restore,

    // === Staging / Changes ===
    stage_all,
    unstage_all,
    trash_all,
    trash_all_tooltip,
    commit_message_placeholder,
    commit_btn,
    changes,
    no_changes,
    staged_changes,
    unstaged_changes,
    amend,
    allow_empty,
    allow_empty_tooltip,
    select_lines,
    hide_lines,
    stage_selected_lines,
    unstage_selected_lines,
    stage_hunk,
    unstage_hunk,
    stage_file,
    unstage_file,
    discard,
    discard_tooltip,
    blame,
    file_history,
    show_diff,
    side_by_side,

    // === File status kinds ===
    status_added,
    status_modified,
    status_deleted,
    status_renamed,
    status_typechange,

    // === Branches/Tags panel ===
    branches_local,
    branches_remote,
    tags_section,
    stashes_section,
    submodules_section,
    worktrees_section,
    new_branch,
    new_branch_tooltip,
    filter_placeholder,
    show_all,
    show_less,
    stash_apply,
    stash_drop,
    stash_pop,
    stash_update,
    submodule_open,
    not_init,

    // === Conflict editor ===
    conflict_mark_resolved,
    conflict_accept_ours,
    conflict_accept_theirs,

    // === Gitignore editor ===
    gitignore_save,

    // === Rebase editor ===
    rebase_title,
    rebase_start,

    // === Commit templates ===
    template_add_coauthor,
    template_add_trailer,

    // === Commit list ===
    load_more_commits,
    no_file_changes,

    // === Commit diff dialog ===
    commit_diff_title,

    // === Log viewer ===
    logs_title,
    logs_copy,
    logs_copied,
    logs_copy_desc,

    // === Header chrome ===
    open_in,

    // === Toast / status ===
    toast_push_rejected,
    toast_remote_has_new,

    // === Binary file ===
    binary_diff_not_supported,
}

/// Format strings — runtime interpolation. Call `fmt_t(FmtKey::Xxx(...))`.
pub enum FmtKey<'a> {
    toast_added_remote(&'a str),
    toast_removed_remote(&'a str),
    toast_updated_url(&'a str),
    toast_cloned(&'a str),
    toast_fetched(&'a str),
    toast_pulled(&'a str),
    toast_pushed(&'a str),
    toast_pushed_tag(&'a str),
    toast_deleted_branch(&'a str),
    toast_deleted_tag(&'a str),
    toast_deleted_remote_tag(&'a str),
    toast_stash_applied(usize),
    toast_stash_dropped(usize),
    toast_reset(&'a str),
    toast_switched(&'a str),
    toast_created_tag(&'a str),
    toast_renamed_branch(&'a str, &'a str),
    toast_restored(&'a str, &'a str),
    toast_resolved(&'a str),
    toast_archive_saved(&'a str),
    toast_patch_saved(&'a str),
    toast_applied_patch(&'a str),
    toast_applied_patch_no_commit(&'a str),
    toast_graph_saved(&'a str),
    error_generic(&'a str),
    error_failed_git(&'a str),
    error_failed_open_repo(&'a str),
    error_failed_open_workspace(&'a str),
    error_failed_rebase(&'a str),
    error_failed_stage_hunk(&'a str),
    error_failed_unstage_hunk(&'a str),
    error_failed_write_patch(&'a str),
    error_blame_failed(&'a str),
    error_file_history_failed(&'a str),
    error_git_am(&'a str, &'a str),
    title_failed(&'a str),
    open_in(&'a str),
    stage_hunk_n(usize),
    unstage_hunk_n(usize),
    show_all_n(usize),
    delete_branch_confirm(&'a str),
    bisecting_status(&'a str),
    copy_sha(&'a str),
    commit_files_staged_unstaged(usize, usize),
    dialog_nothing_to_commit_staged(&'a str, &'a str),
    dialog_nothing_to_commit_empty(usize),
}

impl<'a> FmtKey<'a> {
    pub fn render(&self) -> String {
        match self {
            FmtKey::toast_added_remote(r) => format!("Added remote '{}'", r),
            FmtKey::toast_removed_remote(r) => format!("Removed remote '{}'", r),
            FmtKey::toast_updated_url(r) => format!("Updated URL for '{}'", r),
            FmtKey::toast_cloned(p) => format!("Cloned to {}", p),
            FmtKey::toast_fetched(r) => format!("Fetched from {}", r),
            FmtKey::toast_pulled(r) => format!("Pulled from {}", r),
            FmtKey::toast_pushed(r) => format!("Pushed to {}", r),
            FmtKey::toast_pushed_tag(t) => format!("Pushed tag '{}'", t),
            FmtKey::toast_deleted_branch(b) => format!("Deleted branch '{}'", b),
            FmtKey::toast_deleted_tag(t) => format!("Deleted tag '{}'", t),
            FmtKey::toast_deleted_remote_tag(t) => format!("Deleted remote tag '{}'", t),
            FmtKey::toast_stash_applied(i) => format!("Applied stash@{{{}}}", i),
            FmtKey::toast_stash_dropped(i) => format!("Dropped stash@{{{}}}", i),
            FmtKey::toast_reset(desc) => format!("Reset {}", desc),
            FmtKey::toast_switched(b) => format!("Switched to {}", b),
            FmtKey::toast_created_tag(t) => format!("Created tag '{}'", t),
            FmtKey::toast_renamed_branch(o, n) => format!("Renamed '{}' \u{2192} '{}'", o, n),
            FmtKey::toast_restored(f, c) => format!("Restored '{}' from {}", f, c),
            FmtKey::toast_resolved(f) => format!("Resolved: {}", f),
            FmtKey::toast_archive_saved(p) => format!("Archive saved: {}", p),
            FmtKey::toast_patch_saved(p) => format!("Patch saved: {}", p),
            FmtKey::toast_applied_patch(f) => format!("Applied patch: {}", f),
            FmtKey::toast_applied_patch_no_commit(f) => format!("Applied patch (no commit): {}", f),
            FmtKey::toast_graph_saved(p) => format!("Graph saved: {}", p),
            FmtKey::error_generic(e) => format!("Error: {}", e),
            FmtKey::error_failed_git(e) => format!("Failed to run git: {}", e),
            FmtKey::error_failed_open_repo(e) => format!("Failed to open repository:\n{}", e),
            FmtKey::error_failed_open_workspace(e) => format!("Failed to open workspace:\n{}", e),
            FmtKey::error_failed_rebase(e) => format!("Failed to prepare rebase: {}", e),
            FmtKey::error_failed_stage_hunk(e) => format!("Failed to stage hunk: {}", e),
            FmtKey::error_failed_unstage_hunk(e) => format!("Failed to unstage hunk: {}", e),
            FmtKey::error_failed_write_patch(e) => format!("Failed to write patch: {}", e),
            FmtKey::error_blame_failed(e) => format!("Blame failed: {}", e),
            FmtKey::error_file_history_failed(e) => format!("File history failed: {}", e),
            FmtKey::error_git_am(am, ap) => format!("git am: {}\n\ngit apply: {}", am, ap),
            FmtKey::title_failed(t) => format!("{} Failed", t),
            FmtKey::open_in(name) => format!("Open in {}", name),
            FmtKey::stage_hunk_n(i) => format!("Stage Hunk {}", i),
            FmtKey::unstage_hunk_n(i) => format!("Unstage Hunk {}", i),
            FmtKey::show_all_n(n) => format!("Show all ({} more)", n),
            FmtKey::delete_branch_confirm(b) => format!("Delete branch '{}'? This cannot be undone.", b),
            FmtKey::bisecting_status(msg) => format!("Bisecting \u{2014} {}", msg),
            FmtKey::copy_sha(sha) => format!("Copy SHA ({})", sha),
            FmtKey::commit_files_staged_unstaged(s, u) => format!("Changes \u{2014} {} staged / {} unstaged", s, u),
            FmtKey::dialog_nothing_to_commit_staged(s, u) => {
                format!("No files are staged. Stage changes first, or tick 'Allow empty' to commit anyway.\n\n{} staged, {} unstaged", s, u)
            }
            FmtKey::dialog_nothing_to_commit_empty(staged) => format!("Nothing to commit\n{} staged", staged),
        }
    }
}

// ========== English ==========
static EN: std::sync::LazyLock<HashMap<Key, &'static str>> = std::sync::LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(Key::pref_general, "General");
    m.insert(Key::pref_display, "Display");
    m.insert(Key::pref_date_format, "Date Format");
    m.insert(Key::pref_commit_files_limit, "Commit files limit");
    m.insert(Key::pref_commit_files_limit_sub, "0 = show all files");
    m.insert(Key::pref_sidebar_items_limit, "Sidebar items limit");
    m.insert(Key::pref_sidebar_items_limit_sub, "0 = show all items per section");
    m.insert(Key::pref_updates, "Updates");
    m.insert(Key::pref_auto_refresh, "Auto-refresh interval (seconds)");
    m.insert(Key::pref_auto_refresh_sub, "0 = disabled");
    m.insert(Key::pref_refresh_now, "Refresh Now");
    m.insert(Key::pref_refresh_now_sub, "Force an immediate refresh");
    m.insert(Key::pref_external_tools, "External Tools");
    m.insert(Key::pref_external_tools_desc, "Open the current repository in an editor or IDE");
    m.insert(Key::pref_open_with, "Open with");
    m.insert(Key::pref_open_with_sub, "Asks the system which application to use");
    m.insert(Key::pref_custom_command, "Custom command");
    m.insert(Key::pref_not_configured, "Not configured");
    m.insert(Key::pref_custom_command_option, "Custom command\u{2026}");
    m.insert(Key::pref_language, "Language");
    m.insert(Key::pref_lang_system, "System Default");
    m.insert(Key::pref_lang_zh_cn, "Simplified Chinese");
    m.insert(Key::pref_lang_en, "English");

    m.insert(Key::app_name, "Gitpanel");
    m.insert(Key::btn_fetch, "Fetch");
    m.insert(Key::btn_pull, "Pull");
    m.insert(Key::btn_push, "Push");
    m.insert(Key::btn_menu, "Menu");
    m.insert(Key::btn_toggle_tree, "Toggle Repository Tree");
    m.insert(Key::btn_open_workspace, "Open Workspace / Repository");
    m.insert(Key::btn_toggle_branches, "Toggle Branches/Tags Panel");
    m.insert(Key::btn_search_commits, "Search Commits (Ctrl+F)");
    m.insert(Key::btn_sync, "Sync (Fetch / Pull / Push)");
    m.insert(Key::btn_stash, "Stash (Ctrl+Z)");
    m.insert(Key::branch_label_default, "\u{2014}");
    m.insert(Key::open_folder_hint, "Open a folder to browse repos");
    m.insert(Key::no_commits_yet, "No commits yet");
    m.insert(Key::select_repo_hint, "Select a repository to view the graph");
    m.insert(Key::branches_and_tags, "Branches & Tags");
    m.insert(Key::close_panel, "Close panel");
    m.insert(Key::stash_list, "Stash List");
    m.insert(Key::no_stashes, "No stashes");
    m.insert(Key::no_repo_selected, "No repository selected");
    m.insert(Key::copy_message, "Copy Message");
    m.insert(Key::checkout_this_commit, "Checkout This Commit");
    m.insert(Key::cherry_pick, "Cherry-pick");
    m.insert(Key::revert_commit, "Revert Commit");
    m.insert(Key::reset_soft, "Reset Soft to Here");
    m.insert(Key::reset_mixed, "Reset Mixed to Here");
    m.insert(Key::reset_hard, "Reset Hard to Here");
    m.insert(Key::create_tag_ellipsis, "Create Tag\u{2026}");
    m.insert(Key::export_patch_ellipsis, "Export as Patch\u{2026}");
    m.insert(Key::bisect_start_title, "Start Bisect");
    m.insert(Key::bisect_start_btn, "Start");
    m.insert(Key::bisect_desc, "Mark a known-bad commit (defaults to HEAD) and a known-good ancestor. Git will then check out the middle commit.");
    m.insert(Key::bisect_bad_ref, "Bad ref (broken)");
    m.insert(Key::bisect_good_ref, "Good ref (works)");
    m.insert(Key::bisect_bad, "Bad");
    m.insert(Key::bisect_good, "Good");
    m.insert(Key::bisect_skip, "Skip");
    m.insert(Key::bisect_reset, "Reset");
    m.insert(Key::bisect_continue, "Continue");
    m.insert(Key::bisect_abort, "Abort");
    m.insert(Key::bisecting, "Bisecting");
    m.insert(Key::export_graph_title, "Export graph as PNG");
    m.insert(Key::select_patch_file, "Select patch file to apply");
    m.insert(Key::save_patch_as, "Save patch as");
    m.insert(Key::export_archive, "Export archive");
    m.insert(Key::remotes_label, "Remotes:");
    m.insert(Key::copy_sha, "Copy SHA");

    m.insert(Key::menu_recent, "Recent");
    m.insert(Key::menu_clone_repo, "Clone Repository\u{2026}");
    m.insert(Key::menu_fetch, "Fetch");
    m.insert(Key::menu_fetch_from, "Fetch from\u{2026}");
    m.insert(Key::menu_pull, "Pull");
    m.insert(Key::menu_pull_from, "Pull from\u{2026}");
    m.insert(Key::menu_push, "Push");
    m.insert(Key::menu_push_to, "Push to\u{2026}");
    m.insert(Key::menu_push_all, "Push to all remotes\u{2026}");
    m.insert(Key::menu_force_push, "Force Push");
    m.insert(Key::menu_force_push_to, "Force Push to\u{2026}");
    m.insert(Key::menu_force_push_all, "Force Push to all remotes\u{2026}");
    m.insert(Key::menu_manage_remotes, "Manage Remotes\u{2026}");
    m.insert(Key::menu_stash, "Stash");
    m.insert(Key::menu_compare_branches, "Compare Branches\u{2026}");
    m.insert(Key::menu_reflog, "Reflog");
    m.insert(Key::menu_bisect, "Start Bisect\u{2026}");
    m.insert(Key::menu_apply_patch, "Apply Patch\u{2026}");
    m.insert(Key::menu_edit_gitignore, "Edit .gitignore");
    m.insert(Key::menu_export_graph, "Export Graph as PNG\u{2026}");
    m.insert(Key::menu_add_license, "Add License\u{2026}");
    m.insert(Key::menu_remote, "Remote");
    m.insert(Key::menu_tools, "Tools");
    m.insert(Key::menu_preferences, "Preferences");
    m.insert(Key::menu_logs, "Logs");
    m.insert(Key::menu_about, "About Gitpanel");
    m.insert(Key::menu_open_in_editor, "Open in editor");
    m.insert(Key::menu_interactive_rebase, "Interactive Rebase\u{2026}");

    m.insert(Key::dialog_error, "Error");
    m.insert(Key::dialog_cancel, "cancel");
    m.insert(Key::dialog_delete_branch, "Delete Branch?");
    m.insert(Key::dialog_delete_tag, "Delete Tag?");
    m.insert(Key::dialog_delete_remote_tag, "Delete Remote Tag?");
    m.insert(Key::dialog_discard_changes, "Discard Changes?");
    m.insert(Key::dialog_restore_file, "Restore File?");
    m.insert(Key::dialog_revert_commit, "Revert Commit?");
    m.insert(Key::dialog_move_to_trash, "Move All Changes to Trash?");
    m.insert(Key::dialog_push_rejected, "Push Rejected");
    m.insert(Key::dialog_push_rejected_hint, "Remote has new commits. Pull first, then push again.");
    m.insert(Key::dialog_cannot_delete_current, "Cannot delete the current branch");
    m.insert(Key::dialog_force_delete_hint, "Force delete (even if unmerged)");
    m.insert(Key::dialog_create_tag, "Create a new tag on the selected commit:");
    m.insert(Key::dialog_enter_branch_name, "Enter the name for the new branch (created from HEAD):");
    m.insert(Key::dialog_rename_branch, "Rename Branch");
    m.insert(Key::dialog_nothing_to_commit, "Nothing to commit");
    m.insert(Key::dialog_commit_type, "Commit type");

    m.insert(Key::clone_title, "Clone Repository");
    m.insert(Key::clone_url, "Repository URL");
    m.insert(Key::clone_dest, "Destination folder");
    m.insert(Key::clone_desc, "The repository will be cloned into a new folder named after the repo inside the destination.");
    m.insert(Key::clone_btn, "Clone");
    m.insert(Key::clone_cloning, "Cloning");
    m.insert(Key::clone_cloned_to, "Cloned to");

    m.insert(Key::remotes_title, "Remotes");
    m.insert(Key::remotes_existing, "Existing Remotes");
    m.insert(Key::remotes_add, "Add");
    m.insert(Key::remotes_no_configured, "No remotes configured");
    m.insert(Key::remotes_name, "Name");
    m.insert(Key::remotes_url, "URL");
    m.insert(Key::remotes_edit_url, "Edit URL");

    m.insert(Key::compare_title, "Compare Branches");
    m.insert(Key::compare_refs, "Refs");
    m.insert(Key::compare_base, "Base (from)");
    m.insert(Key::compare_target, "Target (to)");
    m.insert(Key::compare_btn, "Compare");
    m.insert(Key::compare_no_diff, "No differences");

    m.insert(Key::reflog_title, "Reflog");
    m.insert(Key::reflog_no_entries, "No reflog entries");

    m.insert(Key::file_history_no_history, "No history");
    m.insert(Key::file_history_restore, "Restore file from this commit");

    m.insert(Key::stage_all, "Stage All");
    m.insert(Key::unstage_all, "Unstage All");
    m.insert(Key::trash_all, "Trash All");
    m.insert(Key::trash_all_tooltip, "Move all changes to Trash (safe rollback)");
    m.insert(Key::commit_message_placeholder, "Commit message");
    m.insert(Key::commit_btn, "Commit");
    m.insert(Key::changes, "Changes");
    m.insert(Key::no_changes, "No changes");
    m.insert(Key::staged_changes, "Staged Changes");
    m.insert(Key::unstaged_changes, "Unstaged Changes");
    m.insert(Key::amend, "Amend");
    m.insert(Key::allow_empty, "Allow empty");
    m.insert(Key::allow_empty_tooltip, "Allow commit when no files are staged");
    m.insert(Key::select_lines, "Select Lines");
    m.insert(Key::hide_lines, "Hide Lines");
    m.insert(Key::stage_selected_lines, "Stage Selected Lines");
    m.insert(Key::unstage_selected_lines, "Unstage Selected Lines");
    m.insert(Key::stage_hunk, "Stage Hunk");
    m.insert(Key::unstage_hunk, "Unstage Hunk");
    m.insert(Key::stage_file, "Stage file");
    m.insert(Key::unstage_file, "Unstage file");
    m.insert(Key::discard, "Discard");
    m.insert(Key::discard_tooltip, "Discard changes");
    m.insert(Key::blame, "Blame");
    m.insert(Key::file_history, "File History");
    m.insert(Key::show_diff, "Show diff");
    m.insert(Key::side_by_side, "Side-by-side");

    m.insert(Key::status_added, "Added");
    m.insert(Key::status_modified, "Modified");
    m.insert(Key::status_deleted, "Deleted");
    m.insert(Key::status_renamed, "Renamed");
    m.insert(Key::status_typechange, "Typechange");

    m.insert(Key::branches_local, "Local");
    m.insert(Key::branches_remote, "Remote");
    m.insert(Key::tags_section, "Tags");
    m.insert(Key::stashes_section, "Stashes");
    m.insert(Key::submodules_section, "Submodules");
    m.insert(Key::worktrees_section, "Worktrees");
    m.insert(Key::new_branch, "New branch");
    m.insert(Key::new_branch_tooltip, "Create branch from HEAD");
    m.insert(Key::filter_placeholder, "Filter\u{2026}");
    m.insert(Key::show_all, "Show all");
    m.insert(Key::show_less, "Show less");
    m.insert(Key::stash_apply, "Apply");
    m.insert(Key::stash_drop, "Drop");
    m.insert(Key::stash_pop, "Pop");
    m.insert(Key::stash_update, "Update");
    m.insert(Key::submodule_open, "Open");
    m.insert(Key::not_init, "not init");

    m.insert(Key::conflict_mark_resolved, "Mark Resolved");
    m.insert(Key::conflict_accept_ours, "Accept All Ours");
    m.insert(Key::conflict_accept_theirs, "Accept All Theirs");

    m.insert(Key::gitignore_save, "Save");

    m.insert(Key::rebase_title, "Interactive Rebase");
    m.insert(Key::rebase_start, "Start Rebase");

    m.insert(Key::template_add_coauthor, "Add Co-Author");
    m.insert(Key::template_add_trailer, "Add Trailer");

    m.insert(Key::load_more_commits, "Load more commits...");
    m.insert(Key::no_file_changes, "No file changes");

    m.insert(Key::commit_diff_title, "Commit Diff");

    m.insert(Key::logs_title, "Logs");
    m.insert(Key::logs_copy, "Copy");
    m.insert(Key::logs_copied, "Copied!");
    m.insert(Key::logs_copy_desc, "Copy the log to the clipboard");

    m.insert(Key::open_in, "Open in");

    m.insert(Key::toast_push_rejected, "Push Rejected");
    m.insert(Key::toast_remote_has_new, "Remote has new commits. Pull first, then push again.");

    m.insert(Key::binary_diff_not_supported, "Binary diff is not supported");

    m
});

// ========== Simplified Chinese ==========
static ZH_CN: std::sync::LazyLock<HashMap<Key, &'static str>> =
    std::sync::LazyLock::new(|| {
        let mut m = HashMap::new();
        m.insert(Key::pref_general, "通用");
        m.insert(Key::pref_display, "显示");
        m.insert(Key::pref_date_format, "日期格式");
        m.insert(Key::pref_commit_files_limit, "提交文件数量限制");
        m.insert(Key::pref_commit_files_limit_sub, "0 = 显示全部文件");
        m.insert(Key::pref_sidebar_items_limit, "侧边栏项目数量限制");
        m.insert(Key::pref_sidebar_items_limit_sub, "0 = 显示每个部分的全部项目");
        m.insert(Key::pref_updates, "更新");
        m.insert(Key::pref_auto_refresh, "自动刷新间隔（秒）");
        m.insert(Key::pref_auto_refresh_sub, "0 = 禁用");
        m.insert(Key::pref_refresh_now, "立即刷新");
        m.insert(Key::pref_refresh_now_sub, "强制立即刷新");
        m.insert(Key::pref_external_tools, "外部工具");
        m.insert(Key::pref_external_tools_desc, "在编辑器或 IDE 中打开当前仓库");
        m.insert(Key::pref_open_with, "打开方式");
        m.insert(Key::pref_open_with_sub, "向系统询问使用哪个应用");
        m.insert(Key::pref_custom_command, "自定义命令");
        m.insert(Key::pref_not_configured, "未配置");
        m.insert(Key::pref_custom_command_option, "自定义命令\u{2026}");
        m.insert(Key::pref_language, "语言");
        m.insert(Key::pref_lang_system, "系统默认");
        m.insert(Key::pref_lang_zh_cn, "简体中文");
        m.insert(Key::pref_lang_en, "英文");

        m.insert(Key::app_name, "Gitpanel");
        m.insert(Key::btn_fetch, "获取");
        m.insert(Key::btn_pull, "拉取");
        m.insert(Key::btn_push, "推送");
        m.insert(Key::btn_menu, "菜单");
        m.insert(Key::btn_toggle_tree, "切换仓库树形视图");
        m.insert(Key::btn_open_workspace, "打开工作区 / 仓库");
        m.insert(Key::btn_toggle_branches, "切换分支/标签面板");
        m.insert(Key::btn_search_commits, "搜索提交 (Ctrl+F)");
        m.insert(Key::btn_sync, "同步 (获取 / 拉取 / 推送)");
        m.insert(Key::btn_stash, "储藏 (Ctrl+Z)");
        m.insert(Key::branch_label_default, "\u{2014}");
        m.insert(Key::open_folder_hint, "打开文件夹浏览仓库");
        m.insert(Key::no_commits_yet, "尚无提交记录");
        m.insert(Key::select_repo_hint, "选择一个仓库查看图表");
        m.insert(Key::branches_and_tags, "分支 & 标签");
        m.insert(Key::close_panel, "关闭面板");
        m.insert(Key::stash_list, "储藏列表");
        m.insert(Key::no_stashes, "无储藏");
        m.insert(Key::no_repo_selected, "未选择仓库");
        m.insert(Key::copy_message, "复制提交信息");
        m.insert(Key::checkout_this_commit, "切换到此提交");
        m.insert(Key::cherry_pick, "选择提交");
        m.insert(Key::revert_commit, "还原提交");
        m.insert(Key::reset_soft, "软重置到此");
        m.insert(Key::reset_mixed, "混合重置到此");
        m.insert(Key::reset_hard, "硬重置到此");
        m.insert(Key::create_tag_ellipsis, "创建标签\u{2026}");
        m.insert(Key::export_patch_ellipsis, "导出为补丁\u{2026}");
        m.insert(Key::bisect_start_title, "开始二分查找");
        m.insert(Key::bisect_start_btn, "开始");
        m.insert(Key::bisect_desc, "标记一个已知坏的提交（默认为 HEAD）和一个已知好的祖先提交。Git 将检出中间提交。");
        m.insert(Key::bisect_bad_ref, "坏引用（已损坏）");
        m.insert(Key::bisect_good_ref, "好引用（可用）");
        m.insert(Key::bisect_bad, "坏");
        m.insert(Key::bisect_good, "好");
        m.insert(Key::bisect_skip, "跳过");
        m.insert(Key::bisect_reset, "重置");
        m.insert(Key::bisect_continue, "继续");
        m.insert(Key::bisect_abort, "中止");
        m.insert(Key::bisecting, "二分查找中");
        m.insert(Key::export_graph_title, "导出图表为 PNG");
        m.insert(Key::select_patch_file, "选择要应用的补丁文件");
        m.insert(Key::save_patch_as, "将补丁保存为");
        m.insert(Key::export_archive, "导出归档");
        m.insert(Key::remotes_label, "远程仓库:");
        m.insert(Key::copy_sha, "复制 SHA");

        m.insert(Key::menu_recent, "最近使用");
        m.insert(Key::menu_clone_repo, "克隆仓库\u{2026}");
        m.insert(Key::menu_fetch, "获取");
        m.insert(Key::menu_fetch_from, "从\u{2026}获取");
        m.insert(Key::menu_pull, "拉取");
        m.insert(Key::menu_pull_from, "从\u{2026}拉取");
        m.insert(Key::menu_push, "推送");
        m.insert(Key::menu_push_to, "推送到\u{2026}");
        m.insert(Key::menu_push_all, "推送到所有远程仓库\u{2026}");
        m.insert(Key::menu_force_push, "强制推送");
        m.insert(Key::menu_force_push_to, "强制推送到\u{2026}");
        m.insert(Key::menu_force_push_all, "强制推送到所有远程仓库\u{2026}");
        m.insert(Key::menu_manage_remotes, "管理远程仓库\u{2026}");
        m.insert(Key::menu_stash, "储藏");
        m.insert(Key::menu_compare_branches, "比较分支\u{2026}");
        m.insert(Key::menu_reflog, "引用日志");
        m.insert(Key::menu_bisect, "开始二分查找\u{2026}");
        m.insert(Key::menu_apply_patch, "应用补丁\u{2026}");
        m.insert(Key::menu_edit_gitignore, "编辑 .gitignore");
        m.insert(Key::menu_export_graph, "导出图表为 PNG\u{2026}");
        m.insert(Key::menu_add_license, "添加许可证\u{2026}");
        m.insert(Key::menu_remote, "远程仓库");
        m.insert(Key::menu_tools, "工具");
        m.insert(Key::menu_preferences, "偏好设置");
        m.insert(Key::menu_logs, "日志");
        m.insert(Key::menu_about, "关于 Gitpanel");
        m.insert(Key::menu_open_in_editor, "在编辑器中打开");
        m.insert(Key::menu_interactive_rebase, "交互式变基\u{2026}");

        m.insert(Key::dialog_error, "错误");
        m.insert(Key::dialog_cancel, "取消");
        m.insert(Key::dialog_delete_branch, "删除分支?");
        m.insert(Key::dialog_delete_tag, "删除标签?");
        m.insert(Key::dialog_delete_remote_tag, "删除远程标签?");
        m.insert(Key::dialog_discard_changes, "放弃更改?");
        m.insert(Key::dialog_restore_file, "恢复文件?");
        m.insert(Key::dialog_revert_commit, "还原提交?");
        m.insert(Key::dialog_move_to_trash, "将所有更改移到回收站?");
        m.insert(Key::dialog_push_rejected, "推送被拒绝");
        m.insert(Key::dialog_push_rejected_hint, "远程仓库有新提交。请先拉取再推送。");
        m.insert(Key::dialog_cannot_delete_current, "无法删除当前分支");
        m.insert(Key::dialog_force_delete_hint, "强制删除（即使未合并）");
        m.insert(Key::dialog_create_tag, "在选定的提交上创建新标签:");
        m.insert(Key::dialog_enter_branch_name, "输入新分支的名称（从 HEAD 创建）:");
        m.insert(Key::dialog_rename_branch, "重命名分支");
        m.insert(Key::dialog_nothing_to_commit, "没有可提交的内容");
        m.insert(Key::dialog_commit_type, "提交类型");

        m.insert(Key::clone_title, "克隆仓库");
        m.insert(Key::clone_url, "仓库 URL");
        m.insert(Key::clone_dest, "目标文件夹");
        m.insert(Key::clone_desc, "仓库将被克隆到目标位置内以仓库名命名的新文件夹中。");
        m.insert(Key::clone_btn, "克隆");
        m.insert(Key::clone_cloning, "正在克隆");
        m.insert(Key::clone_cloned_to, "已克隆到");

        m.insert(Key::remotes_title, "远程仓库");
        m.insert(Key::remotes_existing, "现有远程仓库");
        m.insert(Key::remotes_add, "添加");
        m.insert(Key::remotes_no_configured, "未配置远程仓库");
        m.insert(Key::remotes_name, "名称");
        m.insert(Key::remotes_url, "URL");
        m.insert(Key::remotes_edit_url, "编辑 URL");

        m.insert(Key::compare_title, "比较分支");
        m.insert(Key::compare_refs, "引用");
        m.insert(Key::compare_base, "基础（从）");
        m.insert(Key::compare_target, "目标（到）");
        m.insert(Key::compare_btn, "比较");
        m.insert(Key::compare_no_diff, "无差异");

        m.insert(Key::reflog_title, "引用日志");
        m.insert(Key::reflog_no_entries, "无引用日志条目");

        m.insert(Key::file_history_no_history, "无历史记录");
        m.insert(Key::file_history_restore, "从此提交恢复文件");

        m.insert(Key::stage_all, "全部暂存");
        m.insert(Key::unstage_all, "全部取消暂存");
        m.insert(Key::trash_all, "全部移入回收站");
        m.insert(Key::trash_all_tooltip, "将所有更改移到回收站（安全回滚）");
        m.insert(Key::commit_message_placeholder, "提交信息");
        m.insert(Key::commit_btn, "提交");
        m.insert(Key::changes, "更改");
        m.insert(Key::no_changes, "无更改");
        m.insert(Key::staged_changes, "已暂存的更改");
        m.insert(Key::unstaged_changes, "未暂存的更改");
        m.insert(Key::amend, "修改提交");
        m.insert(Key::allow_empty, "允许空提交");
        m.insert(Key::allow_empty_tooltip, "在没有已暂存文件时允许提交");
        m.insert(Key::select_lines, "选择行");
        m.insert(Key::hide_lines, "隐藏行");
        m.insert(Key::stage_selected_lines, "暂存选中行");
        m.insert(Key::unstage_selected_lines, "取消暂存选中行");
        m.insert(Key::stage_hunk, "暂存片段");
        m.insert(Key::unstage_hunk, "取消暂存片段");
        m.insert(Key::stage_file, "暂存文件");
        m.insert(Key::unstage_file, "取消暂存文件");
        m.insert(Key::discard, "放弃");
        m.insert(Key::discard_tooltip, "放弃更改");
        m.insert(Key::blame, "追踪");
        m.insert(Key::file_history, "文件历史");
        m.insert(Key::show_diff, "显示差异");
        m.insert(Key::side_by_side, "并排显示");

        m.insert(Key::status_added, "新增");
        m.insert(Key::status_modified, "已修改");
        m.insert(Key::status_deleted, "已删除");
        m.insert(Key::status_renamed, "已重命名");
        m.insert(Key::status_typechange, "类型变更");

        m.insert(Key::branches_local, "本地");
        m.insert(Key::branches_remote, "远程");
        m.insert(Key::tags_section, "标签");
        m.insert(Key::stashes_section, "储藏");
        m.insert(Key::submodules_section, "子模块");
        m.insert(Key::worktrees_section, "工作树");
        m.insert(Key::new_branch, "新建分支");
        m.insert(Key::new_branch_tooltip, "从 HEAD 创建分支");
        m.insert(Key::filter_placeholder, "筛选\u{2026}");
        m.insert(Key::show_all, "显示全部");
        m.insert(Key::show_less, "收起");
        m.insert(Key::stash_apply, "应用");
        m.insert(Key::stash_drop, "删除");
        m.insert(Key::stash_pop, "恢复");
        m.insert(Key::stash_update, "更新");
        m.insert(Key::submodule_open, "打开");
        m.insert(Key::not_init, "未初始化");

        m.insert(Key::conflict_mark_resolved, "标记已解决");
        m.insert(Key::conflict_accept_ours, "接受全部我方");
        m.insert(Key::conflict_accept_theirs, "接受全部他方");

        m.insert(Key::gitignore_save, "保存");

        m.insert(Key::rebase_title, "交互式变基");
        m.insert(Key::rebase_start, "开始变基");

        m.insert(Key::template_add_coauthor, "添加共同作者");
        m.insert(Key::template_add_trailer, "添加追加信息");

        m.insert(Key::load_more_commits, "加载更多提交...");
        m.insert(Key::no_file_changes, "无文件更改");

        m.insert(Key::commit_diff_title, "提交差异");

        m.insert(Key::logs_title, "日志");
        m.insert(Key::logs_copy, "复制");
        m.insert(Key::logs_copied, "已复制!");
        m.insert(Key::logs_copy_desc, "将日志复制到剪贴板");

        m.insert(Key::open_in, "在中打开");

        m.insert(Key::toast_push_rejected, "推送被拒绝");
        m.insert(Key::toast_remote_has_new, "远程仓库有新提交。请先拉取再推送。");

        m.insert(Key::binary_diff_not_supported, "不支持显示二进制文件差异");

        m
    });

/// Detect system locale and return the appropriate language.
fn detect_system_language() -> Language {
    for var in &["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        if let Ok(val) = std::env::var(var) {
            let lower = val.to_lowercase();
            if lower.starts_with("zh_cn")
                || lower.starts_with("zh_hans")
                || lower.starts_with("zh")
            {
                return Language::ZhCn;
            }
        }
    }
    Language::En
}

/// The application language preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    /// Follow system locale.
    System,
    /// Simplified Chinese.
    ZhCn,
    /// English.
    En,
}

impl Language {
    /// Resolve `System` to the concrete language.
    pub fn resolve(self) -> Self {
        match self {
            Language::System => detect_system_language(),
            other => other,
        }
    }

    /// Detect the system language at startup.
    fn detect() -> Self {
        detect_system_language()
    }

    pub fn index(self) -> u32 {
        match self {
            Language::System => 0,
            Language::ZhCn => 1,
            Language::En => 2,
        }
    }

    pub fn from_index(i: u32) -> Self {
        match i {
            1 => Language::ZhCn,
            2 => Language::En,
            _ => Language::System,
        }
    }
}

impl Default for Language {
    fn default() -> Self {
        Language::System
    }
}

/// Get the currently active language.
fn current_lang() -> Language {
    lang_override_u8().unwrap_or(*CURRENT_LANG)
}

/// Set the runtime language override (called when preferences change).
pub fn set_language(lang: Language) {
    let resolved = lang.resolve();
    set_lang_override_u8(resolved);
}

/// Initialize the language from config (call once at startup).
pub fn init_language(lang: Language) {
    let resolved = lang.resolve();
    set_lang_override_u8(resolved);
}

/// Translate a key to the current locale string.
pub fn t(key: Key) -> &'static str {
    let lang = current_lang();
    match lang {
        Language::ZhCn => ZH_CN
            .get(&key)
            .copied()
            .unwrap_or_else(|| EN.get(&key).copied().unwrap_or("???")),
        _ => EN.get(&key).copied().unwrap_or("???"),
    }
}
