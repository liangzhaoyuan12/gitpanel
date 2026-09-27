#![cfg_attr(windows, windows_subsystem = "windows")]

use gitpanel::app::GitpanelApp;
use gitpanel::i18n;
use gitpanel::utils::{config::AppConfig, logging, rebase};

fn main() {
    // git 把本程序再次作为序列编辑器拉起：拷贝 todo 后立即退出，
    // 不碰日志/GTK/GApplication（单实例机制会吞掉这次二次调用）。
    rebase::maybe_run_seq_edit();

    if let Err(e) = logging::init_logging() {
        eprintln!("Failed to initialise logging: {e}");
    }

    // Initialize i18n language from saved config
    let config = AppConfig::load();
    i18n::init_language(config.language);

    let app = GitpanelApp::new();
    app.run();
}
