use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use gitpulsar_core::models::{CommitInfo, RepoStatus};
use gitpulsar_core::repository::GitRepo;
use gitpulsar_core::workspace::{self, WorkspaceEntry};

struct BackgroundRefreshResult {
    status: Option<RepoStatus>,
    status_hash: u64,
    workspace_entries: Option<Vec<WorkspaceEntry>>,
    workspace_hash: u64,
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
    status.ahead.hash(&mut hasher);
    status.behind.hash(&mut hasher);
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
use super::repo_tree;

mod imp {
    use super::*;

    pub struct GitpulsarWindow {
        pub repo: RefCell<Option<GitRepo>>,
        pub workspace_entries: RefCell<Vec<WorkspaceEntry>>,
        pub commits: RefCell<Vec<CommitInfo>>,
        pub selected_commit_id: RefCell<Option<String>>,
        /// Hash of last status to skip redundant UI updates.
        pub last_status_hash: std::cell::Cell<u64>,
        /// Hash of last workspace entries to skip redundant UI updates.
        pub last_workspace_hash: std::cell::Cell<u64>,
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
    }

    impl Default for GitpulsarWindow {
        fn default() -> Self {
            Self {
                repo: RefCell::new(None),
                workspace_entries: RefCell::new(Vec::new()),
                commits: RefCell::new(Vec::new()),
                selected_commit_id: RefCell::new(None),
                last_status_hash: std::cell::Cell::new(0),
                last_workspace_hash: std::cell::Cell::new(0),
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
        // HEADER BAR
        // ==========================================
        let header = adw::HeaderBar::new();

        // Left: open button
        let open_button = gtk::Button::builder()
            .icon_name("folder-open-symbolic")
            .tooltip_text("Open Workspace / Repository")
            .build();
        open_button.set_action_name(Some("win.open-repo"));
        header.pack_start(&open_button);

        // Left: stash button
        let stash_btn = gtk::Button::builder()
            .icon_name("document-save-symbolic")
            .tooltip_text("Stash (Ctrl+Z)")
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
        header.pack_start(&stash_btn);

        // Left: toggle left sidebar (repo tree)
        let toggle_repo_tree = gtk::ToggleButton::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text("Toggle Repository Tree")
            .active(true)
            .build();
        header.pack_start(&toggle_repo_tree);

        // Center: title "Gitpulsar"
        let title_label = gtk::Label::builder()
            .label("Gitpulsar")
            .css_classes(["title"])
            .build();
        header.set_title_widget(Some(&title_label));

        // Right: toggle right sidebar (branches/tags)
        let toggle_right_panel = gtk::ToggleButton::builder()
            .icon_name("sidebar-show-right-symbolic")
            .tooltip_text("Toggle Branches/Tags Panel")
            .active(true)
            .build();
        header.pack_end(&toggle_right_panel);

        // Right: indicators
        let indicators = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        imp.ahead_label.add_css_class("success");
        imp.ahead_label.add_css_class("caption");
        imp.behind_label.add_css_class("error");
        imp.behind_label.add_css_class("caption");
        indicators.append(&imp.ahead_label);
        indicators.append(&imp.behind_label);
        header.pack_end(&indicators);

        // Right: remote operation buttons
        header.pack_end(&imp.fetch_btn);
        header.pack_end(&imp.pull_btn);
        header.pack_end(&imp.push_btn);

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

        // Right: branch label (display only, no popover — branches are in right sidebar now)
        let branch_content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        imp.branch_label.set_label("—");
        branch_content.append(&gtk::Image::from_icon_name("network-workgroup-symbolic"));
        branch_content.append(&imp.branch_label);
        header.pack_end(&branch_content);

        // ==========================================
        // LEFT SIDEBAR — repo tree
        // ==========================================
        let repo_sidebar = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let repo_header = gtk::Label::builder()
            .label("Workspace")
            .css_classes(["heading"])
            .xalign(0.0)
            .margin_start(12)
            .margin_top(8)
            .margin_bottom(4)
            .build();
        repo_sidebar.append(&repo_header);

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
        repo_sidebar.append(&repo_scrolled);

        // ==========================================
        // CENTER — ViewStack (Commits / Changes)
        // ==========================================

        // --- Commits page ---
        let commits_page = gtk::Box::new(gtk::Orientation::Vertical, 0);

        commits_page.append(&imp.search_entry);

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
        // LAYOUT ASSEMBLY
        // ==========================================

        // Center: ViewStack + ViewSwitcherBar at the bottom
        let view_switcher_bar = adw::ViewSwitcherBar::new();
        view_switcher_bar.set_stack(Some(&imp.view_stack));
        view_switcher_bar.set_reveal(true);

        let center_toolbar = adw::ToolbarView::new();
        center_toolbar.set_content(Some(&imp.view_stack));
        center_toolbar.add_bottom_bar(&view_switcher_bar);
        center_toolbar.set_bottom_bar_style(adw::ToolbarStyle::Raised);

        // Inner split: content = center (ViewStack + switcher), sidebar = branches/tags (right)
        let inner_split = adw::OverlaySplitView::new();
        inner_split.set_sidebar_position(gtk::PackType::End);
        inner_split.set_sidebar(Some(&branches_panel));
        inner_split.set_content(Some(&center_toolbar));
        inner_split.set_collapsed(false);
        inner_split.set_show_sidebar(true);
        inner_split.set_min_sidebar_width(200.0);
        inner_split.set_max_sidebar_width(300.0);

        // Outer split: sidebar = repo tree (left), content = inner
        let outer_split = adw::OverlaySplitView::new();
        outer_split.set_sidebar_position(gtk::PackType::Start);
        outer_split.set_sidebar(Some(&repo_sidebar));
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

        // ToolbarView: header on top (full width), splits below
        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.set_top_bar_style(adw::ToolbarStyle::Raised);
        toolbar_view.set_content(Some(&outer_split));

        imp.toast_overlay.set_child(Some(&toolbar_view));
        self.set_content(Some(&imp.toast_overlay));

        // ==========================================
        // BREAKPOINTS
        // ==========================================

        // Medium (<1000px): collapse inner split (branches/tags overlay)
        let bp_medium = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            1000.0,
            adw::LengthUnit::Px,
        ));
        bp_medium.add_setter(&inner_split, "collapsed", Some(&true.to_value()));
        self.add_breakpoint(bp_medium);

