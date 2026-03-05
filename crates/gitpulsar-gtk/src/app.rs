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

        Self { app }
    }

    pub fn run(&self) -> glib::ExitCode {
        self.app.run()
    }
}
