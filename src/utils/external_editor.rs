//! Launching an external editor or IDE on the currently open repository.
//!
//! This module covers the unsandboxed case only — a distribution package, a
//! build from source, or the AppImage — where the editor can simply be spawned.
//!
//! Inside a Flatpak sandbox neither detection nor spawning is possible: host
//! binaries are invisible, and reaching them would need
//! `--talk-name=org.freedesktop.Flatpak`, which Flathub's linter rejects
//! outright (`finish-args-flatpak-spawn-access`). There the UI asks the XDG
//! desktop portal to pick an application instead — no permission required.
//! See `GitpanelWindow::open_with_portal`.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{anyhow, Result};

/// An editor we know how to launch.
pub struct KnownEditor {
    pub name: &'static str,
    /// Candidate commands in preference order. Flatpak application IDs work as
    /// commands too — Flatpak exports them into the host PATH under
    /// `/var/lib/flatpak/exports/bin`, so `com.visualstudio.code /repo` runs.
    pub commands: &'static [&'static str],
}

/// Editors that accept a directory as their positional argument.
pub const KNOWN_EDITORS: &[KnownEditor] = &[
    KnownEditor { name: "VS Code", commands: &["code", "com.visualstudio.code"] },
    KnownEditor { name: "VSCodium", commands: &["codium", "com.vscodium.codium"] },
    KnownEditor { name: "Cursor", commands: &["cursor"] },
    KnownEditor { name: "Windsurf", commands: &["windsurf"] },
    KnownEditor { name: "Zed", commands: &["zed", "dev.zed.Zed"] },
    KnownEditor { name: "GNOME Builder", commands: &["gnome-builder", "org.gnome.Builder"] },
    KnownEditor { name: "Kate", commands: &["kate", "org.kde.kate"] },
    KnownEditor { name: "KDevelop", commands: &["kdevelop", "org.kde.kdevelop"] },
    KnownEditor { name: "Sublime Text", commands: &["subl"] },
    KnownEditor { name: "Qt Creator", commands: &["qtcreator", "io.qt.QtCreator"] },
    KnownEditor { name: "Emacs", commands: &["emacs", "org.gnu.emacs"] },
    KnownEditor { name: "IntelliJ IDEA", commands: &["idea"] },
    KnownEditor { name: "PyCharm", commands: &["pycharm"] },
    KnownEditor { name: "CLion", commands: &["clion"] },
    KnownEditor { name: "RustRover", commands: &["rustrover"] },
    KnownEditor { name: "GoLand", commands: &["goland"] },
    KnownEditor { name: "WebStorm", commands: &["webstorm"] },
    KnownEditor { name: "PhpStorm", commands: &["phpstorm"] },
    KnownEditor { name: "Android Studio", commands: &["studio", "com.google.AndroidStudio"] },
];

/// An editor found on this system, paired with the command it was found under.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedEditor {
    pub name: String,
    pub command: String,
}

/// True when running inside a Flatpak sandbox, where this module does not apply.
///
/// `GP_SIMULATE_FLATPAK=1` forces the sandboxed behaviour on an ordinary
/// desktop. That path is otherwise only reachable from a real Flatpak build,
/// and shipping it unexercised is how the broken v1.3.0 reached Flathub.
pub fn in_flatpak() -> bool {
    if std::env::var("GP_SIMULATE_FLATPAK").is_ok_and(|v| v == "1") {
        return true;
    }
    Path::new("/.flatpak-info").exists()
}

/// Split a command line into argv, honouring single and double quotes and
/// backslash escapes. Users type things like `flatpak run com.foo.Bar --new`
/// into the custom-command field, and splitting on whitespace alone would break
/// any path containing a space.
pub fn split_command(line: &str) -> Vec<String> {
    let mut argv = Vec::new();
    let mut current = String::new();
    let mut has_token = false;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in line.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                } else if ch == '\\' && q == '"' {
                    escaped = true;
                } else {
                    current.push(ch);
                }
            }
            None => match ch {
                '\'' | '"' => {
                    quote = Some(ch);
                    has_token = true;
                }
                '\\' => escaped = true,
                c if c.is_whitespace() => {
                    if has_token {
                        argv.push(std::mem::take(&mut current));
                        has_token = false;
                    }
                }
                c => {
                    current.push(c);
                    has_token = true;
                }
            },
        }
    }

    if has_token {
        argv.push(current);
    }
    argv
}

