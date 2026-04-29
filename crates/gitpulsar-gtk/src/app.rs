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
            ";
            let provider = gtk::CssProvider::new();
            provider.load_from_data(css);
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

        Self { app }
    }

    pub fn run(&self) -> glib::ExitCode {
        self.app.run()
    }
}
