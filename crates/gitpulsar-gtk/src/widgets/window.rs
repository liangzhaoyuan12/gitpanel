use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use gitpulsar_core::models::{CommitInfo, DiffFile, RepoStatus};
use gitpulsar_core::repository::GitRepo;
use gitpulsar_core::workspace::{self, WorkspaceEntry};

use crate::config::AppConfig;

struct BackgroundRepoData {
    commits: Vec<CommitInfo>,
    tags_map: HashMap<String, Vec<String>>,
    status: Option<RepoStatus>,
    branches: Vec<gitpulsar_core::models::BranchInfo>,
    tags: Vec<gitpulsar_core::models::TagInfo>,
    ahead: usize,
    behind: usize,
    branch_name: Option<String>,
    unstaged_diffs: Vec<DiffFile>,
    staged_diffs: Vec<DiffFile>,
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
            ind.ahead.hash(&mut hasher);
        }
    }
    hasher.finish()
}

use super::branches_tags_panel;
use super::changes_view;
use super::commit_list;
use super::preferences_dialog;
use super::repo_tree;

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
        // Changes view file list (set during setup_ui)
        pub changes_file_list: RefCell<Option<gtk::ListBox>>,
        // Sidebar header title (folder name)
        pub sidebar_title_label: gtk::Label,
        // Sidebar status
        pub sidebar_repo_name_label: gtk::Label,
        pub sidebar_status_label: gtk::Label,
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
                changes_file_list: RefCell::new(None),
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
            .property("width-request", 390)
            .property("height-request", 400)
            .build();

        window.setup_ui();
        window.setup_actions();
        window.setup_commit_context_menu();
        window.setup_auto_refresh();
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
        let menu_model = gio::Menu::new();
        menu_model.append(Some("Preferences"), Some("win.preferences"));
        menu_model.append(Some("About Gitpulsar"), Some("win.about"));

        let menu_btn = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&menu_model)
            .tooltip_text("Menu")
            .build();
        sidebar_header.pack_end(&menu_btn);

        // ==========================================
        // CONTENT HEADER BAR
        // ==========================================
        let content_header = adw::HeaderBar::new();
        content_header.set_show_start_title_buttons(false);
        content_header.set_show_end_title_buttons(true);
        // Empty title widget to prevent window title "Gitpulsar" from leaking
        content_header.set_title_widget(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));

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
        let graph_btn = gtk::Button::builder()
            .icon_name("org.gnome.Settings-network-symbolic")
            .tooltip_text("Branch Graph")
            .build();
        let win = self.clone();
        graph_btn.connect_clicked(move |_| {
            win.show_branch_graph();
        });
        content_header.pack_end(&graph_btn);

        // Content header right: branch label
        let branch_content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        imp.branch_label.set_label("—");
        branch_content.append(&gtk::Image::from_icon_name("view-list-symbolic"));
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

        let repo_header = gtk::Label::builder()
            .label("Workspace")
            .css_classes(["heading"])
            .xalign(0.0)
            .margin_start(12)
            .margin_top(8)
            .margin_bottom(4)
            .build();
        repo_sidebar_content.append(&repo_header);

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

        imp.commit_list_box.set_selection_mode(gtk::SelectionMode::None);
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
            win.on_commit_selected(row.index() as usize);
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

        // Connect file row activation — toggle diff accordion
        let changes_fl = changes_refs.file_list_box.clone();
        let win = self.clone();
        changes_fl.connect_row_activated(move |_, row| {
            win.on_changes_file_activated(row);
        });

        // Connect per-row stage/unstage/discard buttons
        self.setup_changes_row_button_signals(&changes_refs.file_list_box);

        // Store changes file list ref
        *imp.changes_file_list.borrow_mut() = Some(changes_refs.file_list_box.clone());

        // --- ViewStack setup ---
        imp.view_stack.add_titled_with_icon(&commits_page, Some("commits"), "Commits", "emoji-recent-symbolic");
        imp.view_stack.add_titled_with_icon(&changes_box, Some("changes"), "Changes", "document-edit-symbolic");

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
            .icon_name("emoji-recent-symbolic")
            .tooltip_text("Commits")
            .active(true)
            .css_classes(["flat"])
            .build();
        let changes_toggle = gtk::ToggleButton::builder()
            .icon_name("document-edit-symbolic")
            .tooltip_text("Changes")
            .css_classes(["flat"])
            .build();
        changes_toggle.set_group(Some(&commits_toggle));
        compact_switcher.append(&commits_toggle);
        compact_switcher.append(&changes_toggle);

        // Sync compact toggles → view_stack
        let vs = imp.view_stack.clone();
        commits_toggle.connect_toggled(move |btn| {
            if btn.is_active() { vs.set_visible_child_name("commits"); }
        });
        let vs = imp.view_stack.clone();
        changes_toggle.connect_toggled(move |btn| {
            if btn.is_active() { vs.set_visible_child_name("changes"); }
        });

        // Sync view_stack → compact toggles
        let ct = commits_toggle.clone();
        let cht = changes_toggle.clone();
        imp.view_stack.connect_visible_child_name_notify(move |stack| {
            if let Some(name) = stack.visible_child_name() {
                match name.as_str() {
                    "commits" => { if !ct.is_active() { ct.set_active(true); } }
                    "changes" => { if !cht.is_active() { cht.set_active(true); } }
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

        // Center: content_header + ToastOverlay(ViewStack) + bottom bar
        imp.toast_overlay.set_child(Some(&imp.view_stack));
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
        inner_split.set_min_sidebar_width(180.0);
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
        outer_split.set_min_sidebar_width(180.0);
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

        self.set_content(Some(&outer_split));

        // ==========================================
        // BREAKPOINTS
        // ==========================================

        // Medium (<1000px): collapse inner split, move window buttons to content header
        let bp_medium = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            1000.0,
            adw::LengthUnit::Px,
        ));
        bp_medium.add_setter(&inner_split, "collapsed", Some(&true.to_value()));
        bp_medium.add_setter(&content_header, "show-end-title-buttons", Some(&true.to_value()));
        self.add_breakpoint(bp_medium);

        // Narrow (<700px): collapse both, strip labels, compact footer
        let bp_narrow = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            700.0,
            adw::LengthUnit::Px,
        ));
        bp_narrow.add_setter(&outer_split, "collapsed", Some(&true.to_value()));
        bp_narrow.add_setter(&inner_split, "collapsed", Some(&true.to_value()));
        // Window buttons on content header (only visible header now)
        bp_narrow.add_setter(&content_header, "show-end-title-buttons", Some(&true.to_value()));
        // Hide text labels
        bp_narrow.add_setter(&imp.sidebar_title_label, "visible", Some(&false.to_value()));
        bp_narrow.add_setter(&branch_content, "visible", Some(&false.to_value()));
        bp_narrow.add_setter(&graph_btn, "visible", Some(&false.to_value()));
        bp_narrow.add_setter(&open_button, "visible", Some(&false.to_value()));
        // Footer: icon-only switcher
        bp_narrow.add_setter(&view_switcher, "visible", Some(&false.to_value()));
        bp_narrow.add_setter(&compact_switcher, "visible", Some(&true.to_value()));
        self.add_breakpoint(bp_narrow);
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
            let dialog = adw::AboutWindow::builder()
                .application_name("Gitpulsar")
                .application_icon("dev.gitpulsar.Gitpulsar")
                .developer_name("Gitpulsar")
                .version(env!("CARGO_PKG_VERSION"))
                .website("https://gitlab.com/ilshat.ishdavletov/gitpulsar")
                .license_type(gtk::License::Gpl30)
                .transient_for(&window)
                .modal(true)
                .build();
            dialog.present();
        });
        self.add_action(&about_action);
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
                let dialog = adw::MessageDialog::new(
                    Some(self),
                    Some("Error"),
                    Some(&format!("Failed to open workspace:\n{}", e)),
                );
                dialog.add_response("ok", "OK");
                dialog.present();
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
                let dialog = adw::MessageDialog::new(
                    Some(self),
                    Some("Error"),
                    Some(&format!("Failed to open repository:\n{}", e)),
                );
                dialog.add_response("ok", "OK");
                dialog.present();
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
            let Ok(repo) = GitRepo::open(&path) else { return };
            let commits = repo.log(200).unwrap_or_default();
            let tags_map = repo.tags_by_commit().unwrap_or_default();
            let status = repo.status().ok();
            let branches = repo.branches().unwrap_or_default();
            let tags = repo.tags().unwrap_or_default();
            let (ahead, behind) = repo.ahead_behind().unwrap_or((0, 0));
            let branch_name = repo.current_branch_name();
            let unstaged_diffs = repo.diff_unstaged().unwrap_or_default();
            let staged_diffs = repo.diff_staged().unwrap_or_default();
            tx.send_blocking(BackgroundRepoData {
                commits,
                tags_map,
                status,
                branches,
                tags,
                ahead,
                behind,
                branch_name,
                unstaged_diffs,
                staged_diffs,
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

                // Load commits
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

                // Update sidebar status
                if let Some(ref path_str) = win.repo_path_string() {
                    let name = std::path::Path::new(path_str)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    win.update_sidebar_status(&name, data.status.as_ref());
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

        for (idx, commit_info) in commits.iter().enumerate() {
            let tags = tags_map
                .get(&commit_info.id)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let is_unpushed = idx < ahead;
            let is_head = idx == 0;
            let date_format = imp.config.borrow().date_format;
            let row = commit_list::create_commit_row(commit_info, tags, is_unpushed, is_head, date_format);

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
                    // Load file list for this commit
                    let repo_ref = imp.repo.borrow();
                    if let Some(ref repo) = *repo_ref {
                        if let Ok(files) = repo.diff_commit(&commit_id) {
                            if let Some(files_box) = commit_list::get_files_box(&row) {
                                commit_list::populate_commit_files(&files_box, &files);
                            }
                        }
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
                    let dialog = adw::MessageDialog::new(
                        Some(self),
                        Some(title),
                        Some(&format!("{}", e)),
                    );
                    dialog.add_response("ok", "OK");
                    dialog.present();
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
            self.refresh_staging();
        }
    }

    fn discard_file_with_confirm(&self, path: &str) {
        let dialog = adw::MessageDialog::new(
            Some(self),
            Some("Discard Changes?"),
            Some(&format!(
                "This will permanently discard all changes to:\n\n<b>{}</b>\n\nThis cannot be undone.",
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
                    if let Err(e) = repo.discard_file(&file_path) {
                        tracing::error!("Failed to discard {}: {}", file_path, e);
                    }
                    drop(repo_ref);
                    win.refresh_staging();
                }
            }
        });
        dialog.present();
    }

    /// Update the sidebar status bar with repo name and git status summary.
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
            if let Ok(status) = repo.status() {
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
        let list = self.imp().changes_file_list.borrow().clone();
        if let Some(ref list_box) = list {
            let files = changes_view::collect_changed_files(status);
            changes_view::populate_file_list(list_box, &files);
        }
    }

    /// Handle click on a file in the changes accordion — toggle inline diff.
    fn on_changes_file_activated(&self, row: &gtk::ListBoxRow) {
        let expanded = changes_view::toggle_file_diff(row);

        if expanded {
            let file_path = row.widget_name().to_string();
            if file_path.is_empty() {
                return;
            }

            let repo_ref = self.imp().repo.borrow();
            let Some(ref repo) = *repo_ref else { return };

            // Try to get diff for this file — try unstaged first, then staged, then untracked
            let diff_file = self.get_file_diff(repo, &file_path);

            if let Some(file) = diff_file {
                if let Some(tv) = changes_view::get_diff_textview(row) {
                    changes_view::render_file_diff(&tv, &file);
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

        if let (Some(ref local), Some(ref remote)) = (local, remote) {
            branches_tags_panel::populate_branches(local, remote, branches);
        }

        if let Some(ref tl) = tags_list {
            branches_tags_panel::populate_tags(tl, tags);
        }
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
        let dialog = adw::MessageDialog::new(Some(self), Some(title), Some(body));
        dialog.set_body_use_markup(false);
        dialog.add_response("ok", "OK");
        dialog.present();
    }

    fn show_push_rejected_dialog(&self, _body: &str) {
        let dialog = adw::MessageDialog::new(
            Some(self),
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
        dialog.present();
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
            let mut args = vec!["push"];
            if force {
                args.push("--force");
            }
            run_git_cmd(path, &args)
                .map(|_| if force { "Force push complete".to_string() } else { "Push complete".to_string() })
        });
    }

    fn show_force_push_dialog(&self) {
        let dialog = adw::MessageDialog::new(
            Some(self),
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
        dialog.present();
    }

    fn refresh_after_remote_op(&self) {
        // Re-open repo since the background thread may have changed state
        let path = self.repo_path_string();
        if let Some(path) = path {
            if let Ok(repo) = GitRepo::open(&path) {
                self.load_repo_data(&repo);
                *self.imp().repo.borrow_mut() = Some(repo);
            }
        }
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
        let dialog = adw::MessageDialog::new(
            Some(self),
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
        dialog.present();
    }

    // ==========================================
    // AUTO-REFRESH
    // ==========================================

    fn setup_auto_refresh(&self) {
        let interval = self.imp().config.borrow().refresh_interval_secs;
        self.start_refresh_timer(interval);
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
            win.trigger_background_refresh();
            glib::ControlFlow::Continue
        });
        *self.imp().refresh_source_id.borrow_mut() = Some(source_id);
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
                    win.populate_commit_list(&commits, ahead, &tags_map);
                }
            }

            if interval_changed {
                win.start_refresh_timer(new_config.refresh_interval_secs);
            }
        });
        dialog.set_transient_for(Some(self));
        dialog.set_modal(true);
        dialog.present();
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

        let (tx, rx) = async_channel::bounded::<BackgroundRefreshResult>(1);

        std::thread::spawn(move || {
            let (status, ahead, behind, unstaged_diffs, staged_diffs) =
                if let Some(ref p) = repo_path {
                    if let Ok(repo) = GitRepo::open(p) {
                        let st = repo.status().ok();
                        let (a, b) = repo.ahead_behind().unwrap_or((0, 0));
                        let ud = repo.diff_unstaged().unwrap_or_default();
                        let sd = repo.diff_staged().unwrap_or_default();
                        (st, a, b, ud, sd)
                    } else {
                        (None, 0, 0, Vec::new(), Vec::new())
                    }
                } else {
                    (None, 0, 0, Vec::new(), Vec::new())
                };
            let status_hash = status.as_ref().map(|s| hash_status(s)).unwrap_or(0);

            let workspace_entries = workspace_root.and_then(|root| {
                workspace::scan_workspace(&root).ok()
            });
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
                    // Update sidebar status
                    if let Some(ref path_str) = win.repo_path_string() {
                        let name = std::path::Path::new(path_str)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        win.update_sidebar_status(&name, Some(&status));
                    }
                }
            }

            imp.ahead_label.set_label(&format!("▲ {}", result.ahead));
            imp.behind_label.set_label(&format!("▼ {}", result.behind));

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

    fn show_revert_confirm_dialog(&self, commit_id: &str) {
        let dialog = adw::MessageDialog::new(
            Some(self),
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
        dialog.present();
    }

    fn show_branch_graph(&self) {
        let commits = self.imp().commits.borrow();
        if commits.is_empty() {
            return;
        }
        super::commit_graph::show_graph_window(self.upcast_ref::<gtk::Window>(), &commits);
    }

    fn show_edit_message_dialog(&self, original_message: &str) {
        let dialog = adw::Window::builder()
            .title("Edit Commit Message")
            .default_width(600)
            .default_height(400)
            .modal(true)
            .transient_for(self)
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

        dialog.set_content(Some(&toolbar_view));

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
        dialog.present();
    }

    fn show_create_tag_dialog(&self, commit_id: &str) {
        let dialog = adw::MessageDialog::new(
            Some(self),
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
        dialog.present();
    }

}
