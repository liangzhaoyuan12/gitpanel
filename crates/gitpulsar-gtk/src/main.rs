mod app;
mod widgets;

use app::GitpulsarApp;

fn main() {
    tracing_subscriber::fmt::init();

    let app = GitpulsarApp::new();
    app.run();
}
