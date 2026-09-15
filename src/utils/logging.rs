//! Application-wide logging built on the `tracing` facade.
//!
//! Gitpanel is privacy-first: nothing leaves the machine. Logs are therefore
//! written to a size-rotating file in the user's data directory
//! (`~/.local/share/io.github.liangzhaoyuan12/logs/gitpanel.log` by default),
//! never to a remote endpoint and never to any telemetry service.
//!
//! Initialise once at start-up with [`init_logging`]; afterwards every
//! `tracing::info!` / `warn!` / `error!` / `debug!` / `trace!` call already
//! scattered through the codebase is captured.
//!
//! Environment overrides:
//! * `GP_LOG_DIR`    — directory for the log file (default: data dir + `logs`)
//! * `GP_LOG_STDOUT` — set to `1` to also mirror logs to stdout (debugging)
//! * `GP_LOG_LEVEL`  — `trace`/`debug`/`info`/`warn`/`error` (overrides default)
//! * `RUST_LOG`      — standard `tracing` filter; takes precedence when set

use std::env;
use std::fs;
use std::str::FromStr;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use tracing::Level;
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::writer::MakeWriter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

/// Configuration for the logging subsystem.
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Directory that holds the log file and its rotated backups.
    pub dir: PathBuf,
    /// Rotate once the current file reaches this many bytes.
    pub max_bytes: u64,
    /// Number of rotated backups to keep (`gitpanel.log.1` … `.{max_files}`).
    pub max_files: usize,
    /// Minimum level emitted when `RUST_LOG` is not set.
    pub level: Level,
    /// Also print to stdout (ignored by desktop launches, handy in a terminal).
    pub to_stdout: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        let dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("io.github.liangzhaoyuan12")
            .join("logs");
        Self {
            dir,
            max_bytes: 2 * 1024 * 1024, // 2 MiB
            max_files: 5,
            level: Level::INFO,
            to_stdout: false,
        }
    }
}

/// Default log directory (before any env override is applied).
pub fn log_directory() -> PathBuf {
    LogConfig::default().dir
}

/// Path of the active log file.
pub fn current_log_file() -> PathBuf {
    log_directory().join("gitpanel.log")
}

/// Initialise logging with the default [`LogConfig`], honouring `GP_*` env vars.
pub fn init_logging() -> anyhow::Result<()> {
    let mut config = LogConfig::default();

    if let Ok(dir) = env::var("GP_LOG_DIR") {
        if !dir.trim().is_empty() {
            config.dir = PathBuf::from(dir.trim());
        }
    }
    if env::var("GP_LOG_STDOUT").is_ok_and(|v| v == "1") {
        config.to_stdout = true;
    }
    if let Ok(lvl) = env::var("GP_LOG_LEVEL") {
        if let Ok(parsed) = Level::from_str(lvl.trim()) {
            config.level = parsed;
        }
    }

    init_logging_with(config)
}

/// Initialise logging with an explicit configuration.
///
/// Safe to call more than once: only the first call installs a subscriber;
/// later calls are ignored so tests and library consumers cannot clobber it.
pub fn init_logging_with(config: LogConfig) -> anyhow::Result<()> {
    let file_writer = RotatingFileWriter::new(&config.dir, config.max_bytes, config.max_files)?;

    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_target(true)
        .with_thread_names(true)
        .with_file(false)
        .with_line_number(false)
        .with_timer(fmt::time::SystemTime);

    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(config.level.as_str().to_ascii_lowercase()))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let stdout_layer = if config.to_stdout {
        Some(
            fmt::layer()
                .with_writer(io::stdout)
                .with_ansi(true)
                .with_target(true),
        )
    } else {
        None
    };

    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer);

    match tracing::subscriber::set_global_default(subscriber) {
        Ok(()) => {
            tracing::info!(
                "Logging initialised — level={}, dir={}",
                config.level,
                config.dir.display()
            );
            Ok(())
        }
        Err(_) => {
            // Already initialised (e.g. by a test harness). Non-fatal.
            eprintln!("[gitpanel] logging already initialised; ignoring re-init");
            Ok(())
        }
    }
}

