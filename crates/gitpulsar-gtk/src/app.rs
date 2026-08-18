use adw::prelude::*;
use gtk::{gio, glib};

use crate::widgets::window::GitpulsarWindow;

pub struct GitpulsarApp {
    app: adw::Application,
}

impl GitpulsarApp {
    pub fn new() -> Self {
        let app = adw::Application::builder()
            .application_id("io.gitlab.ilshat_apps.gitpulsar")
            .flags(gio::ApplicationFlags::HANDLES_OPEN)
            .build();

        app.connect_activate(|app| {
            // Set window icon
            gtk::Window::set_default_icon_name("io.gitlab.ilshat_apps.gitpulsar");

            // Custom CSS for repo indicators — explicit colors that don't follow user accent
            let css = "
                label.gp-ind-yellow { color: #e5a50a; }
                label.gp-ind-blue { color: #3584e4; }
                label.gp-ind-green { color: #26a269; }
                /* Touch-friendly row heights once mobile breakpoint applies. */
                .gp-mobile .gp-file-row { min-height: 48px; }
                .gp-mobile listview.navigation-sidebar > row { min-height: 48px; }
                /* Below 600 sp the AdwViewSwitcherBar drops to icon-only and
                 * the bar gets a tighter vertical footprint — the default
                 * narrow-mode AdwViewSwitcher is too tall once the labels are
                 * gone. */
                .gp-narrow viewswitcherbar button label,
                .gp-mobile viewswitcherbar button label { font-size: 0; min-height: 0; padding: 0; margin: 0; }
                .gp-narrow viewswitcherbar button,
                .gp-mobile viewswitcherbar button { min-width: 48px; min-height: 28px; padding: 2px 8px; }
                .gp-narrow viewswitcherbar > revealer > box,
                .gp-mobile viewswitcherbar > revealer > box { min-height: 32px; padding: 0; }
            ";
            let provider = gtk::CssProvider::new();
            provider.load_from_string(css);
            if let Some(display) = gtk::gdk::Display::default() {
                gtk::style_context_add_provider_for_display(
                    &display,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }

            // Add icon search path for development builds
            let display = gtk::gdk::Display::default().unwrap();
            let icon_theme = gtk::IconTheme::for_display(&display);
            // Check relative to executable (cargo run)
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()));
            if let Some(dir) = exe_dir {
                // Try <project>/data/icons
                let data_icons = dir.join("../../data/icons");
                if data_icons.exists() {
                    icon_theme.add_search_path(data_icons);
                }
            }
            // Also check from current working dir
            let cwd_icons = std::path::PathBuf::from("data/icons");
            if cwd_icons.exists() {
                icon_theme.add_search_path(cwd_icons);
            }

            let window = GitpulsarWindow::new(app);
            window.present();
        });

        // Keyboard shortcuts
        app.set_accels_for_action("win.open-repo", &["<Control>o"]);
        app.set_accels_for_action("win.open-in-editor", &["<Control><Shift>o"]);

        app.set_accels_for_action("win.commit", &["<Control>Return"]);
        app.set_accels_for_action("win.stage-all", &["<Control><Shift>s"]);
        app.set_accels_for_action("win.unstage-all", &["<Control><Shift>u"]);
        app.set_accels_for_action("win.fetch", &["<Control><Shift>f"]);
        app.set_accels_for_action("win.push", &["<Control><Shift>p"]);
        app.set_accels_for_action("win.pull", &["<Control><Shift>l"]);
        app.set_accels_for_action("win.show-commits", &["<Control>1"]);
        app.set_accels_for_action("win.show-changes", &["<Control>2"]);
        app.set_accels_for_action("win.show-graph", &["<Control>3"]);
        app.set_accels_for_action("win.focus-search", &["<Control>f"]);
        app.set_accels_for_action("win.undo", &["<Control>z"]);
        app.set_accels_for_action("win.redo", &["<Control><Shift>z"]);
        app.set_accels_for_action("win.stash-save", &["<Control><Alt>s"]);
        app.set_accels_for_action("win.stash-pop", &["<Control><Alt>p"]);
        app.set_accels_for_action("app.quit", &["<Control>q"]);

        let quit = gio::SimpleAction::new("quit", None);
        let app_for_quit = app.clone();
        quit.connect_activate(move |_, _| app_for_quit.quit());
        app.add_action(&quit);

        Self { app }
    }

    pub fn run(&self) -> glib::ExitCode {
        self.app.run()
    }
}