/// Build the argv for `command_line` opening `repo_path`.
///
/// A `{path}` placeholder anywhere in the command line is substituted; without
/// one the path is appended as the final argument, which is what every editor in
/// [`KNOWN_EDITORS`] expects.
pub fn build_argv(command_line: &str, repo_path: &str) -> Result<Vec<String>> {
    let mut argv = split_command(command_line);
    if argv.is_empty() {
        return Err(anyhow!("The editor command is empty"));
    }

    let mut substituted = false;
    for arg in argv.iter_mut() {
        if arg.contains("{path}") {
            *arg = arg.replace("{path}", repo_path);
            substituted = true;
        }
    }
    if !substituted {
        argv.push(repo_path.to_string());
    }
    Ok(argv)
}

/// 探测用的可执行目录：PATH + 平台补充目录。
///
/// macOS 上 Finder/Dock 启动的 GUI 进程只有 launchd 的极简 PATH，
/// Homebrew/MacPorts 目录不在里面，GUI 宿主必须自己补。
pub fn executable_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    #[cfg(target_os = "macos")]
    for extra in ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"] {
        let p = PathBuf::from(extra);
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    }
    dirs
}

/// Windows 的可执行后缀表（PATHEXT）；非 Windows 返回空表，
/// 空表即“Unix 模式”（要求可执行位）。
pub fn pathext() -> Vec<String> {
    #[cfg(windows)]
    {
        let raw = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        return raw
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_uppercase())
            .collect();
    }
    #[cfg(not(windows))]
    Vec::new()
}

/// Unix：文件存在且带可执行位（对齐 `command -v` 语义）。
fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return path
            .metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }
    #[cfg(not(unix))]
    true
}

