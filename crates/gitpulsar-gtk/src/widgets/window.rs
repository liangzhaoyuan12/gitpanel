use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;

use gitpulsar_core::models::CommitInfo;
use gitpulsar_core::repository::GitRepo;
use gitpulsar_core::workspace::{self, WorkspaceEntry};

use super::commit_list;
use super::diff_view;
use super::repo_tree;
use super::staging_area;

mod imp {
    use super::*;

    pub struct GitpulsarWindow {
        pub repo: RefCell<Option<GitRepo>>,
        pub workspace_entries: RefCell<Vec<WorkspaceEntry>>,
        pub commits: RefCell<Vec<CommitInfo>>,
        pub selected_commit_id: RefCell<Option<String>>,
        pub side_by_side: std::cell::Cell<bool>,
        // Layout refs
        pub outer_split: RefCell<Option<adw::OverlaySplitView>>,
        pub inner_split: RefCell<Option<adw::OverlaySplitView>>,
        pub toast_overlay: adw::ToastOverlay,
        // Widget refs
        pub repo_list_box: gtk::ListBox,
        pub commit_list_box: gtk::ListBox,
        pub view_stack: adw::ViewStack,
        pub diff_stack: gtk::Stack,
        pub diff_unified_view: gtk::TextView,
        pub diff_left_view: gtk::TextView,
        pub diff_right_view: gtk::TextView,
        pub unstaged_list: gtk::ListBox,
        pub staged_list: gtk::ListBox,
        pub branch_label: gtk::Label,
        pub ahead_label: gtk::Label,
        pub behind_label: gtk::Label,
        pub commit_entry: gtk::TextView,
        pub commit_button: gtk::Button,
        pub search_entry: gtk::SearchEntry,
        pub fetch_btn: gtk::Button,
        pub pull_btn: gtk::Button,
        pub push_btn: gtk::Button,
    }

