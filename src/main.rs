use gitpanel::app::GitpanelApp;
use gitpanel::utils::logging;

fn main() {
    if let Err(e) = logging::init_logging() {
        eprintln!("Failed to initialise logging: {e}");
    }

    let app = GitpanelApp::new();
    app.run();
}
