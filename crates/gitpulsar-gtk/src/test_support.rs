//! Initialise GTK + libadwaita exactly once across the whole test process.
//! All UI-touching tests must `#[serial]` so they don't race on the global.

use std::sync::Once;

static INIT: Once = Once::new();

pub fn ensure_gtk_init() {
    INIT.call_once(|| {
        // gtk_init can fail in a CI container with no DISPLAY — in that
        // case the test binary will not be able to construct widgets.
        // Treat init failure as a soft-skip via env var so headless CI
        // can still run the rest of the suite.
        if std::env::var("GP_SKIP_GTK_TESTS").is_ok() {
            return;
        }
        gtk::init().expect("gtk::init() failed; set GP_SKIP_GTK_TESTS=1 to skip");
        adw::init().expect("adw::init() failed");
    });
}

pub fn gtk_available() -> bool {
    std::env::var("GP_SKIP_GTK_TESTS").is_err()
}