    impl Default for GitpulsarWindow {
        fn default() -> Self {
            Self {
                repo: RefCell::new(None),
                workspace_entries: RefCell::new(Vec::new()),
                commits: RefCell::new(Vec::new()),
                selected_commit_id: RefCell::new(None),
                side_by_side: std::cell::Cell::new(true),
                outer_split: RefCell::new(None),
                inner_split: RefCell::new(None),
                toast_overlay: adw::ToastOverlay::new(),
                repo_list_box: gtk::ListBox::new(),
                commit_list_box: gtk::ListBox::new(),
                view_stack: adw::ViewStack::new(),
                diff_stack: gtk::Stack::new(),
                diff_unified_view: gtk::TextView::new(),
                diff_left_view: gtk::TextView::new(),
                diff_right_view: gtk::TextView::new(),
                unstaged_list: gtk::ListBox::new(),
                staged_list: gtk::ListBox::new(),
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

        // Left: view toggle (sbs / unified)
        let view_toggle_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        view_toggle_box.add_css_class("linked");
        let split_btn = gtk::ToggleButton::builder()
            .icon_name("view-dual-symbolic")
            .tooltip_text("Side-by-side diff")
            .active(true)
            .build();
        let unified_btn = gtk::ToggleButton::builder()
            .icon_name("view-continuous-symbolic")
            .tooltip_text("Unified diff")
            .group(&split_btn)
            .build();
        view_toggle_box.append(&split_btn);
        view_toggle_box.append(&unified_btn);
        header.pack_start(&view_toggle_box);

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

        // Right: toggle right sidebar (commits/changes)
        let toggle_right_panel = gtk::ToggleButton::builder()
            .icon_name("sidebar-show-right-symbolic")
            .tooltip_text("Toggle Commits/Changes Panel")
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

        // Right: branch selector (MenuButton + Popover)
        let branch_content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        imp.branch_label.set_label("—");
        branch_content.append(&imp.branch_label);
        branch_content.append(&gtk::Image::from_icon_name("pan-down-symbolic"));
        let branch_menu_btn = gtk::MenuButton::builder()
            .css_classes(["flat"])
            .build();
        branch_menu_btn.set_property("child", &branch_content);

        // Branch popover contents
        let branch_popover = gtk::Popover::new();
        let branch_popover_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        branch_popover_box.set_margin_top(8);
        branch_popover_box.set_margin_bottom(8);
        branch_popover_box.set_margin_start(8);
        branch_popover_box.set_margin_end(8);
        branch_popover_box.set_width_request(250);

        let branch_search = gtk::SearchEntry::builder()
            .placeholder_text("Filter branches…")
            .build();
        branch_popover_box.append(&branch_search);

        let branch_local_label = gtk::Label::builder()
            .label("Local")
            .css_classes(["heading"])
            .xalign(0.0)
            .margin_top(4)
            .build();
        branch_popover_box.append(&branch_local_label);

        let branch_local_list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        let branch_local_scrolled = gtk::ScrolledWindow::builder()
            .max_content_height(200)
            .propagate_natural_height(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        branch_local_scrolled.set_child(Some(&branch_local_list));
        branch_popover_box.append(&branch_local_scrolled);

        let branch_remote_label = gtk::Label::builder()
            .label("Remote")
            .css_classes(["heading"])
            .xalign(0.0)
            .margin_top(4)
            .build();
        branch_popover_box.append(&branch_remote_label);

        let branch_remote_list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        let branch_remote_scrolled = gtk::ScrolledWindow::builder()
            .max_content_height(200)
            .propagate_natural_height(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        branch_remote_scrolled.set_child(Some(&branch_remote_list));
        branch_popover_box.append(&branch_remote_scrolled);

        let create_branch_btn = gtk::Button::builder()
            .label("Create New Branch…")
            .css_classes(["suggested-action"])
            .margin_top(4)
            .build();
        branch_popover_box.append(&create_branch_btn);

        branch_popover.set_child(Some(&branch_popover_box));
        branch_menu_btn.set_popover(Some(&branch_popover));

        // Populate branch lists when popover opens
        let win = self.clone();
        let local_list = branch_local_list.clone();
        let remote_list = branch_remote_list.clone();
        branch_popover.connect_show(move |_| {
            win.populate_branch_lists(&local_list, &remote_list);
        });

        // Branch search filter
        let ll = branch_local_list.clone();
        let rl = branch_remote_list.clone();
        branch_search.connect_search_changed(move |entry| {
            let query = entry.text().to_lowercase();
            let query_ref = query.clone();
            ll.set_filter_func(move |row| {
                if query_ref.is_empty() {
                    return true;
                }
                row.widget_name().to_lowercase().contains(&query_ref)
            });
            let query_ref = query;
            rl.set_filter_func(move |row| {
                if query_ref.is_empty() {
                    return true;
                }
                row.widget_name().to_lowercase().contains(&query_ref)
            });
        });

        // Click on local branch → checkout
        let win = self.clone();
        let bp2 = branch_popover.clone();
        branch_local_list.connect_row_activated(move |_, row| {
            let name = row.widget_name().to_string();
            bp2.popdown();
            win.on_checkout_branch(&name);
        });

        // Click on remote branch → checkout remote
        let win = self.clone();
        let bp3 = branch_popover.clone();
        branch_remote_list.connect_row_activated(move |_, row| {
            let name = row.widget_name().to_string();
            bp3.popdown();
            win.on_checkout_remote_branch(&name);
        });

        // Create new branch
        let win = self.clone();
        let bp4 = branch_popover.clone();
        create_branch_btn.connect_clicked(move |_| {
            bp4.popdown();
            win.show_create_branch_dialog();
        });

        header.pack_end(&branch_menu_btn);

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
        // MIDDLE PANEL — ViewStack (Commits / Changes)
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

        imp.commit_list_box.set_selection_mode(gtk::SelectionMode::Single);
        imp.commit_list_box.add_css_class("navigation-sidebar");

        let commits_placeholder = gtk::Label::builder()
            .label("Select a repository")
            .css_classes(["dim-label"])
            .margin_top(24)
            .margin_bottom(24)
            .build();
        imp.commit_list_box.set_placeholder(Some(&commits_placeholder));

        // Connect commit selection
        let win = self.clone();
        imp.commit_list_box.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                win.on_commit_selected(row.index() as usize);
            }
        });

        commit_scrolled.set_child(Some(&imp.commit_list_box));
        commits_page.append(&commit_scrolled);

        // --- Changes page ---
        let (staging_box, staging_buttons) = staging_area::build_staging_panel(
            &imp.unstaged_list,
            &imp.staged_list,
            &imp.commit_entry,
            &imp.commit_button,
        );

        // Connect commit button
        let win = self.clone();
        imp.commit_button.connect_clicked(move |_| {
            win.on_commit_clicked();
        });

        // Connect Stage All button
        let win = self.clone();
        staging_buttons.stage_all_btn.connect_clicked(move |_| {
            win.on_stage_all();
        });

        // Connect Unstage All button
        let win = self.clone();
        staging_buttons.unstage_all_btn.connect_clicked(move |_| {
            win.on_unstage_all();
        });

        // Connect file selection in unstaged list → show diff
        let win = self.clone();
        imp.unstaged_list.connect_row_selected(move |_, row| {
            if row.is_some() {
                win.on_unstaged_file_selected();
            }
        });

        // Connect file selection in staged list → show diff
        let win = self.clone();
        imp.staged_list.connect_row_selected(move |_, row| {
            if row.is_some() {
                win.on_staged_file_selected();
            }
        });

        // Connect per-row stage/unstage/discard buttons via click on list
        self.setup_row_button_signals();

        // --- ViewStack setup ---
        imp.view_stack.add_titled_with_icon(&commits_page, Some("commits"), "Commits", "emoji-recent-symbolic");
        imp.view_stack.add_titled_with_icon(&staging_box, Some("changes"), "Changes", "document-edit-symbolic");

        let middle_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        middle_box.append(&imp.view_stack);
        middle_box.set_width_request(250);

        // ==========================================
        // RIGHT PANEL — diff view
        // ==========================================
        let diff_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        diff_box.set_vexpand(true);
        diff_box.set_hexpand(true);


        // Placeholder
        let placeholder_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
        placeholder_box.set_valign(gtk::Align::Center);
        placeholder_box.set_halign(gtk::Align::Center);
        placeholder_box.set_vexpand(true);
        placeholder_box.append(
            &gtk::Image::builder()
                .icon_name("document-open-symbolic")
                .pixel_size(64)
                .css_classes(["dim-label"])
                .build(),
        );
        placeholder_box.append(
            &gtk::Label::builder()
                .label("Select a commit or file to view diff")
                .css_classes(["dim-label", "title-3"])
                .build(),
        );

        // Configure text views
        for tv in [&imp.diff_unified_view, &imp.diff_left_view, &imp.diff_right_view] {
            tv.set_editable(false);
            tv.set_monospace(true);
            tv.set_left_margin(4);
            tv.set_right_margin(4);
            tv.set_top_margin(4);
            tv.set_bottom_margin(4);
            tv.set_cursor_visible(false);
            tv.set_wrap_mode(gtk::WrapMode::None);
        }

        // Unified view
        let unified_scrolled = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .build();
        unified_scrolled.set_child(Some(&imp.diff_unified_view));

        // Side-by-side view
        let sbs_box = gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            .vexpand(true)
            .hexpand(true)
            .resize_start_child(true)
            .resize_end_child(true)
            .build();

        let left_scrolled = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .build();
        left_scrolled.set_child(Some(&imp.diff_left_view));

        let right_scrolled = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .build();
        right_scrolled.set_child(Some(&imp.diff_right_view));

        diff_view::sync_scroll(&left_scrolled, &right_scrolled);

        sbs_box.set_start_child(Some(&left_scrolled));
        sbs_box.set_end_child(Some(&right_scrolled));

        // Stack: placeholder / side-by-side / unified
        imp.diff_stack.add_named(&placeholder_box, Some("placeholder"));
        imp.diff_stack.add_named(&sbs_box, Some("side-by-side"));
        imp.diff_stack.add_named(&unified_scrolled, Some("unified"));
        imp.diff_stack.set_visible_child_name("placeholder");

        // View toggle buttons
        let win1 = self.clone();
        split_btn.connect_toggled(move |btn| {
            if btn.is_active() {
                win1.imp().side_by_side.set(true);
                win1.re_render_diff();
            }
        });
        let win2 = self.clone();
        unified_btn.connect_toggled(move |btn| {
            if btn.is_active() {
                win2.imp().side_by_side.set(false);
                win2.re_render_diff();
            }
        });

        diff_box.append(&imp.diff_stack);

        // ==========================================
        // LAYOUT ASSEMBLY
        // ==========================================

        // Center: diff + ViewSwitcherBar at the bottom of diff area only
        let view_switcher_bar = adw::ViewSwitcherBar::new();
        view_switcher_bar.set_stack(Some(&imp.view_stack));
        view_switcher_bar.set_reveal(true);

        let center_toolbar = adw::ToolbarView::new();
        center_toolbar.set_content(Some(&diff_box));
        center_toolbar.add_bottom_bar(&view_switcher_bar);
        center_toolbar.set_bottom_bar_style(adw::ToolbarStyle::Raised);

        // Inner split: content = center (diff + switcher), sidebar = ViewStack (right)
        let inner_split = adw::OverlaySplitView::new();
        inner_split.set_sidebar_position(gtk::PackType::End);
        inner_split.set_sidebar(Some(&middle_box));
        inner_split.set_content(Some(&center_toolbar));
        inner_split.set_collapsed(false);
        inner_split.set_show_sidebar(true);
        inner_split.set_min_sidebar_width(250.0);
        inner_split.set_max_sidebar_width(400.0);

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

        // Toggle right sidebar (commits/changes)
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

        // Medium (<1000px): collapse inner split (commits/changes overlay)
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
        bp_narrow.add_setter(&view_toggle_box, "visible", Some(&false.to_value()));
        bp_narrow.add_setter(&indicators, "visible", Some(&false.to_value()));

        let win_narrow = self.clone();
        let split_btn_ref = split_btn.clone();
        bp_narrow.connect_apply(move |_| {
            win_narrow.imp().side_by_side.set(false);
            win_narrow.re_render_diff();
        });
        let win_wide = self.clone();
        bp_narrow.connect_unapply(move |_| {
            if split_btn_ref.is_active() {
                win_wide.imp().side_by_side.set(true);
                win_wide.re_render_diff();
            }
        });
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
        self.populate_commit_list(&commits);
        *imp.commits.borrow_mut() = commits;

        // Load status for staging
        if let Ok(status) = repo.status() {
            staging_area::populate_file_list(&imp.unstaged_list, &status.unstaged, &status.untracked);
            staging_area::populate_staged_list(&imp.staged_list, &status.staged);
        }

        // Reset diff & search
        imp.diff_stack.set_visible_child_name("placeholder");
        *imp.selected_commit_id.borrow_mut() = None;
        imp.search_entry.set_text("");
        imp.commit_list_box.set_filter_func(|_| true);
    }

