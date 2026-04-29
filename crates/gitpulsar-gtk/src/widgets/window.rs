use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use gitpulsar_core::models::{CommitInfo, DiffFile, RepoStatus, ResetMode, StashEntry, SubmoduleInfo, WorktreeInfo};
use gitpulsar_core::repository::GitRepo;
use gitpulsar_core::workspace::{self, WorkspaceEntry};

use crate::config::AppConfig;
use crate::undo::{UndoStack, UndoableOp};

struct BackgroundRepoData {
    commits: Vec<CommitInfo>,
    tags_map: HashMap<String, Vec<String>>,
    status: Option<RepoStatus>,
    branches: Vec<gitpulsar_core::models::BranchInfo>,
    tags: Vec<gitpulsar_core::models::TagInfo>,
    stash_entries: Vec<StashEntry>,
    submodules: Vec<SubmoduleInfo>,
    worktrees: Vec<WorktreeInfo>,
    ahead: usize,
    behind: usize,
    branch_name: Option<String>,
    unstaged_diffs: Vec<DiffFile>,
    staged_diffs: Vec<DiffFile>,
    has_conflicts: bool,
    is_merging: bool,
    is_rebasing: bool,
}

struct BackgroundRefreshResult {
    status: Option<RepoStatus>,
    status_hash: u64,
    workspace_entries: Option<Vec<WorkspaceEntry>>,
    workspace_hash: u64,
    ahead: usize,
    behind: usize,
    unstaged_diffs: Vec<DiffFile>,
    staged_diffs: Vec<DiffFile>,
}

fn hash_status(status: &RepoStatus) -> u64 {
    let mut hasher = DefaultHasher::new();
    for f in &status.unstaged {
        f.path.hash(&mut hasher);
    }
    for f in &status.staged {
        f.path.hash(&mut hasher);
    }
    for p in &status.untracked {
        p.hash(&mut hasher);
    }
    hasher.finish()
}

fn hash_commits(commits: &[CommitInfo]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for c in commits {
        c.id.hash(&mut hasher);
    }
    hasher.finish()
}

fn hash_workspace(entries: &[WorkspaceEntry]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for e in entries {
        e.name.hash(&mut hasher);
        e.is_git_repo.hash(&mut hasher);
        if let Some(ref ind) = e.indicator {
            ind.is_dirty.hash(&mut hasher);
            ind.has_tracked_changes.hash(&mut hasher);
            ind.ahead.hash(&mut hasher);
        }
    }
    hasher.finish()
}

use super::branches_tags_panel;
use super::changes_view;
use super::commit_list;
use super::blame_view;
use super::conflict_editor;
use super::file_history_dialog;
use super::clone_dialog;
use super::reflog_dialog;
use super::remotes_dialog;
use super::gitignore_editor;
use super::preferences_dialog;
use super::rebase_editor;
use super::repo_tree;

const COMMIT_PAGE_SIZE: usize = 50;

/// Run a git CLI command with a 30-second timeout.
/// Returns stdout on success, or an anyhow error with stderr on failure.
fn run_git_cmd(repo_path: &str, args: &[&str]) -> Result<String, anyhow::Error> {
    use std::process::Command;

    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run git: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout)
    } else {
        let msg = if stderr.trim().is_empty() { stdout } else { stderr };
        anyhow::bail!("{}", msg.trim());
    }
}

mod imp {
    use super::*;

    pub struct GitpulsarWindow {
        pub repo: RefCell<Option<GitRepo>>,
        pub workspace_entries: RefCell<Vec<WorkspaceEntry>>,
        pub commits: RefCell<Vec<CommitInfo>>,
        pub selected_commit_id: RefCell<Option<String>>,
        /// Hash of last status to skip redundant UI updates.
        pub last_status_hash: Cell<u64>,
        /// Hash of last workspace entries to skip redundant UI updates.
        pub last_workspace_hash: Cell<u64>,
        /// Hash of last commit list to skip redundant rebuilds.
        pub last_commits_hash: Cell<u64>,
        /// Guard to prevent concurrent background refreshes.
        pub refresh_in_progress: Cell<bool>,
        /// Tick counter for throttling workspace scans.
        pub refresh_tick: Cell<u32>,
        /// Cached ahead/behind to skip redundant UI updates.
        pub last_ahead: Cell<usize>,
        pub last_behind: Cell<usize>,
        /// Cached unstaged diffs from last refresh.
        pub cached_unstaged_diffs: RefCell<Vec<DiffFile>>,
        /// Cached staged diffs from last refresh.
        pub cached_staged_diffs: RefCell<Vec<DiffFile>>,
        /// Application configuration.
        pub config: RefCell<AppConfig>,
        /// Source ID of the auto-refresh timer (to restart on config change).
        pub refresh_source_id: RefCell<Option<glib::SourceId>>,
        // Layout refs
        pub outer_split: RefCell<Option<adw::OverlaySplitView>>,
        pub inner_split: RefCell<Option<adw::OverlaySplitView>>,
        pub toast_overlay: adw::ToastOverlay,
        // Widget refs
        pub repo_list_box: gtk::ListBox,
        pub commit_list_box: gtk::ListBox,
        pub view_stack: adw::ViewStack,
        pub branch_label: gtk::Label,
        pub ahead_label: gtk::Label,
        pub behind_label: gtk::Label,
        pub commit_entry: gtk::TextView,
        pub commit_button: gtk::Button,
        pub search_entry: gtk::SearchEntry,
        pub amend_check: gtk::CheckButton,
        pub fetch_btn: gtk::Button,
        pub pull_btn: gtk::Button,
        pub push_btn: gtk::Button,
        // Branches/tags panel refs (set during setup_ui)
        pub branches_local_list: RefCell<Option<gtk::ListBox>>,
        pub branches_remote_list: RefCell<Option<gtk::ListBox>>,
        pub tags_list: RefCell<Option<gtk::ListBox>>,
        // Changes view file lists (set during setup_ui)
        pub unstaged_file_list: RefCell<Option<gtk::ListBox>>,
        pub staged_file_list: RefCell<Option<gtk::ListBox>>,
        // Stashes list in right sidebar (set during setup_ui)
        pub stashes_list: RefCell<Option<gtk::ListBox>>,
        pub submodules_list: RefCell<Option<gtk::ListBox>>,
        pub worktrees_list: RefCell<Option<gtk::ListBox>>,
        // Merge/rebase banner
        pub conflict_banner: RefCell<Option<gtk::Box>>,
        // Sidebar header title (folder name)
        pub sidebar_title_label: gtk::Label,
        // Sidebar status
        pub sidebar_repo_name_label: gtk::Label,
        pub sidebar_status_label: gtk::Label,
        // Hamburger menu button (for updating Recent menu)
        pub menu_btn: gtk::MenuButton,
        /// Number of commits currently loaded (for pagination).
        pub commits_loaded_count: Cell<usize>,
        /// Undo/redo stack for staging operations.
        pub undo_stack: RefCell<UndoStack>,
        /// Current panel focus index for Tab cycling.
        pub focus_panel_index: Cell<u8>,
    }

