use gitpanel::app::GitpanelApp;

fn main() {
    tracing_subscriber::fmt::init();

    let app = GitpanelApp::new();
    app.run();
}