    fn populate_commit_list(&self, commits: &[CommitInfo]) {
        let list_box = &self.imp().commit_list_box;

        while let Some(child) = list_box.first_child() {
            list_box.remove(&child);
        }

        for commit_info in commits {
            let row = commit_list::create_commit_row(commit_info);
            list_box.append(&row);
        }
    }

    fn on_commit_selected(&self, index: usize) {
        let imp = self.imp();
        let commits = imp.commits.borrow();

        if let Some(commit) = commits.get(index) {
            *imp.selected_commit_id.borrow_mut() = Some(commit.id.clone());
            drop(commits);
            self.re_render_diff();

            // In collapsed mode, hide sidebar to show content
            if let Some(ref split) = *imp.inner_split.borrow() {
                if split.is_collapsed() {
                    split.set_show_sidebar(false);
                }
            }
        }
    }

    fn re_render_diff(&self) {
        let imp = self.imp();
        let commit_id = imp.selected_commit_id.borrow().clone();

        let Some(commit_id) = commit_id else {
            return;
        };

        let repo_ref = imp.repo.borrow();
        let Some(ref repo) = *repo_ref else {
            return;
        };

        match repo.diff_commit(&commit_id) {
            Ok(files) => {
                self.render_diff_files(&files);
            }
            Err(e) => {
                tracing::error!("Failed to get diff: {}", e);
            }
        }
    }