    impl Default for GitpulsarWindow {
        fn default() -> Self {
            Self {
                repo: RefCell::new(None),
                workspace_entries: RefCell::new(Vec::new()),
                commits: RefCell::new(Vec::new()),
                selected_commit_id: RefCell::new(None),
                last_status_hash: Cell::new(0),
                last_workspace_hash: Cell::new(0),
                last_commits_hash: Cell::new(0),
                refresh_in_progress: Cell::new(false),
                refresh_tick: Cell::new(0),
                last_ahead: Cell::new(0),
                last_behind: Cell::new(0),
                cached_unstaged_diffs: RefCell::new(Vec::new()),
                cached_staged_diffs: RefCell::new(Vec::new()),
                config: RefCell::new(AppConfig::load()),
                refresh_source_id: RefCell::new(None),
                outer_split: RefCell::new(None),
                inner_split: RefCell::new(None),
                toast_overlay: adw::ToastOverlay::new(),
                repo_list_box: gtk::ListBox::new(),
                commit_list_box: gtk::ListBox::new(),
                view_stack: adw::ViewStack::new(),
                branch_label: gtk::Label::new(Some("main")),
                ahead_label: gtk::Label::new(Some("▲ 0")),
                behind_label: gtk::Label::new(Some("▼ 0")),
                commit_entry: gtk::TextView::new(),
                commit_button: gtk::Button::new(),
                search_entry: gtk::SearchEntry::builder()
                    .placeholder_text("Search commits")
                    .margin_start(8)
                    .margin_end(8)
                    .margin_top(8)
                    .margin_bottom(4)
                    .build(),
                amend_check: gtk::CheckButton::builder()
                    .label("Amend")
                    .build(),
                fetch_btn: gtk::Button::builder()
                    .icon_name("view-refresh-symbolic")
                    .tooltip_text("Fetch")
                    .build(),
                pull_btn: gtk::Button::builder()
                    .icon_name("go-down-symbolic")
                    .tooltip_text("Pull")
                    .build(),
                push_btn: gtk::Button::builder()
                    .icon_name("go-up-symbolic")
                    .tooltip_text("Push")
                    .build(),
                branches_local_list: RefCell::new(None),
                branches_remote_list: RefCell::new(None),
                tags_list: RefCell::new(None),
                unstaged_file_list: RefCell::new(None),
                staged_file_list: RefCell::new(None),
                stashes_list: RefCell::new(None),
                submodules_list: RefCell::new(None),
                worktrees_list: RefCell::new(None),
                conflict_banner: RefCell::new(None),
                sidebar_title_label: gtk::Label::builder()
                    .label("Gitpulsar")
                    .css_classes(["title"])
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .build(),
                sidebar_repo_name_label: gtk::Label::builder()
                    .label("")
                    .css_classes(["caption"])
                    .xalign(0.0)
                    .build(),
                sidebar_status_label: gtk::Label::builder()
                    .label("")
                    .css_classes(["caption", "dim-label"])
                    .xalign(0.0)
                    .build(),
                menu_btn: gtk::MenuButton::builder()
                    .icon_name("open-menu-symbolic")
                    .tooltip_text("Menu")
                    .build(),
                commits_loaded_count: Cell::new(0),
                undo_stack: RefCell::new(UndoStack::default()),
                focus_panel_index: Cell::new(0),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GitpulsarWindow {
        const NAME: &'static str = "GitpulsarWindow";
        type Type = super::GitpulsarWindow;
        type ParentType = adw::ApplicationWindow;
    }

    impl ObjectImpl for GitpulsarWindow {}
    impl WidgetImpl for GitpulsarWindow {}
    impl WindowImpl for GitpulsarWindow {}
    impl ApplicationWindowImpl for GitpulsarWindow {}
    impl AdwApplicationWindowImpl for GitpulsarWindow {}
}

glib::wrapper! {
    pub struct GitpulsarWindow(ObjectSubclass<imp::GitpulsarWindow>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl GitpulsarWindow {
    pub fn new(app: &adw::Application) -> Self {
        let window: Self = glib::Object::builder()
            .property("application", app)
            .property("title", "Gitpulsar")
            .property("default-width", 1200)
            .property("default-height", 800)
            .property("width-request", 360)
            .property("height-request", 294)
            .build();

        window.setup_ui();
        window.setup_actions();
        window.setup_commit_context_menu();
        window.setup_branch_context_menu();
        window.setup_tag_context_menu();
        window.setup_keyboard_navigation();
        window.setup_auto_refresh();

        // Auto-open last workspace
        let last_workspace = window.imp().config.borrow().recent_workspaces.first().cloned();
        if let Some(path) = last_workspace {
            let win = window.clone();
            glib::idle_add_local_once(move || {
                let p = std::path::PathBuf::from(&path);
                if p.is_dir() {
                    win.open_workspace(&p);
                }
            });
        }

        window
    }

    fn setup_ui(&self) {
        let imp = self.imp();

        // ==========================================
        // SIDEBAR HEADER BAR
        // ==========================================
        let sidebar_header = adw::HeaderBar::new();
        sidebar_header.set_show_end_title_buttons(false);
        sidebar_header.set_show_start_title_buttons(true);
        sidebar_header.set_title_widget(Some(&imp.sidebar_title_label));

        // Hamburger menu button (end of sidebar header)
        self.rebuild_hamburger_menu();
        sidebar_header.pack_end(&imp.menu_btn);

        // ==========================================
        // CONTENT HEADER BAR
        // ==========================================
        let content_header = adw::HeaderBar::new();
        content_header.set_show_start_title_buttons(false);
        content_header.set_show_end_title_buttons(true);
        let content_title = gtk::Label::builder()
            .label("Gitpulsar")
            .css_classes(["title"])
            .build();
        content_header.set_title_widget(Some(&content_title));

        // Content header left: toggle repo tree
        let toggle_repo_tree = gtk::ToggleButton::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text("Toggle Repository Tree")
            .active(true)
            .build();
        content_header.pack_start(&toggle_repo_tree);

        // Content header left: open workspace
        let open_button = gtk::Button::builder()
            .icon_name("folder-open-symbolic")
            .tooltip_text("Open Workspace / Repository")
            .build();
        open_button.set_action_name(Some("win.open-repo"));
        content_header.pack_start(&open_button);

        // fetch / pull / push are in the bottom bar (see below)

        // Connect remote buttons
        let win = self.clone();
        imp.fetch_btn.connect_clicked(move |_| {
            win.on_fetch();
        });
        let win = self.clone();
        imp.pull_btn.connect_clicked(move |_| {
            win.on_pull();
        });
        let win = self.clone();
        imp.push_btn.connect_clicked(move |_| {
            win.on_push(false);
        });

        // Push menu with force-push option
        let push_menu = gio::Menu::new();
        push_menu.append(Some("Force Push"), Some("win.force-push"));

        let push_popover = gtk::PopoverMenu::from_model(Some(&push_menu));
        push_popover.set_parent(&imp.push_btn);

        let push_gesture = gtk::GestureLongPress::new();
        let pp = push_popover.clone();
        push_gesture.connect_pressed(move |_, _, _| {
            pp.popup();
        });
        imp.push_btn.add_controller(push_gesture);

        // Content header right: toggle right sidebar (always visible)
        let toggle_right_panel = gtk::ToggleButton::builder()
            .icon_name("sidebar-show-right-symbolic")
            .tooltip_text("Toggle Branches/Tags Panel")
            .active(true)
            .build();
        content_header.pack_end(&toggle_right_panel);

        // Content header right: search toggle (always visible)
        let search_toggle = gtk::ToggleButton::builder()
            .icon_name("system-search-symbolic")
            .tooltip_text("Search Commits (Ctrl+F)")
            .build();
        content_header.pack_end(&search_toggle);

        // Content header right: branch graph button
        // Graph tab accessible via Ctrl+3 or ViewSwitcher

        // Content header right: branch label
        let branch_content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        imp.branch_label.set_label("—");
        branch_content.append(&gtk::Image::builder()
            .icon_name("branch-fork-symbolic")
            .pixel_size(16)
            .build());
        branch_content.append(&imp.branch_label);
        content_header.pack_end(&branch_content);

        // Indicators (will go into bottom bar right)
        let indicators = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        indicators.set_margin_end(8);
        imp.ahead_label.add_css_class("success");
        imp.ahead_label.add_css_class("caption");
        imp.behind_label.add_css_class("error");
        imp.behind_label.add_css_class("caption");
        indicators.append(&imp.ahead_label);
        indicators.append(&imp.behind_label);

        // ==========================================
        // LEFT SIDEBAR — repo tree (full height with own HeaderBar)
        // ==========================================
        let repo_sidebar_content = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let repo_scrolled = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();

        imp.repo_list_box.set_selection_mode(gtk::SelectionMode::Single);
        imp.repo_list_box.add_css_class("navigation-sidebar");

        let placeholder = gtk::Label::builder()
            .label("Open a folder to browse repos")
            .css_classes(["dim-label"])
            .margin_top(24)
            .margin_bottom(24)
            .build();
        imp.repo_list_box.set_placeholder(Some(&placeholder));

        // Connect repo selection
        let win = self.clone();
        imp.repo_list_box.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                win.select_repo(row.index() as usize);
            }
        });

        repo_scrolled.set_child(Some(&imp.repo_list_box));
        repo_sidebar_content.append(&repo_scrolled);

        // Sidebar status bar (bottom)
        let sidebar_status_bar = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(8)
            .build();
        sidebar_status_bar.append(&imp.sidebar_repo_name_label);
        sidebar_status_bar.append(&imp.sidebar_status_label);
        repo_sidebar_content.append(&sidebar_status_bar);

        // Sidebar ToolbarView: own HeaderBar + repo content
        let sidebar_toolbar = adw::ToolbarView::new();
        sidebar_toolbar.add_top_bar(&sidebar_header);
        sidebar_toolbar.set_top_bar_style(adw::ToolbarStyle::Flat);
        sidebar_toolbar.set_content(Some(&repo_sidebar_content));

        // ==========================================
        // CENTER — ViewStack (Commits / Changes)
        // ==========================================

        // --- Commits page ---
        let commits_page = gtk::Box::new(gtk::Orientation::Vertical, 0);

        // Search bar (toggled from header button)
        let search_bar = gtk::SearchBar::new();
        search_bar.set_child(Some(&imp.search_entry));
        search_bar.set_search_mode(false);
        search_bar.connect_entry(&imp.search_entry);
        // Bind search toggle ↔ search bar
        search_toggle.bind_property("active", &search_bar, "search-mode-enabled")
            .bidirectional()
            .sync_create()
            .build();
        commits_page.append(&search_bar);

        // Connect commit search filter
        let win = self.clone();
        imp.search_entry.connect_search_changed(move |entry| {
            let query = entry.text().to_lowercase();
            let commits = win.imp().commits.borrow();
            let commit_data: Vec<(String, String, String)> = commits
                .iter()
                .map(|c| (c.summary.to_lowercase(), c.author.name.to_lowercase(), c.short_id.to_lowercase()))
                .collect();
            drop(commits);

            let list = &win.imp().commit_list_box;
            list.set_filter_func(move |row| {
                if query.is_empty() {
                    return true;
                }
                let idx = row.index() as usize;
                if let Some((summary, author, short_id)) = commit_data.get(idx) {
                    summary.contains(&query) || author.contains(&query) || short_id.contains(&query)
                } else {
                    true
                }
            });
        });

        let commit_scrolled = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();

        imp.commit_list_box.set_selection_mode(gtk::SelectionMode::Browse);
        imp.commit_list_box.add_css_class("navigation-sidebar");

        let commits_placeholder = gtk::Label::builder()
            .label("Select a repository")
            .css_classes(["dim-label"])
            .margin_top(24)
            .margin_bottom(24)
            .build();
        imp.commit_list_box.set_placeholder(Some(&commits_placeholder));

        // Connect commit activation — toggle expand/collapse detail
        let win = self.clone();
        imp.commit_list_box.connect_row_activated(move |_, row| {
            if row.widget_name() == "load-more-row" {
                win.load_more_commits();
            } else {
                win.on_commit_selected(row.index() as usize);
            }
        });

        commit_scrolled.set_child(Some(&imp.commit_list_box));
        commits_page.append(&commit_scrolled);

        // --- Changes page: file accordion list with inline diffs ---
        let (changes_box, changes_refs) = changes_view::build_changes_view(
            &imp.commit_entry,
            &imp.commit_button,
            &imp.amend_check,
        );

        // Connect commit button
        let win = self.clone();
        imp.commit_button.connect_clicked(move |_| {
            win.on_commit_clicked();
        });

        // Connect amend toggle — fill commit message from HEAD
        let win = self.clone();
        imp.amend_check.connect_toggled(move |check| {
            if check.is_active() {
                let repo_ref = win.imp().repo.borrow();
                if let Some(ref repo) = *repo_ref {
                    if let Ok(msg) = repo.head_commit_message() {
                        let buffer = win.imp().commit_entry.buffer();
                        buffer.set_text(&msg.trim());
                    }
                }
            }
        });

        // Connect Stage All button
        let win = self.clone();
        changes_refs.stage_all_btn.connect_clicked(move |_| {
            win.on_stage_all();
        });

        // Connect Unstage All button
        let win = self.clone();
        changes_refs.unstage_all_btn.connect_clicked(move |_| {
            win.on_unstage_all();
        });

        // Connect file row activation — toggle diff accordion (both lists)
        let win = self.clone();
        changes_refs.unstaged_list_box.connect_row_activated(move |_, row| {
            win.on_changes_file_activated(row);
        });
        let win = self.clone();
        changes_refs.staged_list_box.connect_row_activated(move |_, row| {
            win.on_changes_file_activated(row);
        });

        // Connect per-row stage/unstage/discard buttons (both lists)
        self.setup_changes_row_button_signals(&changes_refs.unstaged_list_box);
        self.setup_changes_row_button_signals(&changes_refs.staged_list_box);

        // DnD drop handlers — drop on unstaged list = unstage, drop on staged = stage
        {
            let win = self.clone();
            let unstaged_list = changes_refs.unstaged_list_box.clone();
            // Find the DropTarget on unstaged list
            for ctrl in unstaged_list.observe_controllers().into_iter() {
                if let Some(ctrl) = ctrl.ok() {
                    if let Ok(dt) = ctrl.downcast::<gtk::DropTarget>() {
                        dt.connect_drop(move |_, value, _, _| {
                            if let Ok(path) = value.get::<String>() {
                                win.unstage_file(&path);
                                return true;
                            }
                            false
                        });
                        break;
                    }
                }
            }
        }
        {
            let win = self.clone();
            let staged_list = changes_refs.staged_list_box.clone();
            for ctrl in staged_list.observe_controllers().into_iter() {
                if let Some(ctrl) = ctrl.ok() {
                    if let Ok(dt) = ctrl.downcast::<gtk::DropTarget>() {
                        dt.connect_drop(move |_, value, _, _| {
                            if let Ok(path) = value.get::<String>() {
                                win.stage_file(&path);
                                return true;
                            }
                            false
                        });
                        break;
                    }
                }
            }
        }

        // Store changes file list refs
        *imp.unstaged_file_list.borrow_mut() = Some(changes_refs.unstaged_list_box.clone());
        *imp.staged_file_list.borrow_mut() = Some(changes_refs.staged_list_box.clone());

        // --- Graph page ---
        let graph_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        graph_page.set_widget_name("graph-page");
        let graph_scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .hexpand(true)
            .build();
        let graph_placeholder = gtk::Label::builder()
            .label("Select a repository to view the graph")
            .css_classes(["dim-label"])
            .margin_top(24)
            .build();
        graph_scrolled.set_child(Some(&graph_placeholder));
        graph_page.append(&graph_scrolled);

        // --- ViewStack setup ---
        imp.view_stack.add_titled_with_icon(&commits_page, Some("commits"), "Commits", "commit-symbolic");
        imp.view_stack.add_titled_with_icon(&changes_box, Some("changes"), "Changes", "branch-compare-symbolic");
        imp.view_stack.add_titled_with_icon(&graph_page, Some("graph"), "Graph", "branch-fork-symbolic");

        // ==========================================
        // RIGHT SIDEBAR — branches & tags panel
        // ==========================================
        let (branches_panel, branches_refs) = branches_tags_panel::build_branches_tags_panel();

        // Connect branch click → checkout
        let win = self.clone();
        branches_refs.local_list.connect_row_activated(move |_, row| {
            let name = row.widget_name().to_string();
            win.on_checkout_branch(&name);
        });

        let win = self.clone();
        branches_refs.remote_list.connect_row_activated(move |_, row| {
            let name = row.widget_name().to_string();
            win.on_checkout_remote_branch(&name);
        });

        // Connect create branch button
        let win = self.clone();
        branches_refs.create_branch_btn.connect_clicked(move |_| {
            win.show_create_branch_dialog();
        });

        let bl_list = branches_refs.local_list.clone();
        let br_list = branches_refs.remote_list.clone();
        let tg_list = branches_refs.tags_list.clone();
        let st_list = branches_refs.stashes_list.clone();
        let sm_list = branches_refs.submodules_list.clone();
        let wt_list = branches_refs.worktrees_list.clone();

        // ==========================================
        // BOTTOM BAR — CenterBox with ViewSwitcher
        // ==========================================

        // Left: stash (flat button)
        let stash_btn = gtk::Button::builder()
            .icon_name("document-save-symbolic")
            .tooltip_text("Stash (Ctrl+Z)")
            .css_classes(["flat"])
            .build();
        stash_btn.set_action_name(Some("win.stash-save"));

        // Long press → stash list popover
        let stash_popover = gtk::Popover::new();
        stash_popover.set_parent(&stash_btn);
        let stash_gesture = gtk::GestureLongPress::new();
        let sp = stash_popover.clone();
        let win = self.clone();
        stash_gesture.connect_pressed(move |_, _, _| {
            win.build_stash_popover_content(&sp);
            sp.popup();
        });
        stash_btn.add_controller(stash_gesture);

        let bottom_left = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        bottom_left.set_margin_start(4);
        bottom_left.set_valign(gtk::Align::Center);
        bottom_left.append(&stash_btn);
        imp.fetch_btn.add_css_class("flat");
        imp.pull_btn.add_css_class("flat");
        imp.push_btn.add_css_class("flat");
        bottom_left.append(&imp.fetch_btn);
        bottom_left.append(&imp.pull_btn);
        bottom_left.append(&imp.push_btn);

        // Center: ViewSwitcher (wide) + compact icon-only toggles (narrow)
        let view_switcher = adw::ViewSwitcher::new();
        view_switcher.set_stack(Some(&imp.view_stack));
        view_switcher.set_policy(adw::ViewSwitcherPolicy::Wide);

        let compact_switcher = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        compact_switcher.set_halign(gtk::Align::Center);
        compact_switcher.set_visible(false);

        let commits_toggle = gtk::ToggleButton::builder()
            .icon_name("commit-symbolic")
            .tooltip_text("Commits")
            .active(true)
            .css_classes(["flat"])
            .build();
        let changes_toggle = gtk::ToggleButton::builder()
            .icon_name("document-edit-symbolic")
            .tooltip_text("Changes")
            .css_classes(["flat"])
            .build();
        let graph_toggle = gtk::ToggleButton::builder()
            .icon_name("branch-fork-symbolic")
            .tooltip_text("Graph")
            .css_classes(["flat"])
            .build();
        changes_toggle.set_group(Some(&commits_toggle));
        graph_toggle.set_group(Some(&commits_toggle));
        compact_switcher.append(&commits_toggle);
        compact_switcher.append(&changes_toggle);
        compact_switcher.append(&graph_toggle);

        // Sync compact toggles → view_stack
        let vs = imp.view_stack.clone();
        commits_toggle.connect_toggled(move |btn| {
            if btn.is_active() { vs.set_visible_child_name("commits"); }
        });
        let vs = imp.view_stack.clone();
        changes_toggle.connect_toggled(move |btn| {
            if btn.is_active() { vs.set_visible_child_name("changes"); }
        });
        let vs = imp.view_stack.clone();
        graph_toggle.connect_toggled(move |btn| {
            if btn.is_active() { vs.set_visible_child_name("graph"); }
        });

        // Sync view_stack → compact toggles
        let ct = commits_toggle.clone();
        let cht = changes_toggle.clone();
        let gt = graph_toggle.clone();
        let win_for_graph = self.clone();
        imp.view_stack.connect_visible_child_name_notify(move |stack| {
            if let Some(name) = stack.visible_child_name() {
                match name.as_str() {
                    "commits" => { if !ct.is_active() { ct.set_active(true); } }
                    "changes" => { if !cht.is_active() { cht.set_active(true); } }
                    "graph" => {
                        if !gt.is_active() { gt.set_active(true); }
                        win_for_graph.populate_graph_tab();
                    }
                    _ => {}
                }
            }
        });

        let switcher_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        switcher_box.set_halign(gtk::Align::Center);
        switcher_box.append(&view_switcher);
        switcher_box.append(&compact_switcher);

        indicators.set_valign(gtk::Align::Center);
        indicators.set_margin_end(8);

        let bottom_bar = gtk::CenterBox::new();
        bottom_bar.set_margin_top(6);
        bottom_bar.set_margin_bottom(6);
        bottom_bar.set_start_widget(Some(&bottom_left));
        bottom_bar.set_center_widget(Some(&switcher_box));
        bottom_bar.set_end_widget(Some(&indicators));

        // ==========================================
        // LAYOUT ASSEMBLY
        // ==========================================

        // Center: content_header + ToastOverlay(banner + ViewStack) + bottom bar
        let center_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        // Conflict banner placeholder (populated dynamically)
        let banner_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        banner_box.set_widget_name("conflict-banner-box");
        center_content.append(&banner_box);
        center_content.append(&imp.view_stack);
        imp.view_stack.set_vexpand(true);
        imp.toast_overlay.set_child(Some(&center_content));
        let center_toolbar = adw::ToolbarView::new();
        center_toolbar.add_top_bar(&content_header);
        center_toolbar.set_top_bar_style(adw::ToolbarStyle::Flat);
        center_toolbar.set_content(Some(&imp.toast_overlay));
        center_toolbar.add_bottom_bar(&bottom_bar);
        center_toolbar.set_bottom_bar_style(adw::ToolbarStyle::Raised);

        // Right sidebar: own HeaderBar (window close buttons) + branches panel
        let right_header = adw::HeaderBar::new();
        right_header.set_show_start_title_buttons(false);
        right_header.set_show_end_title_buttons(true);
        right_header.set_title_widget(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));

        let right_toolbar = adw::ToolbarView::new();
        right_toolbar.add_top_bar(&right_header);
        right_toolbar.set_top_bar_style(adw::ToolbarStyle::Flat);
        right_toolbar.set_content(Some(&branches_panel));

        // Inner split: center | right sidebar (full height each)
        let inner_split = adw::OverlaySplitView::new();
        inner_split.set_sidebar_position(gtk::PackType::End);
        inner_split.set_sidebar(Some(&right_toolbar));
        inner_split.set_content(Some(&center_toolbar));
        inner_split.set_collapsed(false);
        inner_split.set_show_sidebar(true);
        inner_split.set_min_sidebar_width(120.0);
        inner_split.set_max_sidebar_width(300.0);

        // Content header: no window buttons by default (right_header has end buttons)
        content_header.set_show_end_title_buttons(false);

        // Outer split: left sidebar (full height) | inner
        let outer_split = adw::OverlaySplitView::new();
        outer_split.set_sidebar_position(gtk::PackType::Start);
        outer_split.set_sidebar(Some(&sidebar_toolbar));
        outer_split.set_content(Some(&inner_split));
        outer_split.set_collapsed(false);
        outer_split.set_show_sidebar(true);
        outer_split.set_min_sidebar_width(140.0);
        outer_split.set_max_sidebar_width(280.0);
        outer_split.set_vexpand(true);

        // Toggle repo tree (left) sidebar
        let os = outer_split.clone();
        toggle_repo_tree.connect_toggled(move |btn| {
            os.set_show_sidebar(btn.is_active());
        });
        let trt = toggle_repo_tree.clone();
        outer_split.connect_show_sidebar_notify(move |split| {
            if trt.is_active() != split.shows_sidebar() {
                trt.set_active(split.shows_sidebar());
            }
        });

        // Toggle right sidebar (branches/tags)
        let is = inner_split.clone();
        toggle_right_panel.connect_toggled(move |btn| {
            is.set_show_sidebar(btn.is_active());
        });
        let trp = toggle_right_panel.clone();
        inner_split.connect_show_sidebar_notify(move |split| {
            if trp.is_active() != split.shows_sidebar() {
                trp.set_active(split.shows_sidebar());
            }
        });

        // Store refs
        *imp.outer_split.borrow_mut() = Some(outer_split.clone());
        *imp.inner_split.borrow_mut() = Some(inner_split.clone());

        // Store panel list refs in imp for populate_branches_tags
        *imp.branches_local_list.borrow_mut() = Some(bl_list);
        *imp.branches_remote_list.borrow_mut() = Some(br_list);
        *imp.tags_list.borrow_mut() = Some(tg_list);
        *imp.stashes_list.borrow_mut() = Some(st_list);
        *imp.submodules_list.borrow_mut() = Some(sm_list);
        *imp.worktrees_list.borrow_mut() = Some(wt_list);

        self.set_content(Some(&outer_split));

        // ==========================================
        // BREAKPOINTS
        // ==========================================

        // Tablet (<860sp): collapse repo sidebar
        let bp_tablet = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            860.0,
            adw::LengthUnit::Sp,
        ));
        bp_tablet.add_setter(&outer_split, "collapsed", Some(&true.to_value()));
        bp_tablet.add_setter(&content_header, "show-end-title-buttons", Some(&true.to_value()));
        self.add_breakpoint(bp_tablet);