/// A size-rotating log file, safe to share behind an `Arc` and to use as a
/// `tracing_subscriber` writer from many threads at once.
struct RotatingFileWriter {
    inner: Mutex<RotatingInner>,
}

struct RotatingInner {
    dir: PathBuf,
    base_name: String,
    max_bytes: u64,
    max_files: usize,
    file: Option<fs::File>,
    size: u64,
}

impl RotatingFileWriter {
    fn new(
        dir: &std::path::Path,
        max_bytes: u64,
        max_files: usize,
    ) -> anyhow::Result<Self> {
        fs::create_dir_all(dir).map_err(|e| {
            anyhow::anyhow!("Cannot create log directory {}: {e}", dir.display())
        })?;

        let mut inner = RotatingInner {
            dir: dir.to_path_buf(),
            base_name: "gitpanel.log".to_string(),
            max_bytes: max_bytes.max(1),
            max_files: max_files.max(1),
            file: None,
            size: 0,
        };

        // If a previous run left a file already at/over the limit, rotate it
        // away so we don't keep appending to a stale, huge file.
        let path = inner.dir.join(&inner.base_name);
        if let Ok(meta) = fs::metadata(&path) {
            if meta.len() >= inner.max_bytes {
                inner.rotate()?;
            }
        }
        inner.open_fresh()?;

        Ok(Self {
            inner: Mutex::new(inner),
        })
    }
}

impl RotatingInner {
    fn open_fresh(&mut self) -> io::Result<()> {
        let path = self.dir.join(&self.base_name);
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        self.size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        self.file = Some(file);
        Ok(())
    }

    /// Shift existing backups up by one and start a fresh main file.
    fn rotate(&mut self) -> io::Result<()> {
        self.file = None;
        let base = &self.base_name;

        // Drop the oldest backup, then shift the rest up.
        let oldest = self.dir.join(format!("{base}.{}", self.max_files));
        let _ = fs::remove_file(&oldest);
        for i in (1..self.max_files).rev() {
            let src = self.dir.join(format!("{base}.{i}"));
            let dst = self.dir.join(format!("{base}.{}", i + 1));
            if src.exists() {
                let _ = fs::rename(&src, &dst);
            }
        }

        // The current main file becomes backup #1.
        let main = self.dir.join(base);
        if main.exists() {
            let _ = fs::rename(&main, self.dir.join(format!("{base}.1")));
        }

        self.open_fresh()
    }

    fn write_buf(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.file.is_none() {
            self.open_fresh()?;
        }
        if self.size + buf.len() as u64 > self.max_bytes {
            self.rotate()?;
        }
        let n = match &mut self.file {
            Some(f) => f.write(buf)?,
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "log file unavailable after open",
                ))
            }
        };
        self.size += n as u64;
        Ok(n)
    }
}

impl<'a> MakeWriter<'a> for RotatingFileWriter {
    type Writer = RotatingGuard<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        RotatingGuard {
            guard: self
                .inner
                .lock()
                .expect("log writer mutex poisoned"),
        }
    }
}

struct RotatingGuard<'a> {
    guard: MutexGuard<'a, RotatingInner>,
}

impl<'a> io::Write for RotatingGuard<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.guard.write_buf(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.guard.file {
            Some(f) => f.flush(),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[serial_test::serial]
    fn init_writes_a_log_file() {
        let dir = tempfile::tempdir().unwrap();
        env::set_var("GP_LOG_DIR", dir.path());
        env::set_var("GP_LOG_LEVEL", "debug");

        let _ = init_logging();
        tracing::info!("logging test message");

        let log = dir.path().join("gitpanel.log");
        assert!(log.exists(), "log file should be created");
        let contents = fs::read_to_string(&log).unwrap();
        assert!(
            contents.contains("logging test message"),
            "log file should contain the emitted record"
        );
    }

    #[test]
    fn default_config_points_at_data_dir() {
        let config = LogConfig::default();
        assert!(config.dir.ends_with("io.github.liangzhaoyuan12/logs"));
        assert_eq!(config.level, Level::INFO);
        assert!(!config.to_stdout);
    }
}