    fn render_diff_files(&self, files: &[gitpulsar_core::models::DiffFile]) {
        let imp = self.imp();
        if imp.side_by_side.get() {
            diff_view::render_side_by_side(
                &imp.diff_left_view.buffer(),
                &imp.diff_right_view.buffer(),
                files,
            );
            imp.diff_stack.set_visible_child_name("side-by-side");
        } else {
            diff_view::render_unified(&imp.diff_unified_view.buffer(), files);
            imp.diff_stack.set_visible_child_name("unified");
        }
    }

    fn on_unstaged_file_selected(&self) {
        let imp = self.imp();
        let repo_ref = imp.repo.borrow();
        let Some(ref repo) = *repo_ref else { return };

        match repo.diff_unstaged() {
            Ok(files) => {
                // Get selected row index to find corresponding file
                if let Some(row) = imp.unstaged_list.selected_row() {
                    let idx = row.index() as usize;
                    if let Some(file) = files.get(idx) {
                        *imp.selected_commit_id.borrow_mut() = None;
                        self.render_diff_files(&[file.clone()]);
                    } else {
                        // Might be an untracked file beyond the unstaged list
                        imp.diff_stack.set_visible_child_name("placeholder");
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to get unstaged diff: {}", e);
            }
        }
    }

    fn on_staged_file_selected(&self) {
        let imp = self.imp();
        let repo_ref = imp.repo.borrow();
        let Some(ref repo) = *repo_ref else { return };

        match repo.diff_staged() {
            Ok(files) => {
                if let Some(row) = imp.staged_list.selected_row() {
                    let idx = row.index() as usize;
                    if let Some(file) = files.get(idx) {
                        *imp.selected_commit_id.borrow_mut() = None;
                        self.render_diff_files(&[file.clone()]);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to get staged diff: {}", e);
            }
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

        let repo_ref = imp.repo.borrow();
        if let Some(ref repo) = *repo_ref {
            match repo.commit(message) {
                Ok(oid) => {
                    tracing::info!("Created commit: {}", oid);
                    buffer.set_text("");
                    drop(repo_ref);
                    // Reload repo data
                    let repo_ref = self.imp().repo.borrow();
                    if let Some(ref repo) = *repo_ref {
                        self.load_repo_data(repo);
                    }
                    drop(repo_ref);
                    // Also refresh workspace indicators
                    self.scan_indicators();
                }
                Err(e) => {
                    tracing::error!("Failed to commit: {}", e);
                    drop(repo_ref);
                    let dialog = adw::MessageDialog::new(
                        Some(self),
                        Some("Commit Failed"),
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

    /// Reload just the staging lists (after stage/unstage/discard).
    fn refresh_staging(&self) {
        let imp = self.imp();
        let repo_ref = imp.repo.borrow();
        if let Some(ref repo) = *repo_ref {
            if let Ok(status) = repo.status() {
                staging_area::populate_file_list(&imp.unstaged_list, &status.unstaged, &status.untracked);
                staging_area::populate_staged_list(&imp.staged_list, &status.staged);
            }
        }
    }

    /// Wire up per-row buttons (stage/unstage/discard) using GestureClick on lists.
    fn setup_row_button_signals(&self) {
        // Unstaged list: stage-file and discard-file buttons
        let win = self.clone();
        let gesture = gtk::GestureClick::new();
        gesture.connect_released(move |gesture, _, x, y| {
            let Some(widget) = gesture.widget() else { return };
            let Some(target) = widget.pick(x, y, gtk::PickFlags::DEFAULT) else { return };
            // Walk up to find the button
            let mut current = Some(target);
            while let Some(w) = current {
                if let Ok(btn) = w.clone().downcast::<gtk::Button>() {
                    let name = btn.widget_name();
                    // Find the row this button belongs to
                    let mut parent = btn.parent();
                    while let Some(p) = parent {
                        if let Ok(row) = p.clone().downcast::<gtk::ListBoxRow>() {
                            if let Some(path) = staging_area::get_row_file_path(&row) {
                                if name == "stage-file" {
                                    win.stage_file(&path);
                                } else if name == "discard-file" {
                                    win.discard_file_with_confirm(&path);
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
        self.imp().unstaged_list.add_controller(gesture);

        // Staged list: unstage-file button
        let win = self.clone();
        let gesture = gtk::GestureClick::new();
        gesture.connect_released(move |gesture, _, x, y| {
            let Some(widget) = gesture.widget() else { return };
            let Some(target) = widget.pick(x, y, gtk::PickFlags::DEFAULT) else { return };
            let mut current = Some(target);
            while let Some(w) = current {
                if let Ok(btn) = w.clone().downcast::<gtk::Button>() {
                    let name = btn.widget_name();
                    let mut parent = btn.parent();
                    while let Some(p) = parent {
                        if let Ok(row) = p.clone().downcast::<gtk::ListBoxRow>() {
                            if let Some(path) = staging_area::get_row_file_path(&row) {
                                if name == "unstage-file" {
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
        self.imp().staged_list.add_controller(gesture);
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

    fn populate_branch_lists(&self, local_list: &gtk::ListBox, remote_list: &gtk::ListBox) {
        // Clear lists
        while let Some(child) = local_list.first_child() {
            local_list.remove(&child);
        }
        while let Some(child) = remote_list.first_child() {
            remote_list.remove(&child);
        }

        let repo_ref = self.imp().repo.borrow();
        let Some(ref repo) = *repo_ref else { return };

        let branches = match repo.branches() {
            Ok(b) => b,
            Err(_) => return,
        };

        for branch in &branches {
            let row = adw::ActionRow::builder()
                .title(&branch.name)
                .activatable(true)
                .build();
            row.set_widget_name(&branch.name);

            if branch.is_head {
                row.add_prefix(&gtk::Image::from_icon_name("object-select-symbolic"));
            }

            if !branch.is_remote && (branch.ahead > 0 || branch.behind > 0) {
                let indicator = gtk::Label::new(Some(&format!("▲{} ▼{}", branch.ahead, branch.behind)));
                indicator.add_css_class("caption");
                indicator.add_css_class("dim-label");
                row.add_suffix(&indicator);
            }

            if branch.is_remote {
                remote_list.append(&row);
            } else {
                local_list.append(&row);
            }
        }
    }

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
        glib::timeout_add_seconds_local(5, move || {
            let Some(win) = win.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let imp = win.imp();
            let repo_ref = imp.repo.borrow();
            if let Some(ref repo) = *repo_ref {
                // Refresh staging
                if let Ok(status) = repo.status() {
                    staging_area::populate_file_list(&imp.unstaged_list, &status.unstaged, &status.untracked);
                    staging_area::populate_staged_list(&imp.staged_list, &status.staged);

                    // Update ahead/behind
                    imp.ahead_label.set_label(&format!("▲ {}", status.ahead));
                    imp.behind_label.set_label(&format!("▼ {}", status.behind));
                }
            }
            drop(repo_ref);
            // Refresh workspace indicators
            win.scan_indicators();
            glib::ControlFlow::Continue
        });
    }

    /// Refresh indicators for all workspace entries.
    fn scan_indicators(&self) {
        let imp = self.imp();
        let root = {
            let entries = imp.workspace_entries.borrow();
            if entries.is_empty() {
                return;
            }
            entries[0]
                .path
                .parent()
                .unwrap_or(&entries[0].path)
                .to_path_buf()
        };

        if let Ok(new_entries) = workspace::scan_workspace(&root) {
            repo_tree::populate_repo_list(&imp.repo_list_box, &new_entries);
            *imp.workspace_entries.borrow_mut() = new_entries;
        }
    }
}
