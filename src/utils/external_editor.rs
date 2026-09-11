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

use std::path::Path;
use std::process::{Command, Stdio};

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

/// Shell snippet that prints, one per line, whichever of `commands` resolves.
///
/// One probe for the whole list rather than one per command — ~25 process
/// spawns to answer a single question would be wasteful.
pub fn detect_script(commands: &[&str]) -> String {
    let list = commands.join(" ");
    format!("for c in {list}; do command -v \"$c\" >/dev/null 2>&1 && printf '%s\\n' \"$c\"; done")
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
/// Returns nothing under Flatpak: the sandbox has its own PATH, so a probe
/// would only ever find the runtime's binaries, never the user's editor.
pub fn detect_installed() -> Vec<DetectedEditor> {
    if in_flatpak() {
        return Vec::new();
    }

    let all: Vec<&str> = KNOWN_EDITORS
        .iter()
        .flat_map(|e| e.commands.iter().copied())
        .collect();
    let script = detect_script(&all);

    let output = match Command::new("sh").arg("-c").arg(&script).output() {
        Ok(out) => out,
        Err(e) => {
            tracing::warn!("Editor detection failed: {e}");
            return Vec::new();
        }
    };

    let found: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
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

    let argv = build_argv(command_line, &repo_path.to_string_lossy())?;

    tracing::info!("Opening {} with: {}", repo_path.display(), argv.join(" "));

    let child = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| anyhow!("Could not run `{}`: {e}", argv[0]))?;

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
    fn detect_script_probes_every_candidate_once() {
        let script = detect_script(&["code", "zed"]);
        assert!(script.contains("for c in code zed;"));
        assert!(script.contains("command -v"));
    }
}
