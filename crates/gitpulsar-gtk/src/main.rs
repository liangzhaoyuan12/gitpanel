mod app;
mod config;
mod undo;
mod widgets;

use app::GitpulsarApp;

fn main() {
    tracing_subscriber::fmt::init();

    let app = GitpulsarApp::new();
    app.run();
}