/// 在给定目录表里解析一个命令名，纯函数，便于测试。
///
/// - 名字含路径分隔符 → 当作显式路径，存在即用；
/// - Windows 模式（`pathext` 非空）→ 依次尝试 `名+后缀`（Windows 文件系统
///   大小写不敏感，后缀大小写不用管）；
/// - Unix 模式 → 直接拼接并检查可执行位。
pub fn which_in(name: &str, dirs: &[PathBuf], pathext: &[String]) -> Option<PathBuf> {
    if name.contains('/') || name.contains('\\') {
        let p = PathBuf::from(name);
        return p.is_file().then_some(p);
    }
    for dir in dirs {
        if pathext.is_empty() {
            let p = dir.join(name);
            if is_executable_file(&p) {
                return Some(p);
            }
        } else {
            for ext in pathext {
                // PATHEXT 惯例是大写（.EXE），实际文件名常是小写（code.cmd）。
                // Windows 文件系统大小写不敏感所以无所谓，但本函数要在
                // 大小写敏感的系统上也能被测到，原样和小写都试一遍。
                for suffix in [ext.clone(), ext.to_lowercase()] {
                    let p = dir.join(format!("{name}{suffix}"));
                    if p.is_file() {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}

/// 解析当前平台上的一个命令名。
pub fn which(name: &str) -> Option<PathBuf> {
    which_in(name, &executable_dirs(), &pathext())
}

/// Map the probe output back onto [`KNOWN_EDITORS`], keeping catalogue order and
/// picking each editor's most-preferred available command.
pub fn match_detected(found: &[String]) -> Vec<DetectedEditor> {
    KNOWN_EDITORS
        .iter()
        .filter_map(|editor| {
            editor
                .commands
                .iter()
                .find(|cmd| found.iter().any(|f| f == *cmd))
                .map(|cmd| DetectedEditor {
                    name: editor.name.to_string(),
                    command: (*cmd).to_string(),
                })
        })
        .collect()
}

/// Human-readable label for a stored command line.
pub fn label_for_command(command_line: &str) -> String {
    let argv = split_command(command_line);
    let Some(program) = argv.first() else {
        return "External Editor".to_string();
    };

    if let Some(editor) = KNOWN_EDITORS
        .iter()
        .find(|e| e.commands.iter().any(|c| c == program))
    {
        return editor.name.to_string();
    }

    // Custom command: show the executable's basename, not the whole line.
    program
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(program)
        .to_string()
}

/// Probe the system for installed editors. Blocking — call from a worker thread.
///
/// 纯 Rust 扫描 PATH，不经过 `sh -c command -v`（Windows 没有 sh）。
///
/// Returns nothing under Flatpak: the sandbox has its own PATH, so a probe
/// would only ever find the runtime's binaries, never the user's editor.
pub fn detect_installed() -> Vec<DetectedEditor> {
    if in_flatpak() {
        return Vec::new();
    }

    let found: Vec<String> = KNOWN_EDITORS
        .iter()
        .flat_map(|e| e.commands.iter().copied())
        .filter(|c| which(c).is_some())
        .map(|c| c.to_string())
        .collect();

    match_detected(&found)
}

/// Launch `command_line` on `repo_path`, detached from this process.
///
/// Refuses under Flatpak rather than spawning something meaningless inside the
/// sandbox — callers must route that case through the portal instead.
pub fn launch(command_line: &str, repo_path: &Path) -> Result<()> {
    if in_flatpak() {
        return Err(anyhow!(
            "Host applications cannot be started from inside the Flatpak sandbox"
        ));
    }

    let mut argv = build_argv(command_line, &repo_path.to_string_lossy())?;
    let program = argv.remove(0);

    // Windows 上 CreateProcess 只补 .exe，`code` 这种 .cmd 光靠 std 解析不到，
    // 必须自己按 PATHEXT 落到完整路径；解析不到就回退原名，让 spawn 报原始错误。
    let resolved = if program.contains('/') || program.contains('\\') {
        PathBuf::from(&program)
    } else {
        which(&program).unwrap_or_else(|| PathBuf::from(&program))
    };

    tracing::info!(
        "Opening {} with: {} {:?}",
        repo_path.display(),
        resolved.display(),
        argv
    );

    let child = crate::utils::process::command(&resolved)
        .args(&argv)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| anyhow!("Could not run `{}`: {e}", resolved.display()))?;

    // Reap the child so long-running sessions do not accumulate zombies.
    std::thread::spawn(move || {
        let mut child = child;
        let _ = child.wait();
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_command_gets_the_path_appended() {
        assert_eq!(
            build_argv("code", "/home/u/repo").unwrap(),
            vec!["code", "/home/u/repo"]
        );
    }

    #[test]
    fn placeholder_is_substituted_instead_of_appended() {
        assert_eq!(
            build_argv("myide --open {path} --wait", "/home/u/repo").unwrap(),
            vec!["myide", "--open", "/home/u/repo", "--wait"]
        );
    }

    #[test]
    fn quoted_arguments_survive_splitting() {
        assert_eq!(
            split_command("'/opt/My IDE/bin/ide' --flag \"two words\""),
            vec!["/opt/My IDE/bin/ide", "--flag", "two words"]
        );
    }

    #[test]
    fn empty_command_is_rejected() {
        assert!(build_argv("   ", "/repo").is_err());
    }


    #[test]
    fn detection_prefers_the_first_listed_command() {
        let found = vec!["com.visualstudio.code".to_string(), "code".to_string()];
        let detected = match_detected(&found);
        assert_eq!(
            detected,
            vec![DetectedEditor {
                name: "VS Code".to_string(),
                command: "code".to_string(),
            }]
        );
    }

    #[test]
    fn detection_keeps_catalogue_order() {
        let found = vec!["zed".to_string(), "code".to_string()];
        let names: Vec<String> = match_detected(&found).into_iter().map(|d| d.name).collect();
        assert_eq!(names, vec!["VS Code", "Zed"]);
    }

    #[test]
    fn known_commands_are_labelled_by_product_name() {
        assert_eq!(label_for_command("com.visualstudio.code"), "VS Code");
        assert_eq!(label_for_command("code --new-window"), "VS Code");
    }

    #[test]
    fn custom_commands_are_labelled_by_basename() {
        assert_eq!(label_for_command("/opt/tools/bin/myide --flag"), "myide");
        assert_eq!(label_for_command(""), "External Editor");
    }

    #[test]
    fn which_finds_executable_in_unix_mode() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("code");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let dirs = vec![dir.path().to_path_buf()];
        assert_eq!(which_in("code", &dirs, &[]), Some(bin));
    }

    #[test]
    fn which_skips_non_executable_in_unix_mode() {
        #[cfg(unix)]
        {
            let dir = tempfile::tempdir().unwrap();
            let bin = dir.path().join("code");
            std::fs::write(&bin, "#!/bin/sh\n").unwrap(); // 0644，默认无执行位
            let dirs = vec![dir.path().to_path_buf()];
            assert_eq!(which_in("code", &dirs, &[]), None);
        }
    }

    #[test]
    fn which_simulated_windows_tries_pathext_suffixes() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("code.cmd");
        std::fs::write(&bin, "@echo off\r\n").unwrap();
        let dirs = vec![dir.path().to_path_buf()];
        let exts = vec![".COM".to_string(), ".EXE".to_string(), ".CMD".to_string()];
        assert_eq!(which_in("code", &dirs, &exts), Some(bin));
        assert_eq!(which_in("codium", &dirs, &exts), None);
    }

    #[test]
    fn which_honours_explicit_paths_verbatim() {
        let dirs: Vec<PathBuf> = vec![];
        assert_eq!(which_in("/no/such/place/tool", &dirs, &[]), None);
    }
}