        // Narrow (<700px): collapse both, force unified, hide extras
        let bp_narrow = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            700.0,
            adw::LengthUnit::Px,
        ));
        bp_narrow.add_setter(&outer_split, "collapsed", Some(&true.to_value()));
        bp_narrow.add_setter(&inner_split, "collapsed", Some(&true.to_value()));
        bp_narrow.add_setter(&indicators, "visible", Some(&false.to_value()));
        bp_narrow.add_setter(&stash_btn, "visible", Some(&false.to_value()));
        bp_narrow.add_setter(&title_label, "visible", Some(&false.to_value()));
        bp_narrow.add_setter(&imp.fetch_btn, "visible", Some(&false.to_value()));
        bp_narrow.add_setter(&imp.pull_btn, "visible", Some(&false.to_value()));
        bp_narrow.add_setter(&imp.push_btn, "visible", Some(&false.to_value()));
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
    }

    /// Open a workspace folder (or single repo).
    pub fn open_workspace(&self, path: &std::path::Path) {
        match workspace::scan_workspace(path) {
            Ok(entries) => {
                tracing::info!("Opened workspace: {} ({} entries)", path.display(), entries.len());
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

    /// Load data for the currently selected repo.
    fn load_repo_data(&self, repo: &GitRepo) {
        let imp = self.imp();

        // Update branch & indicators
        if let Some(branch) = repo.current_branch_name() {
            imp.branch_label.set_label(&branch);
        }
        let (ahead, behind) = repo.ahead_behind().unwrap_or((0, 0));
        imp.ahead_label.set_label(&format!("▲ {}", ahead));
        imp.behind_label.set_label(&format!("▼ {}", behind));

        // Load commits
        let commits = repo.log(200).unwrap_or_default();
        self.populate_commit_list(&commits, ahead);
        *imp.commits.borrow_mut() = commits;

        // Load status for changes view
        if let Ok(status) = repo.status() {
            self.refresh_changes_list(&status);
        }

        // Reset search
        *imp.selected_commit_id.borrow_mut() = None;
        imp.search_entry.set_text("");
        imp.commit_list_box.set_filter_func(|_| true);

        // Populate branches & tags in right sidebar
        self.populate_branches_tags(repo);
    }

    fn populate_commit_list(&self, commits: &[CommitInfo], ahead: usize) {
        let list_box = &self.imp().commit_list_box;

        while let Some(child) = list_box.first_child() {
            list_box.remove(&child);
        }

        // Load tags map
        let tags_map = {
            let repo_ref = self.imp().repo.borrow();
            repo_ref
                .as_ref()
                .and_then(|r| r.tags_by_commit().ok())
                .unwrap_or_default()
        };

        for (idx, commit_info) in commits.iter().enumerate() {
            let tags = tags_map
                .get(&commit_info.id)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let is_unpushed = idx < ahead;
            let is_head = idx == 0;
            let row = commit_list::create_commit_row(commit_info, tags, is_unpushed, is_head);
            list_box.append(&row);
        }
    }

    fn on_commit_selected(&self, index: usize) {
        let imp = self.imp();
        let commits = imp.commits.borrow();

        if let Some(commit) = commits.get(index) {
            let commit_id = commit.id.clone();
            let full_message = commit.message.clone();
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

                    // Connect edit-message button if HEAD
                    if index == 0 {
                        if let Some(btn) = commit_list::find_edit_message_btn(&row) {
                            let win = self.clone();
                            let msg = full_message.clone();
                            btn.connect_clicked(move |_| {
                                win.show_edit_message_dialog(&msg);
                            });
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
                    self.scan_indicators();
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

    /// Reload the changes file list (after stage/unstage/discard).
    fn refresh_staging(&self) {
        let repo_ref = self.imp().repo.borrow();
        if let Some(ref repo) = *repo_ref {
            if let Ok(status) = repo.status() {
                self.refresh_changes_list(&status);
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

    /// Get the diff for a single file, checking unstaged, staged, and untracked.
    fn get_file_diff(&self, repo: &GitRepo, path: &str) -> Option<gitpulsar_core::models::DiffFile> {
        // Try unstaged
        if let Ok(files) = repo.diff_unstaged() {
            if let Some(f) = files.into_iter().find(|f| f.path == path) {
                return Some(f);
            }
        }

        // Try staged
        if let Ok(files) = repo.diff_staged() {
            if let Some(f) = files.into_iter().find(|f| f.path == path) {
                return Some(f);
            }
        }

        // Try untracked
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

    /// Populate branches and tags in the right sidebar panel.
    fn populate_branches_tags(&self, repo: &GitRepo) {
        let imp = self.imp();

        let local = imp.branches_local_list.borrow().clone();
        let remote = imp.branches_remote_list.borrow().clone();
        let tags = imp.tags_list.borrow().clone();

        if let (Some(ref local), Some(ref remote)) = (local, remote) {
            if let Ok(branches) = repo.branches() {
                branches_tags_panel::populate_branches(local, remote, &branches);
            }
        }

        if let Some(ref tl) = tags {
            if let Ok(tag_list) = repo.tags() {
                branches_tags_panel::populate_tags(tl, &tag_list);
            }
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
        dialog.add_response("ok", "OK");
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

        let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
        std::thread::spawn(move || {
            let result = op(&path).map_err(|e| e.to_string());
            tx.send(result).ok();
        });

        let win = self.clone();
        let title = op_name.to_string();
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            match rx.try_recv() {
                Ok(result) => {
                    win.set_remote_buttons_sensitive(true);
                    match result {
                        Ok(msg) => {
                            win.show_toast(&msg);
                            win.refresh_after_remote_op();
                        }
                        Err(e) => {
                            win.show_error_dialog(&format!("{title} Failed"), &e);
                        }
                    }
                    glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(_) => {
                    win.set_remote_buttons_sensitive(true);
                    glib::ControlFlow::Break
                }
            }
        });
    }

    // ==========================================
    // REMOTE OPERATIONS (async)
    // ==========================================

    fn on_fetch(&self) {
        self.run_git_op("Fetch", |path| {
            let repo = GitRepo::open(path)?;
            repo.fetch()?;
            Ok("Fetch complete".to_string())
        });
    }

    fn on_pull(&self) {
        self.run_git_op("Pull", |path| {
            let repo = GitRepo::open(path)?;
            repo.pull()
        });
    }

    fn on_push(&self, force: bool) {
        self.run_git_op("Push", move |path| {
            let repo = GitRepo::open(path)?;
            repo.push(force)?;
            let msg = if force { "Force push complete" } else { "Push complete" };
            Ok(msg.to_string())
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
        self.scan_indicators();
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
        let win = self.downgrade();
        glib::timeout_add_seconds_local(10, move || {
            let Some(win) = win.upgrade() else {
                return glib::ControlFlow::Break;
            };
            win.trigger_background_refresh();
            glib::ControlFlow::Continue
        });
    }

    /// Run status + workspace scan in a background thread, then apply results on UI thread.
    fn trigger_background_refresh(&self) {
        let imp = self.imp();

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

        let (tx, rx) = std::sync::mpsc::channel::<BackgroundRefreshResult>();

        std::thread::spawn(move || {
            let status = repo_path.and_then(|p| {
                GitRepo::open(&p).ok().and_then(|r| r.status().ok())
            });
            let status_hash = status.as_ref().map(|s| hash_status(s)).unwrap_or(0);

            let workspace_entries = workspace_root.and_then(|root| {
                workspace::scan_workspace(&root).ok()
            });
            let workspace_hash = workspace_entries.as_ref().map(|e| hash_workspace(e)).unwrap_or(0);

            tx.send(BackgroundRefreshResult { status, status_hash, workspace_entries, workspace_hash }).ok();
        });

        let win = self.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            match rx.try_recv() {
                Ok(result) => {
                    let imp = win.imp();

                    // Apply status only if changed
                    if let Some(status) = result.status {
                        if result.status_hash != imp.last_status_hash.get() {
                            imp.last_status_hash.set(result.status_hash);
                            win.refresh_changes_list(&status);
                        }
                        imp.ahead_label.set_label(&format!("▲ {}", status.ahead));
                        imp.behind_label.set_label(&format!("▼ {}", status.behind));
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

                    glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(_) => glib::ControlFlow::Break,
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

    fn show_edit_message_dialog(&self, original_message: &str) {
        let dialog = adw::MessageDialog::new(
            Some(self),
            Some("Edit Commit Message"),
            Some("Edit the HEAD commit message:"),
        );

        let text_view = gtk::TextView::builder()
            .wrap_mode(gtk::WrapMode::Word)
            .top_margin(8)
            .bottom_margin(8)
            .left_margin(8)
            .right_margin(8)
            .height_request(120)
            .build();
        text_view.add_css_class("card");
        text_view.buffer().set_text(original_message.trim());
        dialog.set_extra_child(Some(&text_view));

        dialog.add_response("cancel", "Cancel");
        dialog.add_response("save", "Save");
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");

        let win = self.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "save" {
                let buffer = text_view.buffer();
                let msg = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
                let msg = msg.trim().to_string();
                if msg.is_empty() {
                    return;
                }
                win.run_git_op("Edit Message", move |path| {
                    let repo = GitRepo::open(path)?;
                    repo.amend_commit(Some(&msg))
                });
            }
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

    /// Refresh indicators — delegates to background refresh to avoid blocking UI.
    fn scan_indicators(&self) {
        self.trigger_background_refresh();
    }
}
