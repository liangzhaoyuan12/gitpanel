use gitpanel::app::GitpanelApp;
use gitpanel::i18n;
use gitpanel::utils::{config::AppConfig, logging};

fn main() {
    if let Err(e) = logging::init_logging() {
        eprintln!("Failed to initialise logging: {e}");
    }

    // Initialize i18n language from saved config
    let config = AppConfig::load();
    i18n::init_language(config.language);

    let app = GitpanelApp::new();
    app.run();
}