        // Narrow (<600sp): collapse inner split, compact switcher, hide branch
        let bp_narrow = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            600.0,
            adw::LengthUnit::Sp,
        ));
        bp_narrow.add_setter(&outer_split, "collapsed", Some(&true.to_value()));
        bp_narrow.add_setter(&inner_split, "collapsed", Some(&true.to_value()));
        bp_narrow.add_setter(&content_header, "show-end-title-buttons", Some(&true.to_value()));
        bp_narrow.add_setter(&imp.sidebar_title_label, "visible", Some(&false.to_value()));
        bp_narrow.add_setter(&branch_content, "visible", Some(&false.to_value()));
        bp_narrow.add_setter(&view_switcher, "visible", Some(&false.to_value()));
        bp_narrow.add_setter(&compact_switcher, "visible", Some(&true.to_value()));
        self.add_breakpoint(bp_narrow);

        // Mobile (<500sp): hide content title, remote buttons move to icons only
        let bp_mobile = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            500.0,
            adw::LengthUnit::Sp,
        ));
        bp_mobile.add_setter(&outer_split, "collapsed", Some(&true.to_value()));
        bp_mobile.add_setter(&inner_split, "collapsed", Some(&true.to_value()));
        bp_mobile.add_setter(&content_header, "show-end-title-buttons", Some(&true.to_value()));
        bp_mobile.add_setter(&imp.sidebar_title_label, "visible", Some(&false.to_value()));
        bp_mobile.add_setter(&branch_content, "visible", Some(&false.to_value()));
        bp_mobile.add_setter(&content_title, "visible", Some(&false.to_value()));
        bp_mobile.add_setter(&view_switcher, "visible", Some(&false.to_value()));
        bp_mobile.add_setter(&compact_switcher, "visible", Some(&true.to_value()));
        // Hide non-essential header items to fit on 360px screens
        bp_mobile.add_setter(&search_toggle, "visible", Some(&false.to_value()));
        bp_mobile.add_setter(&open_button, "visible", Some(&false.to_value()));
        // Remove title widget to free header space for sidebar toggles + close button
        bp_mobile.add_setter(&content_header, "show-title", Some(&false.to_value()));
        // Hide stash button label area in bottom bar
        bp_mobile.add_setter(&stash_btn, "visible", Some(&false.to_value()));
        self.add_breakpoint(bp_mobile);
    }

    fn rebuild_hamburger_menu(&self) {
        let imp = self.imp();
        let menu_model = gio::Menu::new();

        // Recent workspaces submenu
        let recent_submenu = gio::Menu::new();
        let config = imp.config.borrow();
        for workspace_path in &config.recent_workspaces {
            let label = std::path::Path::new(workspace_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| workspace_path.clone());
            recent_submenu.append(
                Some(&label),
                Some(&format!("win.open-recent('{}')", workspace_path.replace('\'', ""))),
            );
        }
        drop(config);
        if recent_submenu.n_items() > 0 {
            menu_model.append_submenu(Some("Recent"), &recent_submenu);
        }

        menu_model.append(Some("Clone Repository…"), Some("win.clone-repo"));
        menu_model.append(Some("Manage Remotes…"), Some("win.remotes"));
        menu_model.append(Some("Reflog"), Some("win.reflog"));
        menu_model.append(Some("Edit .gitignore"), Some("win.edit-gitignore"));
        menu_model.append(Some("Apply Patch…"), Some("win.apply-patch"));
        menu_model.append(Some("Preferences"), Some("win.preferences"));
        menu_model.append(Some("About Gitpulsar"), Some("win.about"));

        imp.menu_btn.set_menu_model(Some(&menu_model));
    }

    fn setup_actions(&self) {
        let action = gio::SimpleAction::new("open-repo", None);
        let window = self.clone();
        action.connect_activate(move |_, _| {
            let dialog = gtk::FileChooserDialog::new(
                Some("Open Workspace / Repository"),
                Some(&window),
                gtk::FileChooserAction::SelectFolder,
                &[
                    ("Cancel", gtk::ResponseType::Cancel),
                    ("Open", gtk::ResponseType::Accept),
                ],
            );
            dialog.set_modal(true);
            let win = window.clone();
            dialog.connect_response(move |dialog, response| {
                if response == gtk::ResponseType::Accept {
                    if let Some(folder) = dialog.file() {
                        if let Some(path) = folder.path() {
                            win.open_workspace(&path);
                        }
                    }
                }
                dialog.close();
            });
            dialog.present();
        });
        self.add_action(&action);

        // Open recent workspace action
        let open_recent_action = gio::SimpleAction::new("open-recent", Some(glib::VariantTy::STRING));
        let window = self.clone();
        open_recent_action.connect_activate(move |_, param| {
            if let Some(param) = param {
                if let Some(path_str) = param.get::<String>() {
                    let path = std::path::PathBuf::from(&path_str);
                    if path.is_dir() {
                        window.open_workspace(&path);
                    }
                }
            }
        });
        self.add_action(&open_recent_action);

        // Clone action
        let clone_action = gio::SimpleAction::new("clone-repo", None);
        let window = self.clone();
        clone_action.connect_activate(move |_, _| {
            window.show_clone_dialog();
        });
        self.add_action(&clone_action);

        // Apply patch action
        let apply_patch_action = gio::SimpleAction::new("apply-patch", None);
        let window = self.clone();
        apply_patch_action.connect_activate(move |_, _| {
            window.show_apply_patch_dialog();
        });
        self.add_action(&apply_patch_action);

        // Reflog action
        let reflog_action = gio::SimpleAction::new("reflog", None);
        let window = self.clone();
        reflog_action.connect_activate(move |_, _| {
            window.show_reflog_dialog();
        });
        self.add_action(&reflog_action);

        // Remotes action
        let remotes_action = gio::SimpleAction::new("remotes", None);
        let window = self.clone();
        remotes_action.connect_activate(move |_, _| {
            window.show_remotes_dialog();
        });
        self.add_action(&remotes_action);

        // Force push action
        let force_push_action = gio::SimpleAction::new("force-push", None);
        let window = self.clone();
        force_push_action.connect_activate(move |_, _| {
            window.show_force_push_dialog();
        });
        self.add_action(&force_push_action);

        // Commit action
        let commit_action = gio::SimpleAction::new("commit", None);
        let window = self.clone();
        commit_action.connect_activate(move |_, _| {
            window.on_commit_clicked();
        });
        self.add_action(&commit_action);

        // Stage all
        let stage_all_action = gio::SimpleAction::new("stage-all", None);
        let window = self.clone();
        stage_all_action.connect_activate(move |_, _| {
            window.on_stage_all();
        });
        self.add_action(&stage_all_action);

        // Unstage all
        let unstage_all_action = gio::SimpleAction::new("unstage-all", None);
        let window = self.clone();
        unstage_all_action.connect_activate(move |_, _| {
            window.on_unstage_all();
        });
        self.add_action(&unstage_all_action);

        // Fetch
        let fetch_action = gio::SimpleAction::new("fetch", None);
        let window = self.clone();
        fetch_action.connect_activate(move |_, _| {
            window.on_fetch();
        });
        self.add_action(&fetch_action);

        // Push
        let push_action = gio::SimpleAction::new("push", None);
        let window = self.clone();
        push_action.connect_activate(move |_, _| {
            window.on_push(false);
        });
        self.add_action(&push_action);

        // Pull
        let pull_action = gio::SimpleAction::new("pull", None);
        let window = self.clone();
        pull_action.connect_activate(move |_, _| {
            window.on_pull();
        });
        self.add_action(&pull_action);

        // Show commits page
        let show_commits_action = gio::SimpleAction::new("show-commits", None);
        let window = self.clone();
        show_commits_action.connect_activate(move |_, _| {
            window.imp().view_stack.set_visible_child_name("commits");
        });
        self.add_action(&show_commits_action);

        // Show changes page
        let show_changes_action = gio::SimpleAction::new("show-changes", None);
        let window = self.clone();
        show_changes_action.connect_activate(move |_, _| {
            window.imp().view_stack.set_visible_child_name("changes");
        });
        self.add_action(&show_changes_action);

        // Show graph page
        let show_graph_action = gio::SimpleAction::new("show-graph", None);
        let window = self.clone();
        show_graph_action.connect_activate(move |_, _| {
            window.imp().view_stack.set_visible_child_name("graph");
        });
        self.add_action(&show_graph_action);

        // Focus search
        let focus_search_action = gio::SimpleAction::new("focus-search", None);
        let window = self.clone();
        focus_search_action.connect_activate(move |_, _| {
            window.imp().search_entry.grab_focus();
        });
        self.add_action(&focus_search_action);

        // Stash save
        let stash_save_action = gio::SimpleAction::new("stash-save", None);
        let window = self.clone();
        stash_save_action.connect_activate(move |_, _| {
            window.on_stash_save();
        });
        self.add_action(&stash_save_action);

        // Stash pop
        let stash_pop_action = gio::SimpleAction::new("stash-pop", None);
        let window = self.clone();
        stash_pop_action.connect_activate(move |_, _| {
            window.on_stash_pop();
        });
        self.add_action(&stash_pop_action);

        // Preferences
        let prefs_action = gio::SimpleAction::new("preferences", None);
        let window = self.clone();
        prefs_action.connect_activate(move |_, _| {
            window.open_preferences();
        });
        self.add_action(&prefs_action);

        // About
        let about_action = gio::SimpleAction::new("about", None);
        let window = self.clone();
        about_action.connect_activate(move |_, _| {
            let dialog = adw::AboutDialog::builder()
                .application_name("Gitpulsar")
                .application_icon("io.gitlab.ilshat_apps.gitpulsar")
                .developer_name("Ilshat Ishdavletov")
                .version(env!("CARGO_PKG_VERSION"))
                .website("https://gitlab.com/ilshat.ishdavletov/gitpulsar")
                .license_type(gtk::License::Gpl30)
                .build();
            dialog.present(Some(&window));
        });
        self.add_action(&about_action);

        // Undo
        let undo_action = gio::SimpleAction::new("undo", None);
        let window = self.clone();
        undo_action.connect_activate(move |_, _| {
            window.on_undo();
        });
        self.add_action(&undo_action);

        // Redo
        let redo_action = gio::SimpleAction::new("redo", None);
        let window = self.clone();
        redo_action.connect_activate(move |_, _| {
            window.on_redo();
        });
        self.add_action(&redo_action);

        // Edit .gitignore
        let gitignore_action = gio::SimpleAction::new("edit-gitignore", None);
        let window = self.clone();
        gitignore_action.connect_activate(move |_, _| {
            window.open_gitignore_editor();
        });
        self.add_action(&gitignore_action);
    }

    /// Open a workspace folder (or single repo).
    pub fn open_workspace(&self, path: &std::path::Path) {
        match workspace::scan_workspace(path) {
            Ok(entries) => {
                tracing::info!("Opened workspace: {} ({} entries)", path.display(), entries.len());

                // Update sidebar header title to folder name
                let folder_name = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string());
                self.imp().sidebar_title_label.set_label(&folder_name);

                repo_tree::populate_repo_list(&self.imp().repo_list_box, &entries);

                // Save to recent workspaces
                self.imp().config.borrow_mut().add_recent_workspace(&path.to_string_lossy());
                self.rebuild_hamburger_menu();

                let auto_select = entries.len() == 1 && entries[0].is_git_repo;
                *self.imp().workspace_entries.borrow_mut() = entries;

                if auto_select {
                    self.select_repo(0);
                    // Select first row visually
                    if let Some(row) = self.imp().repo_list_box.row_at_index(0) {
                        self.imp().repo_list_box.select_row(Some(&row));
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to scan workspace: {}", e);
                let dialog = adw::AlertDialog::new(
                    Some("Error"),
                    Some(&format!("Failed to open workspace:\n{}", e)),
                );
                dialog.add_response("ok", "OK");
                dialog.present(Some(self));
            }
        }
    }

    /// Select a repo from the workspace tree by index.
    fn select_repo(&self, index: usize) {
        let entries = self.imp().workspace_entries.borrow();
        let Some(entry) = entries.get(index) else {
            return;
        };

        if !entry.is_git_repo {
            return;
        }

        let path = entry.path.to_string_lossy().to_string();
        drop(entries);

        match GitRepo::open(&path) {
            Ok(repo) => {
                tracing::info!("Selected repo: {}", path);
                self.load_repo_data(&repo);
                *self.imp().repo.borrow_mut() = Some(repo);
            }
            Err(e) => {
                tracing::error!("Failed to open repository: {}", e);
                let dialog = adw::AlertDialog::new(
                    Some("Error"),
                    Some(&format!("Failed to open repository:\n{}", e)),
                );
                dialog.add_response("ok", "OK");
                dialog.present(Some(self));
            }
        }
    }

    /// Load data for the currently selected repo asynchronously.
    fn load_repo_data(&self, repo: &GitRepo) {
        let imp = self.imp();

        // Reset search immediately
        *imp.selected_commit_id.borrow_mut() = None;
        imp.search_entry.set_text("");
        imp.commit_list_box.set_filter_func(|_| true);

        let path = repo.path().to_string_lossy().to_string();

        let (tx, rx) = async_channel::bounded::<BackgroundRepoData>(1);
        std::thread::spawn(move || {
            let Ok(mut repo) = GitRepo::open(&path) else { return };

            // Collect quick scalar data on this thread
            let (ahead, behind) = repo.ahead_behind().unwrap_or((0, 0));
            let branch_name = repo.current_branch_name();
            let stash_entries = repo.stash_list().unwrap_or_default();
            let submodules = repo.list_submodules().unwrap_or_default();
            let worktrees = repo.list_worktrees().unwrap_or_default();
            let has_conflicts = repo.has_conflicts();
            let is_merging = repo.is_merging();
            let is_rebasing = repo.is_rebasing();

            // Run independent heavy operations in parallel
            let path2 = path.clone();
            let path3 = path.clone();

            let commits_handle = std::thread::spawn(move || {
                let Ok(repo) = GitRepo::open(&path2) else {
                    return (Vec::new(), HashMap::new(), Vec::new());
                };
                let commits = repo.log(COMMIT_PAGE_SIZE).unwrap_or_default();
                let tags_map = repo.tags_by_commit().unwrap_or_default();
                let tags = repo.tags().unwrap_or_default();
                (commits, tags_map, tags)
            });

            let status_handle = std::thread::spawn(move || {
                let Ok(repo) = GitRepo::open(&path3) else {
                    return (None, Vec::new(), Vec::new(), Vec::new());
                };
                let status = repo.status(true).ok();
                let branches = repo.branches().unwrap_or_default();
                let unstaged_diffs = repo.diff_unstaged().unwrap_or_default();
                let staged_diffs = repo.diff_staged().unwrap_or_default();
                (status, branches, unstaged_diffs, staged_diffs)
            });

            let (commits, tags_map, tags) = commits_handle.join().unwrap_or_default();
            let (status, branches, unstaged_diffs, staged_diffs) =
                status_handle.join().unwrap_or((None, Vec::new(), Vec::new(), Vec::new()));

            tx.send_blocking(BackgroundRepoData {
                commits,
                tags_map,
                status,
                branches,
                tags,
                stash_entries,
                submodules,
                worktrees,
                ahead,
                behind,
                branch_name,
                unstaged_diffs,
                staged_diffs,
                has_conflicts,
                is_merging,
                is_rebasing,
            }).ok();
        });

        let win = self.clone();
        glib::spawn_future_local(async move {
            if let Ok(data) = rx.recv().await {
                let imp = win.imp();

                // Update branch & indicators
                if let Some(ref branch) = data.branch_name {
                    imp.branch_label.set_label(branch);
                }
                imp.ahead_label.set_label(&format!("▲ {}", data.ahead));
                imp.behind_label.set_label(&format!("▼ {}", data.behind));

                // Load commits (clear first so offset=0 for full rebuild)
                imp.commits.borrow_mut().clear();
                win.populate_commit_list(&data.commits, data.ahead, &data.tags_map);
                *imp.commits.borrow_mut() = data.commits;

                // Load status for changes view
                if let Some(ref status) = data.status {
                    win.refresh_changes_list(status);
                }

                // Cache diffs
                *imp.cached_unstaged_diffs.borrow_mut() = data.unstaged_diffs;
                *imp.cached_staged_diffs.borrow_mut() = data.staged_diffs;

                // Populate branches & tags in right sidebar
                win.populate_branches_tags_data(&data.branches, &data.tags);

                // Populate stashes in right sidebar
                win.populate_stashes_data(&data.stash_entries);

                // Populate submodules & worktrees
                win.populate_submodules_data(&data.submodules);
                win.populate_worktrees_data(&data.worktrees);

                // Show/hide conflict banner
                win.update_conflict_banner(data.has_conflicts, data.is_merging, data.is_rebasing);

                // Update sidebar status
                if let Some(ref path_str) = win.repo_path_string() {
                    let name = std::path::Path::new(path_str)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    win.update_sidebar_status(&name, data.status.as_ref());
                }

                // Refresh graph if currently visible
                if win.imp().view_stack.visible_child_name().as_deref() == Some("graph") {
                    win.populate_graph_tab();
                }
            }
        });
    }

    fn populate_commit_list(
        &self,
        commits: &[CommitInfo],
        ahead: usize,
        tags_map: &HashMap<String, Vec<String>>,
    ) {
        let imp = self.imp();
        let list_box = &imp.commit_list_box;

        // Skip rebuild if commits haven't changed
        let new_hash = hash_commits(commits);
        if new_hash == imp.last_commits_hash.get() {
            return;
        }
        imp.last_commits_hash.set(new_hash);

        while let Some(child) = list_box.first_child() {
            list_box.remove(&child);
        }

        let offset = imp.commits.borrow().len();

        for (idx, commit_info) in commits.iter().enumerate() {
            let global_idx = offset + idx;
            let tags = tags_map
                .get(&commit_info.id)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let is_unpushed = global_idx < ahead;
            let is_head = global_idx == 0;
            let date_format = imp.config.borrow().date_format;
            let row = commit_list::create_commit_row(commit_info, tags, is_unpushed, is_head, date_format, None);

            // Connect edit-message button for HEAD commit
            if is_head {
                if let Some(btn) = commit_list::find_edit_message_btn(&row) {
                    let win = self.clone();
                    let msg = commit_info.message.clone();
                    btn.connect_clicked(move |_| {
                        win.show_edit_message_dialog(&msg);
                    });
                }
            }

            list_box.append(&row);
        }

        // Show "Load more" button if we got a full page
        if commits.len() >= COMMIT_PAGE_SIZE {
            self.append_load_more_row();
        }

        imp.commits_loaded_count.set(offset + commits.len());
    }

    fn load_more_commits(&self) {
        let imp = self.imp();

        let repo_path = self.repo_path_string();
        let Some(path) = repo_path else { return };

        let skip = imp.commits_loaded_count.get();

        // Remove the "Load more" row
        self.remove_load_more_row();

        let (tx, rx) = async_channel::bounded::<(Vec<CommitInfo>, HashMap<String, Vec<String>>)>(1);
        std::thread::spawn(move || {
            let Ok(repo) = GitRepo::open(&path) else { return };
            let commits = repo.log_page(skip, COMMIT_PAGE_SIZE).unwrap_or_default();
            let tags_map = repo.tags_by_commit().unwrap_or_default();
            let _ = tx.send_blocking((commits, tags_map));
        });

        let win = self.clone();
        glib::spawn_future_local(async move {
            if let Ok((new_commits, tags_map)) = rx.recv().await {
                let imp = win.imp();
                let list_box = &imp.commit_list_box;
                let ahead = imp.ahead_label.label()
                    .strip_prefix("▲ ")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(0);
                let offset = imp.commits_loaded_count.get();
                let date_format = imp.config.borrow().date_format;

                for (idx, commit_info) in new_commits.iter().enumerate() {
                    let global_idx = offset + idx;
                    let tags = tags_map
                        .get(&commit_info.id)
                        .map(|v| v.as_slice())
                        .unwrap_or(&[]);
                    let is_unpushed = global_idx < ahead;
                    let row = commit_list::create_commit_row(commit_info, tags, is_unpushed, false, date_format, None);
                    list_box.append(&row);
                }

                // Add "Load more" if full page
                if new_commits.len() >= COMMIT_PAGE_SIZE {
                    win.append_load_more_row();
                }

                imp.commits_loaded_count.set(offset + new_commits.len());
                imp.commits.borrow_mut().extend(new_commits);
            }
        });
    }

    fn remove_load_more_row(&self) {
        let list_box = &self.imp().commit_list_box;
        let n_rows = list_box.observe_children().n_items();
        if n_rows > 0 {
            if let Some(last_row) = list_box.row_at_index((n_rows - 1) as i32) {
                if last_row.widget_name() == "load-more-row" {
                    list_box.remove(&last_row);
                }
            }
        }
    }

    fn append_load_more_row(&self) {
        let load_more_row = gtk::ListBoxRow::builder()
            .selectable(false)
            .activatable(true)
            .build();
        load_more_row.set_widget_name("load-more-row");
        let label = gtk::Label::builder()
            .label("Load more commits...")
            .css_classes(["dim-label"])
            .margin_top(8)
            .margin_bottom(8)
            .build();
        load_more_row.set_child(Some(&label));
        self.imp().commit_list_box.append(&load_more_row);
    }

    fn on_commit_selected(&self, index: usize) {
        let imp = self.imp();
        let commits = imp.commits.borrow();

        if let Some(commit) = commits.get(index) {
            let commit_id = commit.id.clone();
            drop(commits);

            // Toggle detail expand on the selected row
            if let Some(row) = imp.commit_list_box.row_at_index(index as i32) {
                let expanded = commit_list::toggle_detail(&row);

                if expanded {
                    // Load file list asynchronously to avoid blocking the UI
                    let repo_path = self.repo_path_string();
                    let files_box = commit_list::get_files_box(&row);
                    let limit = imp.config.borrow().commit_files_limit;
                    let cid = commit_id.clone();

                    if let (Some(path), Some(files_box)) = (repo_path, files_box) {
                        // Show spinner while loading
                        commit_list::show_files_loading(&files_box);

                        let (tx, rx) = async_channel::bounded::<Vec<DiffFile>>(1);
                        std::thread::spawn(move || {
                            let files = GitRepo::open(&path)
                                .ok()
                                .and_then(|r| r.diff_commit(&cid).ok())
                                .unwrap_or_default();
                            let _ = tx.send_blocking(files);
                        });

                        glib::spawn_future_local(async move {
                            if let Ok(files) = rx.recv().await {
                                commit_list::populate_commit_files(&files_box, &files, limit);
                            }
                        });
                    }
                }
            }

            *imp.selected_commit_id.borrow_mut() = Some(commit_id);
        }
    }

    fn on_commit_clicked(&self) {
        let imp = self.imp();
        let buffer = imp.commit_entry.buffer();
        let message = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        let message = message.trim();

        if message.is_empty() {
            return;
        }

        let is_amend = imp.amend_check.is_active();

        let repo_ref = imp.repo.borrow();
        if let Some(ref repo) = *repo_ref {
            let result = if is_amend {
                repo.amend_commit(Some(message))
            } else {
                repo.commit(message)
            };

            match result {
                Ok(oid) => {
                    tracing::info!("{}: {}", if is_amend { "Amended commit" } else { "Created commit" }, oid);
                    buffer.set_text("");
                    imp.amend_check.set_active(false);
                    drop(repo_ref);
                    // Reload repo data
                    let repo_ref = self.imp().repo.borrow();
                    if let Some(ref repo) = *repo_ref {
                        self.load_repo_data(repo);
                    }
                    drop(repo_ref);
                }
                Err(e) => {
                    tracing::error!("Failed to {}: {}", if is_amend { "amend" } else { "commit" }, e);
                    drop(repo_ref);
                    let title = if is_amend { "Amend Failed" } else { "Commit Failed" };
                    let dialog = adw::AlertDialog::new(
                        Some(title),
                        Some(&format!("{}", e)),
                    );
                    dialog.add_response("ok", "OK");
                    dialog.present(Some(self));
                }
            }
        }
    }

    fn on_stage_all(&self) {
        let repo_ref = self.imp().repo.borrow();
        if let Some(ref repo) = *repo_ref {
            if let Err(e) = repo.stage_all() {
                tracing::error!("Failed to stage all: {}", e);
            }
            drop(repo_ref);
            self.imp().undo_stack.borrow_mut().push(UndoableOp::StageAll);
            self.refresh_staging();
        }
    }

    fn on_unstage_all(&self) {
        let repo_ref = self.imp().repo.borrow();
        if let Some(ref repo) = *repo_ref {
            if let Err(e) = repo.unstage_all() {
                tracing::error!("Failed to unstage all: {}", e);
            }
            drop(repo_ref);
            self.imp().undo_stack.borrow_mut().push(UndoableOp::UnstageAll);
            self.refresh_staging();
        }
    }

    fn stage_file(&self, path: &str) {
        let repo_ref = self.imp().repo.borrow();
        if let Some(ref repo) = *repo_ref {
            if let Err(e) = repo.stage_file(path) {
                tracing::error!("Failed to stage {}: {}", path, e);
            }
            drop(repo_ref);
            self.imp().undo_stack.borrow_mut().push(UndoableOp::StageFile(path.to_string()));
            self.refresh_staging();
        }
    }

    fn unstage_file(&self, path: &str) {
        let repo_ref = self.imp().repo.borrow();
        if let Some(ref repo) = *repo_ref {
            if let Err(e) = repo.unstage_file(path) {
                tracing::error!("Failed to unstage {}: {}", path, e);
            }
            drop(repo_ref);
            self.imp().undo_stack.borrow_mut().push(UndoableOp::UnstageFile(path.to_string()));
            self.refresh_staging();
        }
    }

    fn discard_file_with_confirm(&self, path: &str) {
        let dialog = adw::AlertDialog::new(
            Some("Discard Changes?"),
            Some(&format!(
                "This will permanently discard all changes to:\n\n<b>{}</b>",
                glib::markup_escape_text(path)
            )),
        );
        dialog.set_body_use_markup(true);
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("discard", "Discard");
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let win = self.clone();
        let file_path = path.to_string();
        dialog.connect_response(None, move |_, response| {
            if response == "discard" {
                let repo_ref = win.imp().repo.borrow();
                if let Some(ref repo) = *repo_ref {
                    // Save content before discard for undo
                    let full_path = repo.path().join(&file_path);
                    let saved_content = std::fs::read(&full_path).unwrap_or_default();

                    if let Err(e) = repo.discard_file(&file_path) {
                        tracing::error!("Failed to discard {}: {}", file_path, e);
                    } else {
                        win.imp().undo_stack.borrow_mut().push(
                            UndoableOp::Discard(file_path.clone(), saved_content),
                        );
                    }
                    drop(repo_ref);
                    win.refresh_staging();
                }
            }
        });
        dialog.present(Some(self));
    }

    fn stage_hunk(&self, path: &str, hunk_index: usize) {
        let repo_ref = self.imp().repo.borrow();
        if let Some(ref repo) = *repo_ref {
            if let Err(e) = repo.stage_hunk(path, hunk_index) {
                tracing::error!("Failed to stage hunk {} of {}: {}", hunk_index, path, e);
                self.show_toast(&format!("Failed to stage hunk: {}", e));
            }
            drop(repo_ref);
            self.refresh_staging();
        }
    }

    fn unstage_hunk(&self, path: &str, hunk_index: usize) {
        let repo_ref = self.imp().repo.borrow();
        if let Some(ref repo) = *repo_ref {
            if let Err(e) = repo.unstage_hunk(path, hunk_index) {
                tracing::error!("Failed to unstage hunk {} of {}: {}", hunk_index, path, e);
                self.show_toast(&format!("Failed to unstage hunk: {}", e));
            }
            drop(repo_ref);
            self.refresh_staging();
        }
    }

    fn stage_selected_lines(&self, path: &str, is_unstage: bool) {
        // Find the hunk_actions_box for this file — need to find the row
        // Walk through both lists to find the row with this path
        let imp = self.imp();
        let lists = [
            imp.unstaged_file_list.borrow().clone(),
            imp.staged_file_list.borrow().clone(),
        ];

        for list_opt in &lists {
            let Some(ref list) = list_opt else { continue };
            let mut row_widget = list.first_child();
            while let Some(ref rw) = row_widget {
                if let Ok(row) = rw.clone().downcast::<gtk::ListBoxRow>() {
                    if row.widget_name() == path {
                        if let Some(hunk_box) = changes_view::get_hunk_actions_box(&row) {
                            // Find line-selectors-box inside hunk_box
                            let mut child = hunk_box.first_child();
                            while let Some(c) = child {
                                if c.widget_name() == "line-selectors-box" {
                                    if let Ok(selectors_box) = c.downcast::<gtk::Box>() {
                                        // Iterate over each hunk selector
                                        let mut selector_child = selectors_box.first_child();
                                        let mut hunk_idx = 0;
                                        while let Some(sc) = selector_child {
                                            if let Ok(selector) = sc.clone().downcast::<gtk::Box>() {
                                                let indices = changes_view::collect_selected_lines(&selector);
                                                if !indices.is_empty() {
                                                    let repo_ref = self.imp().repo.borrow();
                                                    if let Some(ref repo) = *repo_ref {
                                                        let result = if is_unstage {
                                                            repo.unstage_lines(path, hunk_idx, &indices)
                                                        } else {
                                                            repo.stage_lines(path, hunk_idx, &indices)
                                                        };
                                                        if let Err(e) = result {
                                                            tracing::error!("Failed to stage lines: {}", e);
                                                            self.show_toast(&format!("Failed: {}", e));
                                                        }
                                                    }
                                                }
                                                hunk_idx += 1;
                                            }
                                            selector_child = sc.next_sibling();
                                        }
                                    }
                                    break;
                                }
                                child = c.next_sibling();
                            }
                        }
                        self.refresh_staging();
                        return;
                    }
                }
                row_widget = rw.next_sibling();
            }
        }
    }

    fn on_undo(&self) {
        let op = self.imp().undo_stack.borrow_mut().undo();
        let Some(op) = op else { return };
        let repo_ref = self.imp().repo.borrow();
        let Some(ref repo) = *repo_ref else { return };

        match op {
            UndoableOp::StageFile(path) => { let _ = repo.unstage_file(&path); }
            UndoableOp::UnstageFile(path) => { let _ = repo.stage_file(&path); }
            UndoableOp::StageAll => { let _ = repo.unstage_all(); }
            UndoableOp::UnstageAll => { let _ = repo.stage_all(); }
            UndoableOp::Discard(path, content) => {
                let full_path = repo.path().join(&path);
                let _ = std::fs::write(&full_path, &content);
            }
        }
        drop(repo_ref);
        self.refresh_staging();
    }

    fn on_redo(&self) {
        let op = self.imp().undo_stack.borrow_mut().redo();
        let Some(op) = op else { return };
        let repo_ref = self.imp().repo.borrow();
        let Some(ref repo) = *repo_ref else { return };

        match op {
            UndoableOp::StageFile(path) => { let _ = repo.stage_file(&path); }
            UndoableOp::UnstageFile(path) => { let _ = repo.unstage_file(&path); }
            UndoableOp::StageAll => { let _ = repo.stage_all(); }
            UndoableOp::UnstageAll => { let _ = repo.unstage_all(); }
            UndoableOp::Discard(path, _) => { let _ = repo.discard_file(&path); }
        }
        drop(repo_ref);
        self.refresh_staging();
    }

    /// Update the sidebar status bar with repo name and git status summary.
    /// Recompute and apply indicator for the active repo without full workspace scan.
    fn refresh_active_repo_indicator(&self, path_str: &str) {
        let imp = self.imp();
        let Ok(repo) = GitRepo::open(path_str) else { return };
        let (tracked, untracked) = repo.dirty_kinds();
        let (ahead, _) = repo.ahead_behind().unwrap_or((0, 0));
        let branch = repo.current_branch_name();
        let new_indicator = gitpulsar_core::workspace::RepoIndicator {
            is_dirty: tracked || untracked,
            has_tracked_changes: tracked,
            ahead,
            branch,
        };

        let mut entries = imp.workspace_entries.borrow_mut();
        let Some(idx) = entries.iter().position(|e| e.path.to_string_lossy() == path_str) else {
            return;
        };
        entries[idx].indicator = Some(new_indicator);
        let entries_clone = entries.clone();
        drop(entries);

        let selected_idx = imp.repo_list_box.selected_row().map(|r| r.index());
        repo_tree::populate_repo_list(&imp.repo_list_box, &entries_clone);
        if let Some(i) = selected_idx {
            if let Some(row) = imp.repo_list_box.row_at_index(i) {
                imp.repo_list_box.select_row(Some(&row));
            }
        }
        // Reset workspace hash so throttled scan doesn't immediately override
        imp.last_workspace_hash.set(0);
    }

    fn update_sidebar_status(&self, repo_name: &str, status: Option<&RepoStatus>) {
        let imp = self.imp();
        imp.sidebar_repo_name_label.set_label(repo_name);
        let text = match status {
            Some(s) => {
                let mut parts = Vec::new();
                let modified = s.unstaged.len();
                let staged = s.staged.len();
                let untracked = s.untracked.len();
                if modified > 0 {
                    parts.push(format!("{} modified", modified));
                }
                if staged > 0 {
                    parts.push(format!("{} staged", staged));
                }
                if untracked > 0 {
                    parts.push(format!("{} untracked", untracked));
                }
                if parts.is_empty() {
                    "Clean".to_string()
                } else {
                    parts.join(", ")
                }
            }
            None => String::new(),
        };
        imp.sidebar_status_label.set_label(&text);
    }

    /// Reload the changes file list (after stage/unstage/discard).
    fn refresh_staging(&self) {
        let repo_ref = self.imp().repo.borrow();
        if let Some(ref repo) = *repo_ref {
            // Update diff cache
            if let Ok(diffs) = repo.diff_unstaged() {
                *self.imp().cached_unstaged_diffs.borrow_mut() = diffs;
            }
            if let Ok(diffs) = repo.diff_staged() {
                *self.imp().cached_staged_diffs.borrow_mut() = diffs;
            }
            if let Ok(status) = repo.status(true) {
                self.refresh_changes_list(&status);
                // Update sidebar status
                let name = repo.path().file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.update_sidebar_status(&name, Some(&status));
            }
        }
    }

    /// Populate the changes view file list from a RepoStatus.
    fn refresh_changes_list(&self, status: &RepoStatus) {
        let imp = self.imp();
        let unstaged = imp.unstaged_file_list.borrow().clone();
        let staged = imp.staged_file_list.borrow().clone();
        if let (Some(ref ul), Some(ref sl)) = (unstaged, staged) {
            let files = changes_view::collect_changed_files(status);
            changes_view::populate_file_lists(ul, sl, &files);
        }
    }

    /// Handle click on a file in the changes accordion — toggle inline diff.
    fn on_changes_file_activated(&self, row: &gtk::ListBoxRow) {
        let file_path = row.widget_name().to_string();

        // Check if this is a conflicted file — open conflict editor instead
        {
            let repo_ref = self.imp().repo.borrow();
            if let Some(ref repo) = *repo_ref {
                if repo.has_conflicts() {
                    if let Ok(chunks) = repo.parse_conflicts(&file_path) {
                        if chunks.iter().any(|c| c.is_conflict) {
                            let fp = file_path.clone();
                            drop(repo_ref);
                            self.open_conflict_editor(&fp, &chunks);
                            return;
                        }
                    }
                }
            }
        }

        let expanded = changes_view::toggle_file_diff(row);

        if expanded {
            if file_path.is_empty() {
                return;
            }

            let repo_ref = self.imp().repo.borrow();
            let Some(ref repo) = *repo_ref else { return };

            // Determine if file is staged
            let is_staged = {
                let diffs = self.imp().cached_staged_diffs.borrow();
                diffs.iter().any(|f| f.path == file_path)
            };

            // Try to get diff for this file
            let diff_file = self.get_file_diff(repo, &file_path);

            if let Some(ref file) = diff_file {
                if let Some(tv) = changes_view::get_diff_textview(row) {
                    changes_view::render_file_diff(&tv, file);
                }
                // Populate hunk action buttons
                if let Some(hunk_box) = changes_view::get_hunk_actions_box(row) {
                    changes_view::populate_hunk_actions(&hunk_box, &file.hunks, is_staged);
                    // Connect hunk buttons
                    let win = self.clone();
                    let fp = file_path.clone();
                    let gesture = gtk::GestureClick::new();
                    gesture.connect_released(move |gesture, _, x, y| {
                        let Some(widget) = gesture.widget() else { return };
                        let Some(target) = widget.pick(x, y, gtk::PickFlags::DEFAULT) else { return };
                        let mut current = Some(target);
                        while let Some(w) = current {
                            if let Ok(btn) = w.clone().downcast::<gtk::Button>() {
                                let name = btn.widget_name().to_string();
                                if let Some(idx_str) = name.strip_prefix("stage-hunk-") {
                                    if let Ok(idx) = idx_str.parse::<usize>() {
                                        win.stage_hunk(&fp, idx);
                                    }
                                } else if let Some(idx_str) = name.strip_prefix("unstage-hunk-") {
                                    if let Ok(idx) = idx_str.parse::<usize>() {
                                        win.unstage_hunk(&fp, idx);
                                    }
                                } else if name == "stage-selected-lines" || name == "unstage-selected-lines" {
                                    win.stage_selected_lines(&fp, name == "unstage-selected-lines");
                                }
                                return;
                            }
                            current = w.parent();
                        }
                    });
                    hunk_box.add_controller(gesture);
                }
            }
        }
    }

    /// Get the diff for a single file from cached diffs, falling back to untracked.
    fn get_file_diff(&self, repo: &GitRepo, path: &str) -> Option<DiffFile> {
        // Try cached unstaged
        {
            let diffs = self.imp().cached_unstaged_diffs.borrow();
            if let Some(f) = diffs.iter().find(|f| f.path == path) {
                return Some(f.clone());
            }
        }

        // Try cached staged
        {
            let diffs = self.imp().cached_staged_diffs.borrow();
            if let Some(f) = diffs.iter().find(|f| f.path == path) {
                return Some(f.clone());
            }
        }

        // Try untracked (not cached since it's per-file)
        if let Ok(f) = repo.diff_untracked(path) {
            return Some(f);
        }

        None
    }

    /// Wire up per-row buttons (stage/unstage/discard) for changes view.
    fn setup_changes_row_button_signals(&self, list_box: &gtk::ListBox) {
        let win = self.clone();
        let gesture = gtk::GestureClick::new();
        gesture.connect_released(move |gesture, _, x, y| {
            let Some(widget) = gesture.widget() else { return };
            let Some(target) = widget.pick(x, y, gtk::PickFlags::DEFAULT) else { return };
            let mut current = Some(target);
            while let Some(w) = current {
                if let Ok(btn) = w.clone().downcast::<gtk::Button>() {
                    let name = btn.widget_name();
                    // Find the row this button belongs to
                    let mut parent = btn.parent();
                    while let Some(p) = parent {
                        if let Ok(row) = p.clone().downcast::<gtk::ListBoxRow>() {
                            if let Some(path) = changes_view::get_row_file_path(&row) {
                                if name == "stage-file" {
                                    win.stage_file(&path);
                                } else if name == "discard-file" {
                                    win.discard_file_with_confirm(&path);
                                } else if name == "unstage-file" {
                                    win.unstage_file(&path);
                                } else if name == "blame-file" {
                                    win.show_blame(&path);
                                } else if name == "history-file" {
                                    win.show_file_history(&path);
                                }
                            }
                            return;
                        }
                        parent = p.parent();
                    }
                    return;
                }
                current = w.parent();
            }
        });
        list_box.add_controller(gesture);
    }

    /// Populate branches and tags from pre-fetched data.
    fn populate_branches_tags_data(
        &self,
        branches: &[gitpulsar_core::models::BranchInfo],
        tags: &[gitpulsar_core::models::TagInfo],
    ) {
        let imp = self.imp();

        let local = imp.branches_local_list.borrow().clone();
        let remote = imp.branches_remote_list.borrow().clone();
        let tags_list = imp.tags_list.borrow().clone();

        let limit = imp.config.borrow().sidebar_items_limit;

        let local_count = branches.iter().filter(|b| !b.is_remote).count();
        let remote_count = branches.iter().filter(|b| b.is_remote).count();

        if let (Some(ref local), Some(ref remote)) = (local, remote) {
            branches_tags_panel::populate_branches(local, remote, branches);
            branches_tags_panel::update_section_header(local, "Local", local_count);
            branches_tags_panel::update_section_header(remote, "Remote", remote_count);
            branches_tags_panel::apply_row_limit(local, limit);
            branches_tags_panel::apply_row_limit(remote, limit);
        }

        if let Some(ref tl) = tags_list {
            branches_tags_panel::populate_tags(tl, tags);
            branches_tags_panel::update_section_header(tl, "Tags", tags.len());
            branches_tags_panel::apply_row_limit(tl, limit);
        }
    }

    fn populate_stashes_data(&self, entries: &[StashEntry]) {
        let imp = self.imp();
        let limit = imp.config.borrow().sidebar_items_limit;
        let stashes_list = imp.stashes_list.borrow().clone();
        if let Some(ref sl) = stashes_list {
            let win_apply = self.clone();
            let win_drop = self.clone();
            branches_tags_panel::populate_stashes(
                sl,
                entries,
                move |idx| win_apply.on_stash_apply(idx),
                move |idx| win_drop.on_stash_drop(idx),
            );
            branches_tags_panel::update_section_header(sl, "Stashes", entries.len());
            branches_tags_panel::apply_row_limit(sl, limit);
        }
    }

    fn populate_submodules_data(&self, submodules: &[SubmoduleInfo]) {
        let imp = self.imp();
        let limit = imp.config.borrow().sidebar_items_limit;
        let list = imp.submodules_list.borrow().clone();
        if let Some(ref sl) = list {
            let win = self.clone();
            branches_tags_panel::populate_submodules(sl, submodules, move |name| {
                win.run_git_op("Submodule Update", {
                    let name = name.clone();
                    move |path| {
                        let repo = GitRepo::open(path)?;
                        repo.submodule_update(&name)
                    }
                });
            });
            branches_tags_panel::update_section_header(sl, "Submodules", submodules.len());
            branches_tags_panel::apply_row_limit(sl, limit);
        }
    }

    fn populate_worktrees_data(&self, worktrees: &[WorktreeInfo]) {
        let imp = self.imp();
        let list = imp.worktrees_list.borrow().clone();
        if let Some(ref wl) = list {
            let win = self.clone();
            branches_tags_panel::populate_worktrees(wl, worktrees, move |path| {
                let p = std::path::PathBuf::from(&path);
                if p.is_dir() {
                    win.open_workspace(&p);
                }
            });
            // Only count extra worktrees (exclude main)
            let extra = if worktrees.len() > 1 { worktrees.len() } else { 0 };
            branches_tags_panel::update_section_header(wl, "Worktrees", extra);
        }
    }

    fn update_conflict_banner(&self, has_conflicts: bool, is_merging: bool, is_rebasing: bool) {
        // Find the banner box in the center content
        let toast_child = self.imp().toast_overlay.child();
        let banner_box = toast_child.as_ref()
            .and_then(|c| c.first_child())
            .and_then(|w| w.downcast::<gtk::Box>().ok());

        let Some(banner_box) = banner_box else { return };
        if banner_box.widget_name() != "conflict-banner-box" { return; }

        // Clear old banner
        while let Some(child) = banner_box.first_child() {
            banner_box.remove(&child);
        }

        if !has_conflicts && !is_merging && !is_rebasing {
            return;
        }

        let banner = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        banner.add_css_class("card");
        banner.set_margin_start(8);
        banner.set_margin_end(8);
        banner.set_margin_top(4);
        banner.set_margin_bottom(4);

        let icon = gtk::Image::builder()
            .icon_name("pull-request-merged-symbolic")
            .css_classes(["warning"])
            .build();
        banner.append(&icon);

        let msg = if has_conflicts {
            "Conflicts detected — resolve files and mark as resolved"
        } else if is_rebasing {
            "Rebase in progress"
        } else {
            "Merge in progress"
        };

        let label = gtk::Label::builder()
            .label(msg)
            .hexpand(true)
            .xalign(0.0)
            .build();
        banner.append(&label);

        if is_merging || is_rebasing {
            let continue_btn = gtk::Button::builder()
                .label("Continue")
                .css_classes(["suggested-action", "pill"])
                .build();
            let win = self.clone();
            let rebasing = is_rebasing;
            continue_btn.connect_clicked(move |_| {
                if rebasing {
                    win.run_git_op("Rebase Continue", |path| {
                        let repo = GitRepo::open(path)?;
                        repo.continue_rebase()
                    });
                } else {
                    win.run_git_op("Merge Continue", |path| {
                        let repo = GitRepo::open(path)?;
                        repo.continue_merge()
                    });
                }
            });
            banner.append(&continue_btn);

            let abort_btn = gtk::Button::builder()
                .label("Abort")
                .css_classes(["destructive-action", "pill"])
                .build();
            let win = self.clone();
            let rebasing = is_rebasing;
            abort_btn.connect_clicked(move |_| {
                if rebasing {
                    win.run_git_op("Rebase Abort", |path| {
                        let repo = GitRepo::open(path)?;
                        repo.abort_rebase()?;
                        Ok("Rebase aborted".to_string())
                    });
                } else {
                    win.run_git_op("Merge Abort", |path| {
                        let repo = GitRepo::open(path)?;
                        repo.abort_merge()?;
                        Ok("Merge aborted".to_string())
                    });
                }
            });
            banner.append(&abort_btn);
        }

        banner_box.append(&banner);
    }

    fn on_stash_apply(&self, index: usize) {
        self.run_git_op("Stash Apply", move |path| {
            let mut repo = GitRepo::open(path)?;
            repo.stash_apply(index)?;
            Ok(format!("Applied stash@{{{}}}", index))
        });
    }

    // ==========================================
    // TOAST HELPERS
    // ==========================================

    fn show_toast(&self, msg: &str) {
        let toast = adw::Toast::new(msg);
        toast.set_timeout(3);
        self.imp().toast_overlay.add_toast(toast);
    }

    fn show_error_dialog(&self, title: &str, body: &str) {
        let dialog = adw::AlertDialog::new(Some(title), Some(body));
        dialog.set_body_use_markup(false);
        dialog.add_response("ok", "OK");
        dialog.present(Some(self));
    }

    fn show_push_rejected_dialog(&self, _body: &str) {
        let dialog = adw::AlertDialog::new(
            Some("Push Rejected"),
            Some("Remote has new commits. Pull first, then push again."),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("pull", "Pull First");
        dialog.set_response_appearance("pull", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("pull"));
        dialog.set_close_response("cancel");

        let win = self.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "pull" {
                win.on_pull();
            }
        });
        dialog.present(Some(self));
    }

    // ==========================================
    // ASYNC HELPERS
    // ==========================================

    /// Get the repo path for spawning background git operations.
    fn repo_path_string(&self) -> Option<String> {
        let repo_ref = self.imp().repo.borrow();
        repo_ref
            .as_ref()
            .map(|r| r.path().to_string_lossy().to_string())
    }

    /// Set remote buttons sensitive/insensitive during operations.
    fn set_remote_buttons_sensitive(&self, sensitive: bool) {
        let imp = self.imp();
        imp.fetch_btn.set_sensitive(sensitive);
        imp.pull_btn.set_sensitive(sensitive);
        imp.push_btn.set_sensitive(sensitive);
    }

    /// Run a git operation in a background thread with UI feedback.
    /// `op_name` is used for error dialog titles.
    /// `op` receives a repo path and returns Result<String> (success message).
    fn run_git_op<F>(&self, op_name: &str, op: F)
    where
        F: FnOnce(&str) -> Result<String, anyhow::Error> + Send + 'static,
    {
        let Some(path) = self.repo_path_string() else {
            self.show_toast("No repository selected");
            return;
        };

        self.set_remote_buttons_sensitive(false);

        let (tx, rx) = async_channel::bounded::<Result<String, String>>(1);
        std::thread::spawn(move || {
            let result = op(&path).map_err(|e| format!("{:#}", e));
            tx.send_blocking(result).ok();
        });

        let win = self.clone();
        let title = op_name.to_string();
        glib::spawn_future_local(async move {
            if let Ok(result) = rx.recv().await {
                win.set_remote_buttons_sensitive(true);
                match result {
                    Ok(msg) => {
                        win.show_toast(&msg);
                        win.refresh_after_remote_op();
                    }
                    Err(e) => {
                        if title == "Push" && e.contains("non-fast-forward") {
                            win.show_push_rejected_dialog(&e);
                        } else {
                            win.show_error_dialog(&format!("{title} Failed"), &e);
                        }
                    }
                }
            } else {
                win.set_remote_buttons_sensitive(true);
            }
        });
    }

    // ==========================================
    // REMOTE OPERATIONS (async)
    // ==========================================

    fn on_fetch(&self) {
        self.run_git_op("Fetch", |path| {
            run_git_cmd(path, &["fetch", "--prune"])
                .map(|_| "Fetch complete".to_string())
        });
    }

    fn on_pull(&self) {
        self.run_git_op("Pull", |path| {
            run_git_cmd(path, &["pull", "--ff-only"])
                .map(|out| if out.trim().is_empty() { "Pull complete".to_string() } else { out })
        });
    }

    fn on_push(&self, force: bool) {
        self.run_git_op("Push", move |path| {
            let mut args = vec!["push", "-u", "origin", "HEAD"];
            if force {
                args.insert(1, "--force");
            }
            run_git_cmd(path, &args)
                .map(|_| if force { "Force push complete".to_string() } else { "Push complete".to_string() })
        });
    }

    fn show_force_push_dialog(&self) {
        let dialog = adw::AlertDialog::new(
            Some("Force Push?"),
            Some("This will overwrite the remote branch. This action cannot be undone and may cause others to lose work."),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("force-push", "Force Push");
        dialog.set_response_appearance("force-push", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let win = self.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "force-push" {
                win.on_push(true);
            }
        });
        dialog.present(Some(self));
    }

    fn refresh_after_remote_op(&self) {
        // Force commit list rebuild by resetting hash (push changes unpushed indicators)
        self.imp().last_commits_hash.set(0);
        self.imp().last_status_hash.set(0);
        // Re-open repo since the background thread may have changed state
        let path = self.repo_path_string();
        if let Some(path) = path {
            if let Ok(repo) = GitRepo::open(&path) {
                self.load_repo_data(&repo);
                *self.imp().repo.borrow_mut() = Some(repo);
            }
        }
        // Also refresh workspace sidebar indicators
        self.trigger_background_refresh();
    }

    // ==========================================
    // BRANCH OPERATIONS (async checkout)
    // ==========================================

    fn on_checkout_branch(&self, name: &str) {
        let branch = name.to_string();
        self.run_git_op("Checkout", move |path| {
            let repo = GitRepo::open(path)?;
            repo.checkout_branch(&branch)?;
            Ok(format!("Switched to {}", branch))
        });
    }

    fn on_checkout_remote_branch(&self, name: &str) {
        let remote_name = name.to_string();
        self.run_git_op("Checkout", move |path| {
            let repo = GitRepo::open(path)?;
            repo.checkout_remote_branch(&remote_name)?;
            let local = remote_name.split_once('/').map(|(_, n)| n).unwrap_or(&remote_name);
            Ok(format!("Switched to {}", local))
        });
    }

    fn show_create_branch_dialog(&self) {
        let dialog = adw::AlertDialog::new(
            Some("Create New Branch"),
            Some("Enter the name for the new branch (created from HEAD):"),
        );

        let entry = gtk::Entry::builder()
            .placeholder_text("branch-name")
            .activates_default(true)
            .build();
        dialog.set_extra_child(Some(&entry));

        dialog.add_response("cancel", "Cancel");
        dialog.add_response("create", "Create");
        dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("create"));
        dialog.set_close_response("cancel");

        let win = self.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "create" {
                let name = entry.text().to_string();
                let name = name.trim().to_string();
                if name.is_empty() {
                    return;
                }
                win.run_git_op("Create Branch", move |path| {
                    let repo = GitRepo::open(path)?;
                    repo.create_branch(&name, true)
                });
            }
        });
        dialog.present(Some(self));
    }

    // ==========================================
    // AUTO-REFRESH
    // ==========================================

    fn setup_keyboard_navigation(&self) {
        let win = self.clone();
        let key_ctrl = gtk::EventControllerKey::new();
        key_ctrl.connect_key_pressed(move |_, key, _, modifier| {
            let imp = win.imp();
            match key {
                gdk::Key::Tab if !imp.commit_entry.has_focus() && !modifier.contains(gdk::ModifierType::SHIFT_MASK) => {
                    win.cycle_focus(true);
                    glib::Propagation::Stop
                }
                gdk::Key::ISO_Left_Tab | gdk::Key::Tab
                    if !imp.commit_entry.has_focus() && modifier.contains(gdk::ModifierType::SHIFT_MASK) =>
                {
                    win.cycle_focus(false);
                    glib::Propagation::Stop
                }
                gdk::Key::Escape => {
                    // Collapse any expanded commit detail
                    if let Some(id) = imp.selected_commit_id.borrow().clone() {
                        let commits = imp.commits.borrow();
                        if let Some(idx) = commits.iter().position(|c| c.id == id) {
                            if let Some(row) = imp.commit_list_box.row_at_index(idx as i32) {
                                commit_list::toggle_detail(&row);
                            }
                        }
                    }
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.add_controller(key_ctrl);
    }

    fn cycle_focus(&self, forward: bool) {
        let imp = self.imp();
        let is_commits = imp.view_stack.visible_child_name().as_deref() == Some("commits");

        // Build list of focusable widgets for current view
        let unstaged = imp.unstaged_file_list.borrow().clone();
        let staged = imp.staged_file_list.borrow().clone();
        let branches = imp.branches_local_list.borrow().clone();

        let mut panels: Vec<&gtk::ListBox> = Vec::new();
        if is_commits {
            panels.push(&imp.commit_list_box);
        } else {
            if let Some(ref ul) = unstaged {
                panels.push(ul);
            }
            if let Some(ref sl) = staged {
                panels.push(sl);
            }
        }
        if let Some(ref bl) = branches {
            panels.push(bl);
        }

        if panels.is_empty() {
            return;
        }

        let mut idx = imp.focus_panel_index.get() as usize;
        if forward {
            idx = (idx + 1) % panels.len();
        } else {
            idx = idx.checked_sub(1).unwrap_or(panels.len() - 1);
        }
        imp.focus_panel_index.set(idx as u8);

        if let Some(panel) = panels.get(idx) {
            panel.grab_focus();
        }
    }

    fn setup_auto_refresh(&self) {
        let interval = self.imp().config.borrow().refresh_interval_secs;
        self.start_refresh_timer(interval);

        // Refresh immediately when window regains focus (e.g. switching back from another desktop)
        let win = self.clone();
        self.connect_is_active_notify(move |w| {
            if w.is_active() {
                win.trigger_background_refresh();
            }
        });
    }

    fn start_refresh_timer(&self, interval_secs: u32) {
        // Remove old timer if any
        if let Some(old_id) = self.imp().refresh_source_id.borrow_mut().take() {
            old_id.remove();
        }
        if interval_secs == 0 {
            return;
        }
        let win = self.downgrade();
        let source_id = glib::timeout_add_seconds_local(interval_secs, move || {
            let Some(win) = win.upgrade() else {
                return glib::ControlFlow::Break;
            };
            // Skip background refresh when window is not active (on another desktop/minimized)
            if win.is_active() {
                win.trigger_background_refresh();
            }
            glib::ControlFlow::Continue
        });
        *self.imp().refresh_source_id.borrow_mut() = Some(source_id);
    }

    fn show_blame(&self, path: &str) {
        let repo_ref = self.imp().repo.borrow();
        let Some(ref repo) = *repo_ref else { return };
        match repo.blame_file(path, None) {
            Ok(lines) => {
                drop(repo_ref);
                let dialog = blame_view::build_blame_dialog(path, &lines, |_commit_id| {
                    // Could navigate to commit — future enhancement
                });
                dialog.present(Some(self));
            }
            Err(e) => {
                self.show_toast(&format!("Blame failed: {}", e));
            }
        }
    }

    fn show_clone_dialog(&self) {
        let win = self.clone();
        let dialog = clone_dialog::build_clone_dialog(move |url, dest| {
            win.clone_repository(&url, &dest);
        });
        dialog.present(Some(self));
    }

    fn clone_repository(&self, url: &str, dest_dir: &str) {
        let url = url.to_string();
        let dest_dir = dest_dir.to_string();
        self.show_toast(&format!("Cloning {}…", url));

        let win = self.clone();
        let (tx, rx) = async_channel::bounded::<Result<String, String>>(1);

        std::thread::spawn(move || {
            use std::process::Command;
            let output = Command::new("git")
                .args(["clone", "--progress", &url])
                .current_dir(&dest_dir)
                .output();
            let result = match output {
                Ok(o) if o.status.success() => {
                    // Derive cloned repo name from URL
                    let name = url
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .unwrap_or("")
                        .trim_end_matches(".git")
                        .to_string();
                    let path = std::path::Path::new(&dest_dir).join(&name);
                    Ok(path.to_string_lossy().to_string())
                }
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                    Err(stderr.trim().to_string())
                }
                Err(e) => Err(format!("Failed to run git: {}", e)),
            };
            let _ = tx.send_blocking(result);
        });

        glib::spawn_future_local(async move {
            match rx.recv().await {
                Ok(Ok(path)) => {
                    win.show_toast(&format!("Cloned to {}", path));
                    win.open_workspace(std::path::Path::new(&path));
                }
                Ok(Err(e)) => {
                    win.show_error_dialog("Clone Failed", &e);
                }
                Err(_) => {}
            }
        });
    }

    fn export_commit_patch(&self, sha: &str) {
        let Some(repo_path) = self.repo_path_string() else { return };
        let sha = sha.to_string();
        let default_name = format!("{}.patch", &sha[..7.min(sha.len())]);

        let dialog = gtk::FileChooserDialog::new(
            Some("Save patch as"),
            Some(self),
            gtk::FileChooserAction::Save,
            &[
                ("Cancel", gtk::ResponseType::Cancel),
                ("Save", gtk::ResponseType::Accept),
            ],
        );
        dialog.set_modal(true);
        dialog.set_current_name(&default_name);

        let win = self.clone();
        dialog.connect_response(move |dialog, response| {
            if response == gtk::ResponseType::Accept {
                if let Some(file) = dialog.file() {
                    if let Some(path) = file.path() {
                        match run_git_cmd(&repo_path, &["format-patch", "-1", "--stdout", &sha]) {
                            Ok(content) => {
                                if let Err(e) = std::fs::write(&path, content) {
                                    win.show_error_dialog("Export Failed", &format!("Failed to write patch: {}", e));
                                } else {
                                    win.show_toast(&format!("Patch saved: {}", path.display()));
                                }
                            }
                            Err(e) => {
                                win.show_error_dialog("Export Failed", &format!("{}", e));
                            }
                        }
                    }
                }
            }
            dialog.close();
        });
        dialog.present();
    }

    fn show_apply_patch_dialog(&self) {
        let Some(repo_path) = self.repo_path_string() else {
            self.show_toast("Open a repository first");
            return;
        };

        let dialog = gtk::FileChooserDialog::new(
            Some("Select patch file to apply"),
            Some(self),
            gtk::FileChooserAction::Open,
            &[
                ("Cancel", gtk::ResponseType::Cancel),
                ("Apply", gtk::ResponseType::Accept),
            ],
        );
        dialog.set_modal(true);

        let win = self.clone();
        dialog.connect_response(move |dialog, response| {
            if response == gtk::ResponseType::Accept {
                if let Some(file) = dialog.file() {
                    if let Some(path) = file.path() {
                        let path_str = path.to_string_lossy().to_string();
                        match run_git_cmd(&repo_path, &["am", "--3way", &path_str]) {
                            Ok(_) => {
                                win.show_toast(&format!("Applied patch: {}", path.display()));
                                win.trigger_background_refresh();
                            }
                            Err(e) => {
                                // If git am fails, try git apply (for patches without commit metadata)
                                match run_git_cmd(&repo_path, &["apply", "--3way", &path_str]) {
                                    Ok(_) => {
                                        win.show_toast(&format!("Applied patch (no commit): {}", path.display()));
                                        win.trigger_background_refresh();
                                    }
                                    Err(e2) => {
                                        win.show_error_dialog(
                                            "Apply Patch Failed",
                                            &format!("git am: {}\n\ngit apply: {}", e, e2),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            dialog.close();
        });
        dialog.present();
    }

    fn show_reflog_dialog(&self) {
        let repo_ref = self.imp().repo.borrow();
        let Some(ref repo) = *repo_ref else {
            self.show_toast("Open a repository first");
            return;
        };
        match repo.reflog(500) {
            Ok(entries) => {
                drop(repo_ref);
                let dialog = reflog_dialog::build_reflog_dialog(&entries);
                dialog.present(Some(self));
            }
            Err(e) => self.show_error_dialog("Reflog Failed", &format!("{}", e)),
        }
    }

    fn show_remotes_dialog(&self) {
        let repo_ref = self.imp().repo.borrow();
        let Some(ref repo) = *repo_ref else {
            self.show_toast("Open a repository first");
            return;
        };
        let remotes = repo.remotes().unwrap_or_default();
        drop(repo_ref);

        let win = self.clone();
        let dialog_holder: std::rc::Rc<std::cell::RefCell<Option<adw::Dialog>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let dh = dialog_holder.clone();
        let dialog = remotes_dialog::build_remotes_dialog(&remotes, move |action| {
            use remotes_dialog::RemoteAction;
            let repo_ref = win.imp().repo.borrow();
            let Some(ref repo) = *repo_ref else { return };
            let result = match action {
                RemoteAction::Add { name, url } => {
                    repo.add_remote(&name, &url).map(|_| format!("Added remote '{}'", name))
                }
                RemoteAction::Remove { name } => {
                    repo.remove_remote(&name).map(|_| format!("Removed remote '{}'", name))
                }
                RemoteAction::Rename { old, new } => repo
                    .rename_remote(&old, &new)
                    .map(|_| format!("Renamed '{}' → '{}'", old, new)),
                RemoteAction::SetUrl { name, url } => repo
                    .set_remote_url(&name, &url)
                    .map(|_| format!("Updated URL for '{}'", name)),
            };
            drop(repo_ref);
            match result {
                Ok(msg) => {
                    win.show_toast(&msg);
                    if let Some(d) = dh.borrow_mut().take() {
                        d.close();
                    }
                    win.show_remotes_dialog();
                }
                Err(e) => win.show_error_dialog("Remote Operation Failed", &format!("{}", e)),
            }
        });
        *dialog_holder.borrow_mut() = Some(dialog.clone());
        dialog.present(Some(self));
    }

    fn show_file_history(&self, path: &str) {
        let repo_ref = self.imp().repo.borrow();
        let Some(ref repo) = *repo_ref else { return };
        match repo.log_for_file(path, 200) {
            Ok(commits) => {
                drop(repo_ref);
                let dialog = file_history_dialog::build_file_history_dialog(path, &commits);
                dialog.present(Some(self));
            }
            Err(e) => {
                self.show_toast(&format!("File history failed: {}", e));
            }
        }
    }

    fn open_conflict_editor(&self, path: &str, chunks: &[gitpulsar_core::conflict::ConflictChunk]) {
        let repo_path = self.repo_path_string();
        let Some(rp) = repo_path else { return };
        let fp = path.to_string();

        let win = self.clone();
        let dialog = conflict_editor::build_conflict_editor(path, chunks, move |resolved_content| {
            if let Ok(repo) = GitRepo::open(&rp) {
                match repo.save_resolved_file(&fp, &resolved_content) {
                    Ok(()) => {
                        win.show_toast(&format!("Resolved: {}", fp));
                        win.refresh_staging();
                        win.trigger_background_refresh();
                    }
                    Err(e) => win.show_toast(&format!("Error: {}", e)),
                }
            }
        });
        dialog.present(Some(self));
    }

    fn show_rebase_editor(&self, commit_count: usize) {
        let repo_ref = self.imp().repo.borrow();
        let Some(ref repo) = *repo_ref else { return };
        let entries = match repo.list_rebase_commits(commit_count) {
            Ok(e) => e,
            Err(e) => {
                self.show_toast(&format!("Failed to prepare rebase: {}", e));
                return;
            }
        };
        let onto = format!("HEAD~{}", commit_count);
        drop(repo_ref);

        let win = self.clone();
        let dialog = rebase_editor::build_rebase_editor(&entries, &onto, move |modified, onto| {
            win.run_git_op("Interactive Rebase", move |path| {
                let repo = GitRepo::open(path)?;
                repo.execute_rebase(&modified, &onto)
            });
        });
        dialog.present(Some(self));
    }

    fn open_gitignore_editor(&self) {
        let repo_ref = self.imp().repo.borrow();
        let Some(ref repo) = *repo_ref else { return };
        let content = repo.read_gitignore().unwrap_or_default();
        let repo_path = repo.path().to_string_lossy().to_string();
        drop(repo_ref);

        let win = self.clone();
        let dialog = gitignore_editor::build_gitignore_editor(&content, move |new_content| {
            if let Ok(repo) = GitRepo::open(&repo_path) {
                match repo.write_gitignore(&new_content) {
                    Ok(()) => {
                        win.show_toast("Saved .gitignore");
                        win.trigger_background_refresh();
                    }
                    Err(e) => win.show_toast(&format!("Error: {}", e)),
                }
            }
        });
        dialog.present(Some(self));
    }

    fn open_preferences(&self) {
        let config = self.imp().config.borrow().clone();
        let win = self.clone();
        let dialog = preferences_dialog::build_preferences_dialog(&config, move |new_config| {
            // Sentinel: u32::MAX means "refresh now"
            if new_config.refresh_interval_secs == u32::MAX {
                win.trigger_background_refresh();
                return;
            }

            let old_config = win.imp().config.borrow().clone();
            let date_changed = old_config.date_format != new_config.date_format;
            let interval_changed = old_config.refresh_interval_secs != new_config.refresh_interval_secs;

            new_config.save();
            *win.imp().config.borrow_mut() = new_config.clone();

            if date_changed {
                // Force commit list rebuild by resetting hash
                win.imp().last_commits_hash.set(0);
                let has_repo = win.imp().repo.borrow().is_some();
                if has_repo {
                    // Re-populate commit list with new date format
                    let commits = win.imp().commits.borrow().clone();
                    let tags_map = {
                        let repo_ref = win.imp().repo.borrow();
                        repo_ref.as_ref()
                            .and_then(|r| r.tags_by_commit().ok())
                            .unwrap_or_default()
                    };
                    let ahead = win.imp().ahead_label.label()
                        .strip_prefix("▲ ")
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);
                    // Clear so offset=0 for full rebuild
                    win.imp().commits.borrow_mut().clear();
                    win.populate_commit_list(&commits, ahead, &tags_map);
                    *win.imp().commits.borrow_mut() = commits;
                }
            }

            if interval_changed {
                win.start_refresh_timer(new_config.refresh_interval_secs);
            }
        });
        dialog.present(Some(self));
    }

    /// Run status + workspace scan in a background thread, then apply results on UI thread.
    fn trigger_background_refresh(&self) {
        let imp = self.imp();

        // Guard against concurrent refreshes
        if imp.refresh_in_progress.get() {
            return;
        }
        imp.refresh_in_progress.set(true);

        // Collect paths needed for background work
        let repo_path = {
            let repo_ref = imp.repo.borrow();
            repo_ref.as_ref().map(|r| r.path().to_string_lossy().to_string())
        };
        let workspace_root = {
            let entries = imp.workspace_entries.borrow();
            if entries.is_empty() {
                None
            } else {
                Some(
                    entries[0]
                        .path
                        .parent()
                        .unwrap_or(&entries[0].path)
                        .to_path_buf(),
                )
            }
        };

        let prev_status_hash = imp.last_status_hash.get();
        // Throttle workspace scans: only every 4th tick (~60s at 15s interval)
        let tick = imp.refresh_tick.get();
        imp.refresh_tick.set(tick.wrapping_add(1));
        let scan_workspace = tick % 4 == 0;
        let (tx, rx) = async_channel::bounded::<BackgroundRefreshResult>(1);

        std::thread::spawn(move || {
            // Run workspace scan only every Nth tick to reduce CPU
            let workspace_handle = if scan_workspace {
                workspace_root.map(|root| {
                    std::thread::spawn(move || workspace::scan_workspace(&root).ok())
                })
            } else {
                None
            };

            let (status, ahead, behind, unstaged_diffs, staged_diffs) =
                if let Some(ref p) = repo_path {
                    if let Ok(repo) = GitRepo::open(p) {
                        let st = repo.status(false).ok();
                        let (a, b) = repo.ahead_behind().unwrap_or((0, 0));
                        // Only compute diffs if status actually changed
                        let status_hash = st.as_ref().map(|s| hash_status(s)).unwrap_or(0);
                        let (ud, sd) = if status_hash != prev_status_hash {
                            (repo.diff_unstaged().unwrap_or_default(),
                             repo.diff_staged().unwrap_or_default())
                        } else {
                            (Vec::new(), Vec::new())
                        };
                        (st, a, b, ud, sd)
                    } else {
                        (None, 0, 0, Vec::new(), Vec::new())
                    }
                } else {
                    (None, 0, 0, Vec::new(), Vec::new())
                };
            let status_hash = status.as_ref().map(|s| hash_status(s)).unwrap_or(0);

            let workspace_entries = workspace_handle
                .and_then(|h| h.join().ok())
                .flatten();
            let workspace_hash = workspace_entries.as_ref().map(|e| hash_workspace(e)).unwrap_or(0);

            tx.send_blocking(BackgroundRefreshResult {
                status,
                status_hash,
                workspace_entries,
                workspace_hash,
                ahead,
                behind,
                unstaged_diffs,
                staged_diffs,
            }).ok();
        });

        let win = self.clone();
        glib::spawn_future_local(async move {
            let result = rx.recv().await;
            let imp = win.imp();
            imp.refresh_in_progress.set(false);

            let Ok(result) = result else { return };

            // Apply status only if changed
            if let Some(status) = result.status {
                if result.status_hash != imp.last_status_hash.get() {
                    imp.last_status_hash.set(result.status_hash);
                    win.refresh_changes_list(&status);
                    // Update diff cache
                    *imp.cached_unstaged_diffs.borrow_mut() = result.unstaged_diffs;
                    *imp.cached_staged_diffs.borrow_mut() = result.staged_diffs;
                    // Update sidebar status and active repo indicator
                    if let Some(ref path_str) = win.repo_path_string() {
                        let name = std::path::Path::new(path_str)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        win.update_sidebar_status(&name, Some(&status));
                        win.refresh_active_repo_indicator(path_str);
                    }
                }
            }

            if result.ahead != imp.last_ahead.get() || result.behind != imp.last_behind.get() {
                imp.last_ahead.set(result.ahead);
                imp.last_behind.set(result.behind);
                imp.ahead_label.set_label(&format!("▲ {}", result.ahead));
                imp.behind_label.set_label(&format!("▼ {}", result.behind));
            }

            // Apply workspace indicators only if changed
            if let Some(new_entries) = result.workspace_entries {
                if result.workspace_hash != imp.last_workspace_hash.get() {
                    imp.last_workspace_hash.set(result.workspace_hash);
                    let selected_idx =
                        imp.repo_list_box.selected_row().map(|r| r.index());
                    repo_tree::populate_repo_list(&imp.repo_list_box, &new_entries);
                    *imp.workspace_entries.borrow_mut() = new_entries;
                    if let Some(idx) = selected_idx {
                        if let Some(row) = imp.repo_list_box.row_at_index(idx) {
                            imp.repo_list_box.select_row(Some(&row));
                        }
                    }
                }
            }
        });
    }

    // ==========================================
    // STASH OPERATIONS
    // ==========================================

    fn on_stash_save(&self) {
        self.run_git_op("Stash Save", |path| {
            let mut repo = GitRepo::open(path)?;
            repo.stash_save(None)
        });
    }

    fn on_stash_pop(&self) {
        self.run_git_op("Stash Pop", |path| {
            let mut repo = GitRepo::open(path)?;
            repo.stash_pop()?;
            Ok("Stash popped".to_string())
        });
    }

    fn on_stash_drop(&self, index: usize) {
        self.run_git_op("Stash Drop", move |path| {
            let mut repo = GitRepo::open(path)?;
            repo.stash_drop(index)?;
            Ok(format!("Dropped stash@{{{}}}", index))
        });
    }

    fn build_stash_popover_content(&self, popover: &gtk::Popover) {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
        content.set_margin_top(8);
        content.set_margin_bottom(8);
        content.set_margin_start(8);
        content.set_margin_end(8);
        content.set_width_request(300);

        let header = gtk::Label::builder()
            .label("Stash List")
            .css_classes(["heading"])
            .xalign(0.0)
            .build();
        content.append(&header);

        let repo_ref = self.imp().repo.borrow();
        if let Some(ref repo) = *repo_ref {
            // Need mutable for stash_list — re-open
            let path = repo.path().to_string_lossy().to_string();
            drop(repo_ref);

            if let Ok(mut repo) = GitRepo::open(&path) {
                match repo.stash_list() {
                    Ok(entries) if !entries.is_empty() => {
                        let list = gtk::ListBox::builder()
                            .selection_mode(gtk::SelectionMode::None)
                            .css_classes(["boxed-list"])
                            .build();

                        for entry in &entries {
                            let row = adw::ActionRow::builder()
                                .title(&format!("stash@{{{}}}", entry.index))
                                .subtitle(&entry.message)
                                .build();

                            let pop_btn = gtk::Button::builder()
                                .icon_name("go-up-symbolic")
                                .css_classes(["flat", "circular"])
                                .tooltip_text("Pop")
                                .valign(gtk::Align::Center)
                                .build();
                            let win = self.clone();
                            let pp = popover.clone();
                            pop_btn.connect_clicked(move |_| {
                                pp.popdown();
                                win.on_stash_pop();
                            });

                            let drop_btn = gtk::Button::builder()
                                .icon_name("user-trash-symbolic")
                                .css_classes(["flat", "circular"])
                                .tooltip_text("Drop")
                                .valign(gtk::Align::Center)
                                .build();
                            let win = self.clone();
                            let pp2 = popover.clone();
                            let idx = entry.index;
                            drop_btn.connect_clicked(move |_| {
                                pp2.popdown();
                                win.on_stash_drop(idx);
                            });

                            row.add_suffix(&pop_btn);
                            row.add_suffix(&drop_btn);
                            list.append(&row);
                        }

                        let scrolled = gtk::ScrolledWindow::builder()
                            .max_content_height(250)
                            .propagate_natural_height(true)
                            .hscrollbar_policy(gtk::PolicyType::Never)
                            .build();
                        scrolled.set_child(Some(&list));
                        content.append(&scrolled);
                    }
                    _ => {
                        let empty = gtk::Label::builder()
                            .label("No stashes")
                            .css_classes(["dim-label"])
                            .margin_top(12)
                            .margin_bottom(12)
                            .build();
                        content.append(&empty);
                    }
                }
            }
        } else {
            drop(repo_ref);
            let empty = gtk::Label::builder()
                .label("No repository selected")
                .css_classes(["dim-label"])
                .margin_top(12)
                .margin_bottom(12)
                .build();
            content.append(&empty);
        }

        popover.set_child(Some(&content));
    }

    // ==========================================
    // COMMIT CONTEXT MENU
    // ==========================================

    fn setup_commit_context_menu(&self) {
        let gesture = gtk::GestureClick::new();
        gesture.set_button(3); // right-click

        let win = self.clone();
        gesture.connect_released(move |_gesture, _, x, y| {
            let imp = win.imp();
            // Find which row was clicked
            let Some(row) = imp.commit_list_box.row_at_y(y as i32) else {
                return;
            };
            let idx = row.index() as usize;

            let commits = imp.commits.borrow();
            let Some(commit) = commits.get(idx) else {
                return;
            };
            let sha = commit.id.clone();
            let short_sha = commit.short_id.clone();
            let message = commit.summary.clone();
            let full_message = commit.message.clone();
            drop(commits);

            // Build popover menu
            let popover = gtk::Popover::new();
            popover.set_parent(&imp.commit_list_box);
            popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.set_has_arrow(true);

            let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            menu_box.set_margin_top(4);
            menu_box.set_margin_bottom(4);

            // Copy SHA
            let copy_sha_btn = gtk::Button::builder()
                .label(&format!("Copy SHA ({})", short_sha))
                .css_classes(["flat"])
                .build();
            let sha_clone = sha.clone();
            let pp = popover.clone();
            let w = win.clone();
            copy_sha_btn.connect_clicked(move |_| {
                if let Some(display) = gdk::Display::default() {
                    display.clipboard().set_text(&sha_clone);
                    w.show_toast("SHA copied to clipboard");
                }
                pp.popdown();
            });
            menu_box.append(&copy_sha_btn);

            // Copy Message
            let copy_msg_btn = gtk::Button::builder()
                .label("Copy Message")
                .css_classes(["flat"])
                .build();
            let msg_clone = message.clone();
            let pp2 = popover.clone();
            let w2 = win.clone();
            copy_msg_btn.connect_clicked(move |_| {
                if let Some(display) = gdk::Display::default() {
                    display.clipboard().set_text(&msg_clone);
                    w2.show_toast("Message copied to clipboard");
                }
                pp2.popdown();
            });
            menu_box.append(&copy_msg_btn);

            // Checkout commit (detached HEAD)
            let checkout_btn = gtk::Button::builder()
                .label("Checkout This Commit")
                .css_classes(["flat"])
                .build();
            let sha_clone = sha.clone();
            let pp3 = popover.clone();
            let w3 = win.clone();
            checkout_btn.connect_clicked(move |_| {
                pp3.popdown();
                let sha = sha_clone.clone();
                w3.run_git_op("Checkout Commit", move |path| {
                    let repo = GitRepo::open(path)?;
                    repo.checkout_detached(&sha)?;
                    Ok(format!("HEAD detached at {}", &sha[..7]))
                });
            });
            menu_box.append(&checkout_btn);

            // Separator
            menu_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

            // Cherry-pick
            let cherry_pick_btn = gtk::Button::builder()
                .label("Cherry-pick")
                .css_classes(["flat"])
                .build();
            let sha_clone = sha.clone();
            let pp4 = popover.clone();
            let w4 = win.clone();
            cherry_pick_btn.connect_clicked(move |_| {
                pp4.popdown();
                let sha = sha_clone.clone();
                w4.run_git_op("Cherry-pick", move |path| {
                    let repo = GitRepo::open(path)?;
                    repo.cherry_pick(&sha)
                });
            });
            menu_box.append(&cherry_pick_btn);

            // Revert
            let revert_btn = gtk::Button::builder()
                .label("Revert Commit")
                .css_classes(["flat"])
                .build();
            let sha_clone = sha.clone();
            let pp5 = popover.clone();
            let w5 = win.clone();
            revert_btn.connect_clicked(move |_| {
                pp5.popdown();
                let sha = sha_clone.clone();
                w5.show_revert_confirm_dialog(&sha);
            });
            menu_box.append(&revert_btn);

            // Separator before Reset
            menu_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

            // Reset Soft
            let reset_soft_btn = gtk::Button::builder()
                .label("Reset Soft to Here")
                .css_classes(["flat"])
                .build();
            let sha_clone = sha.clone();
            let pp_rs = popover.clone();
            let w_rs = win.clone();
            reset_soft_btn.connect_clicked(move |_| {
                pp_rs.popdown();
                w_rs.show_reset_confirm_dialog(&sha_clone, ResetMode::Soft);
            });
            menu_box.append(&reset_soft_btn);

            // Reset Mixed
            let reset_mixed_btn = gtk::Button::builder()
                .label("Reset Mixed to Here")
                .css_classes(["flat"])
                .build();
            let sha_clone = sha.clone();
            let pp_rm = popover.clone();
            let w_rm = win.clone();
            reset_mixed_btn.connect_clicked(move |_| {
                pp_rm.popdown();
                w_rm.show_reset_confirm_dialog(&sha_clone, ResetMode::Mixed);
            });
            menu_box.append(&reset_mixed_btn);

            // Reset Hard
            let reset_hard_btn = gtk::Button::builder()
                .label("Reset Hard to Here")
                .css_classes(["flat"])
                .build();
            let sha_clone = sha.clone();
            let pp_rh = popover.clone();
            let w_rh = win.clone();
            reset_hard_btn.connect_clicked(move |_| {
                pp_rh.popdown();
                w_rh.show_reset_confirm_dialog(&sha_clone, ResetMode::Hard);
            });
            menu_box.append(&reset_hard_btn);

            // Create Tag
            let create_tag_btn = gtk::Button::builder()
                .label("Create Tag…")
                .css_classes(["flat"])
                .build();
            let sha_clone = sha.clone();
            let pp6 = popover.clone();
            let w6 = win.clone();
            create_tag_btn.connect_clicked(move |_| {
                pp6.popdown();
                w6.show_create_tag_dialog(&sha_clone);
            });
            menu_box.append(&create_tag_btn);

            // Export as Patch
            let export_patch_btn = gtk::Button::builder()
                .label("Export as Patch…")
                .css_classes(["flat"])
                .build();
            let sha_clone = sha.clone();
            let pp_ep = popover.clone();
            let w_ep = win.clone();
            export_patch_btn.connect_clicked(move |_| {
                pp_ep.popdown();
                w_ep.export_commit_patch(&sha_clone);
            });
            menu_box.append(&export_patch_btn);

            // Interactive Rebase (onto this commit)
            if idx > 0 {
                let rebase_btn = gtk::Button::builder()
                    .label("Interactive Rebase…")
                    .css_classes(["flat"])
                    .build();
                let pp_rb = popover.clone();
                let w_rb = win.clone();
                let commit_count = idx;
                rebase_btn.connect_clicked(move |_| {
                    pp_rb.popdown();
                    w_rb.show_rebase_editor(commit_count);
                });
                menu_box.append(&rebase_btn);
            }

            // Edit commit message (only for HEAD / idx==0)
            if idx == 0 {
                let edit_msg_btn = gtk::Button::builder()
                    .label("Edit Commit Message")
                    .css_classes(["flat"])
                    .build();
                let pp7 = popover.clone();
                let w7 = win.clone();
                edit_msg_btn.connect_clicked(move |_| {
                    pp7.popdown();
                    w7.show_edit_message_dialog(&full_message);
                });
                menu_box.append(&edit_msg_btn);
            }

            popover.set_child(Some(&menu_box));
            popover.popup();
        });

        self.imp().commit_list_box.add_controller(gesture);
    }

    fn setup_tag_context_menu(&self) {
        let tags_list = self.imp().tags_list.borrow().clone();
        let Some(tags_list) = tags_list else { return };

        let gesture = gtk::GestureClick::new();
        gesture.set_button(3); // right-click

        let win = self.clone();
        let tl = tags_list.clone();
        gesture.connect_released(move |_gesture, _, x, y| {
            let Some(row) = tl.row_at_y(y as i32) else { return };
            let tag_name = row.widget_name().to_string();
            if tag_name.is_empty() { return; }

            let popover = gtk::Popover::new();
            popover.set_parent(&tl);
            popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.set_has_arrow(true);

            let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            menu_box.set_margin_top(4);
            menu_box.set_margin_bottom(4);

            // Push to remote
            let push_btn = gtk::Button::builder()
                .label("Push to remote")
                .css_classes(["flat"])
                .build();
            let name = tag_name.clone();
            let pp = popover.clone();
            let w = win.clone();
            push_btn.connect_clicked(move |_| {
                pp.popdown();
                w.push_tag(&name);
            });
            menu_box.append(&push_btn);

            // Delete remote
            let delete_remote_btn = gtk::Button::builder()
                .label("Delete from remote")
                .css_classes(["flat"])
                .build();
            let name = tag_name.clone();
            let pp = popover.clone();
            let w = win.clone();
            delete_remote_btn.connect_clicked(move |_| {
                pp.popdown();
                w.delete_remote_tag(&name);
            });
            menu_box.append(&delete_remote_btn);

            // Delete local
            let delete_btn = gtk::Button::builder()
                .label("Delete locally")
                .css_classes(["flat"])
                .build();
            let name = tag_name.clone();
            let pp = popover.clone();
            let w = win.clone();
            delete_btn.connect_clicked(move |_| {
                pp.popdown();
                w.delete_local_tag(&name);
            });
            menu_box.append(&delete_btn);

            popover.set_child(Some(&menu_box));
            popover.popup();
        });

        tags_list.add_controller(gesture);
    }

    fn push_tag(&self, name: &str) {
        let name = name.to_string();
        self.run_git_op("Push Tag", move |path| {
            run_git_cmd(path, &["push", "origin", &format!("refs/tags/{}", name)])
                .map(|_| format!("Pushed tag '{}'", name))
        });
    }

    fn delete_remote_tag(&self, name: &str) {
        let dialog = adw::AlertDialog::new(
            Some("Delete Remote Tag?"),
            Some(&format!("Delete tag '{}' from origin? This affects everyone using the remote.", name)),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("delete", "Delete Remote");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let win = self.clone();
        let name_owned = name.to_string();
        dialog.connect_response(None, move |_, response| {
            if response == "delete" {
                let name = name_owned.clone();
                win.run_git_op("Delete Remote Tag", move |path| {
                    run_git_cmd(path, &["push", "origin", &format!(":refs/tags/{}", name)])
                        .map(|_| format!("Deleted remote tag '{}'", name))
                });
            }
        });
        dialog.present(Some(self));
    }

    fn delete_local_tag(&self, name: &str) {
        let dialog = adw::AlertDialog::new(
            Some("Delete Tag?"),
            Some(&format!("Delete local tag '{}'?", name)),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("delete", "Delete");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let win = self.clone();
        let name_owned = name.to_string();
        dialog.connect_response(None, move |_, response| {
            if response == "delete" {
                let name = name_owned.clone();
                win.run_git_op("Delete Tag", move |path| {
                    let repo = GitRepo::open(path)?;
                    repo.delete_tag(&name)?;
                    Ok(format!("Deleted tag '{}'", name))
                });
            }
        });
        dialog.present(Some(self));
    }

    fn setup_branch_context_menu(&self) {
        let local_list = self.imp().branches_local_list.borrow().clone();
        let Some(local_list) = local_list else { return };

        let gesture = gtk::GestureClick::new();
        gesture.set_button(3); // right-click

        let win = self.clone();
        let ll = local_list.clone();
        gesture.connect_released(move |_gesture, _, x, y| {
            let Some(row) = ll.row_at_y(y as i32) else { return };
            let branch_name = row.widget_name().to_string();
            if branch_name.is_empty() { return; }

            // Check if this is the current branch
            let is_head = win.imp().branch_label.label().as_str() == branch_name;

            let popover = gtk::Popover::new();
            popover.set_parent(&ll);
            popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.set_has_arrow(true);

            let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            menu_box.set_margin_top(4);
            menu_box.set_margin_bottom(4);

            // Checkout
            if !is_head {
                let checkout_btn = gtk::Button::builder()
                    .label("Checkout")
                    .css_classes(["flat"])
                    .build();
                let name = branch_name.clone();
                let pp = popover.clone();
                let w = win.clone();
                checkout_btn.connect_clicked(move |_| {
                    pp.popdown();
                    w.on_checkout_branch(&name);
                });
                menu_box.append(&checkout_btn);
            }

            // Rename
            let rename_btn = gtk::Button::builder()
                .label("Rename…")
                .css_classes(["flat"])
                .build();
            let name = branch_name.clone();
            let pp = popover.clone();
            let w = win.clone();
            rename_btn.connect_clicked(move |_| {
                pp.popdown();
                w.show_rename_branch_dialog(&name);
            });
            menu_box.append(&rename_btn);

            // Delete (disabled for current branch)
            let delete_btn = gtk::Button::builder()
                .label("Delete")
                .css_classes(["flat"])
                .build();
            if is_head {
                delete_btn.set_sensitive(false);
                delete_btn.set_tooltip_text(Some("Cannot delete the current branch"));
            }
            let name = branch_name.clone();
            let pp = popover.clone();
            let w = win.clone();
            delete_btn.connect_clicked(move |_| {
                pp.popdown();
                w.show_delete_branch_dialog(&name);
            });
            menu_box.append(&delete_btn);

            popover.set_child(Some(&menu_box));
            popover.popup();
        });

        local_list.add_controller(gesture);
    }

    fn show_rename_branch_dialog(&self, old_name: &str) {
        let dialog = adw::AlertDialog::new(
            Some("Rename Branch"),
            Some(&format!("Rename branch '{}':", old_name)),
        );

        let entry = gtk::Entry::builder()
            .placeholder_text("new-name")
            .text(old_name)
            .activates_default(true)
            .build();
        dialog.set_extra_child(Some(&entry));

        dialog.add_response("cancel", "Cancel");
        dialog.add_response("rename", "Rename");
        dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("rename"));
        dialog.set_close_response("cancel");

        let win = self.clone();
        let old = old_name.to_string();
        dialog.connect_response(None, move |_, response| {
            if response == "rename" {
                let new_name = entry.text().to_string();
                let new_name = new_name.trim().to_string();
                if new_name.is_empty() || new_name == old {
                    return;
                }
                let old = old.clone();
                win.run_git_op("Rename Branch", move |path| {
                    let repo = GitRepo::open(path)?;
                    repo.rename_branch(&old, &new_name)?;
                    Ok(format!("Renamed '{}' → '{}'", old, new_name))
                });
            }
        });
        dialog.present(Some(self));
    }

    fn show_delete_branch_dialog(&self, name: &str) {
        let dialog = adw::AlertDialog::new(
            Some("Delete Branch?"),
            Some(&format!("Delete branch '{}'? This cannot be undone.", name)),
        );

        let force_check = gtk::CheckButton::builder()
            .label("Force delete (even if unmerged)")
            .build();
        dialog.set_extra_child(Some(&force_check));

        dialog.add_response("cancel", "Cancel");
        dialog.add_response("delete", "Delete");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let win = self.clone();
        let branch = name.to_string();
        dialog.connect_response(None, move |_, response| {
            if response == "delete" {
                let branch = branch.clone();
                let force = force_check.is_active();
                win.run_git_op("Delete Branch", move |path| {
                    let repo = GitRepo::open(path)?;
                    repo.delete_branch(&branch, force)?;
                    Ok(format!("Deleted branch '{}'", branch))
                });
            }
        });
        dialog.present(Some(self));
    }

    fn show_revert_confirm_dialog(&self, commit_id: &str) {
        let dialog = adw::AlertDialog::new(
            Some("Revert Commit?"),
            Some("This will create a new commit that undoes the changes of the selected commit."),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("revert", "Revert");
        dialog.set_response_appearance("revert", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let win = self.clone();
        let sha = commit_id.to_string();
        dialog.connect_response(None, move |_, response| {
            if response == "revert" {
                let sha = sha.clone();
                win.run_git_op("Revert", move |path| {
                    let repo = GitRepo::open(path)?;
                    repo.revert_commit(&sha)
                });
            }
        });
        dialog.present(Some(self));
    }

    fn show_reset_confirm_dialog(&self, commit_id: &str, mode: ResetMode) {
        let mode_str = match mode {
            ResetMode::Soft => "Soft",
            ResetMode::Mixed => "Mixed",
            ResetMode::Hard => "Hard",
        };
        let short = &commit_id[..7.min(commit_id.len())];
        let body = match mode {
            ResetMode::Hard => format!(
                "Reset HEAD to {}? This will discard ALL uncommitted changes (working directory and index).",
                short
            ),
            ResetMode::Mixed => format!(
                "Reset HEAD to {}? Staged changes will be unstaged, working directory unchanged.",
                short
            ),
            ResetMode::Soft => format!(
                "Reset HEAD to {}? Index and working directory are unchanged.",
                short
            ),
        };

        let dialog = adw::AlertDialog::new(
            Some(&format!("Reset {} to {}?", mode_str, short)),
            Some(&body),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("reset", &format!("Reset {}", mode_str));
        if mode == ResetMode::Hard {
            dialog.set_response_appearance("reset", adw::ResponseAppearance::Destructive);
        } else {
            dialog.set_response_appearance("reset", adw::ResponseAppearance::Suggested);
        }
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let win = self.clone();
        let sha = commit_id.to_string();
        dialog.connect_response(None, move |_, response| {
            if response == "reset" {
                let sha = sha.clone();
                win.run_git_op("Reset", move |path| {
                    let repo = GitRepo::open(path)?;
                    repo.reset(&sha, mode)
                });
            }
        });
        dialog.present(Some(self));
    }

    fn populate_graph_tab(&self) {
        let commits = self.imp().commits.borrow().clone();
        if commits.is_empty() {
            return;
        }

        // Find the graph page's ScrolledWindow
        let graph_page = self.imp().view_stack.child_by_name("graph");
        let Some(graph_page) = graph_page else { return };
        let Some(graph_box) = graph_page.downcast_ref::<gtk::Box>() else { return };
        let Some(scrolled) = graph_box.first_child().and_then(|c| c.downcast::<gtk::ScrolledWindow>().ok()) else { return };

        // Show spinner while computing
        let spinner = gtk::Spinner::builder()
            .spinning(true)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .vexpand(true)
            .build();
        scrolled.set_child(Some(&spinner));

        // Compute graph in background
        let (tx, rx) = async_channel::bounded::<Vec<super::commit_graph::GraphRow>>(1);
        std::thread::spawn(move || {
            let rows = super::commit_graph::compute_graph(&commits);
            let _ = tx.send_blocking(rows);
        });

        let scrolled_ref = scrolled.clone();
        glib::spawn_future_local(async move {
            if let Ok(rows) = rx.recv().await {
                let max_lanes = rows.iter().map(|r| r.num_active_lanes).max().unwrap_or(1).max(1);
                let graph_width = (max_lanes as f64 * 16.0 + 16.0).ceil() as i32;
                let total_height = (rows.len() as f64 * 32.0).ceil() as i32;

                let da = gtk::DrawingArea::builder()
                    .content_width(graph_width)
                    .content_height(total_height)
                    .build();

                let rows_for_draw = rows.clone();
                da.set_draw_func(move |_da, cr, _w, _h| {
                    for (i, row) in rows_for_draw.iter().enumerate() {
                        let y_offset = i as f64 * 32.0;
                        let _ = cr.save();
                        cr.translate(0.0, y_offset);
                        super::commit_graph::draw_graph_row_public(cr, row, 32.0, &rows_for_draw, i);
                        let _ = cr.restore();
                    }
                });

                // Labels column
                let labels_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
                for row in &rows {
                    let label = gtk::Label::builder()
                        .label(&format!("{} {}", row.short_id, row.summary))
                        .xalign(0.0)
                        .ellipsize(gtk::pango::EllipsizeMode::End)
                        .css_classes(["caption"])
                        .height_request(32)
                        .build();
                    labels_box.append(&label);
                }

                let content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
                content.append(&da);
                content.append(&labels_box);

                scrolled_ref.set_child(Some(&content));
            }
        });
    }

    fn show_edit_message_dialog(&self, original_message: &str) {
        let dialog = adw::Dialog::builder()
            .title("Edit Commit Message")
            .content_width(600)
            .content_height(400)
            .build();

        let toolbar_view = adw::ToolbarView::new();

        let header = adw::HeaderBar::new();
        let save_btn = gtk::Button::builder()
            .label("Save")
            .css_classes(["suggested-action"])
            .build();
        header.pack_end(&save_btn);
        toolbar_view.add_top_bar(&header);

        let text_view = gtk::TextView::builder()
            .wrap_mode(gtk::WrapMode::Word)
            .top_margin(12)
            .bottom_margin(12)
            .left_margin(12)
            .right_margin(12)
            .vexpand(true)
            .height_request(300)
            .build();
        text_view.add_css_class("card");
        text_view.buffer().set_text(original_message.trim());

        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(12)
            .build();
        scroll.set_child(Some(&text_view));
        toolbar_view.set_content(Some(&scroll));

        dialog.set_child(Some(&toolbar_view));

        let win = self.clone();
        let dlg = dialog.clone();
        save_btn.connect_clicked(move |_| {
            let buffer = text_view.buffer();
            let msg = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
            let msg = msg.trim().to_string();
            if msg.is_empty() {
                return;
            }
            dlg.close();
            win.run_git_op("Edit Message", move |path| {
                let repo = GitRepo::open(path)?;
                repo.amend_commit(Some(&msg))
            });
        });
        dialog.present(Some(self));
    }

    fn show_create_tag_dialog(&self, commit_id: &str) {
        let dialog = adw::AlertDialog::new(
            Some("Create Tag"),
            Some("Create a new tag on the selected commit:"),
        );

        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);

        let name_entry = gtk::Entry::builder()
            .placeholder_text("Tag name")
            .activates_default(true)
            .build();
        content.append(&name_entry);

        let annotated_check = gtk::CheckButton::builder()
            .label("Annotated tag")
            .build();
        content.append(&annotated_check);

        let msg_view = gtk::TextView::builder()
            .wrap_mode(gtk::WrapMode::Word)
            .top_margin(8)
            .bottom_margin(8)
            .left_margin(8)
            .right_margin(8)
            .height_request(72)
            .sensitive(false)
            .build();
        msg_view.add_css_class("card");

        let mv = msg_view.clone();
        annotated_check.connect_toggled(move |check| {
            mv.set_sensitive(check.is_active());
        });

        content.append(&msg_view);
        dialog.set_extra_child(Some(&content));

        dialog.add_response("cancel", "Cancel");
        dialog.add_response("create", "Create");
        dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("create"));
        dialog.set_close_response("cancel");

        let win = self.clone();
        let sha = commit_id.to_string();
        dialog.connect_response(None, move |_, response| {
            if response == "create" {
                let name = name_entry.text().trim().to_string();
                if name.is_empty() {
                    return;
                }
                let is_annotated = annotated_check.is_active();
                let sha = sha.clone();

                if is_annotated {
                    let buffer = msg_view.buffer();
                    let msg = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
                    let msg = msg.trim().to_string();
                    win.run_git_op("Create Tag", move |path| {
                        let repo = GitRepo::open(path)?;
                        let message = if msg.is_empty() { &name } else { &msg };
                        repo.create_annotated_tag(&name, &sha, message)
                    });
                } else {
                    win.run_git_op("Create Tag", move |path| {
                        let repo = GitRepo::open(path)?;
                        repo.create_tag(&name, &sha)
                    });
                }
            }
        });
        dialog.present(Some(self));
    }

}
