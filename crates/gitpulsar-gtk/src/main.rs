mod app;
mod config;
mod undo;
mod widgets;

#[cfg(test)]
mod test_support;

use app::GitpulsarApp;

fn main() {
    tracing_subscriber::fmt::init();

    let app = GitpulsarApp::new();
    app.run();
}
