use adw::prelude::*;
use gtk::{gio, glib};

use crate::widgets::window::GitpulsarWindow;

pub struct GitpulsarApp {
    app: adw::Application,
}

impl GitpulsarApp {
    pub fn new() -> Self {
        let app = adw::Application::builder()
            .application_id("dev.gitpulsar.Gitpulsar")
            .flags(gio::ApplicationFlags::HANDLES_OPEN)
            .build();

        app.connect_activate(|app| {
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
        app.set_accels_for_action("win.focus-search", &["<Control>f"]);
        app.set_accels_for_action("win.stash-save", &["<Control>z"]);
        app.set_accels_for_action("win.stash-pop", &["<Control><Shift>z"]);

        Self { app }
    }

    pub fn run(&self) -> glib::ExitCode {
        self.app.run()
    }
}
