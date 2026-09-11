//! Initialise GTK + libadwaita exactly once across the whole test process, and
//! give tests a thread they are allowed to build widgets on.
//!
//! GTK pins itself to whichever thread calls `gtk::init()` first and asserts on
//! that thread in every widget constructor. The Rust test harness hands each
//! test an arbitrary thread, so initialising from inside a test makes widget
//! construction pass or fail depending on scheduling. Instead we own a single
//! long-lived thread, initialise there, and let tests dispatch closures onto it
//! via [`on_gtk_thread`].

use std::sync::mpsc::{channel, Sender};
use std::sync::OnceLock;

type Job = Box<dyn FnOnce() + Send>;

static GTK_THREAD: OnceLock<Option<Sender<Job>>> = OnceLock::new();

/// Spawn (once) the thread that owns GTK. `None` means initialisation was
/// skipped, and widget-touching tests must soft-skip.
fn gtk_thread() -> Option<&'static Sender<Job>> {
    GTK_THREAD
        .get_or_init(|| {
            // gtk_init can fail in a CI container with no DISPLAY — in that
            // case the test binary will not be able to construct widgets.
            // Treat init failure as a soft-skip via env var so headless CI
            // can still run the rest of the suite.
            if std::env::var("GP_SKIP_GTK_TESTS").is_ok() {
                return None;
            }

            let (job_tx, job_rx) = channel::<Job>();
            let (ready_tx, ready_rx) = channel::<bool>();

            std::thread::Builder::new()
                .name("gtk-test-main".into())
                .spawn(move || {
                    let ok = gtk::init().is_ok() && adw::init().is_ok();
                    let _ = ready_tx.send(ok);
                    if !ok {
                        return;
                    }
                    // Serve widget work until the process exits.
                    while let Ok(job) = job_rx.recv() {
                        job();
                    }
                })
                .expect("spawn gtk test thread");

            match ready_rx.recv() {
                Ok(true) => Some(job_tx),
                _ => panic!("gtk::init() failed; set GP_SKIP_GTK_TESTS=1 to skip"),
            }
        })
        .as_ref()
}

/// Bring GTK up if it isn't already. Safe to call from any test thread.
pub fn ensure_gtk_init() {
    let _ = gtk_thread();
}

pub fn gtk_available() -> bool {
    std::env::var("GP_SKIP_GTK_TESTS").is_err()
}

/// Run `f` on the GTK-owning thread and hand back its result.
///
/// Anything calling a widget constructor must go through here. A panic inside
/// `f` (an `assert!` that fired) surfaces as a failure of the calling test, but
/// its message is lost — prefer returning values and asserting on them in the
/// test body.
pub fn on_gtk_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let tx = gtk_thread().expect("GTK unavailable; guard the test with gtk_available()");
    let (result_tx, result_rx) = channel::<T>();
    tx.send(Box::new(move || {
        let _ = result_tx.send(f());
    }))
    .expect("gtk test thread died");
    result_rx
        .recv()
        .expect("gtk test thread panicked while running the closure")
}
