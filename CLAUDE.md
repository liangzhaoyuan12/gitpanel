# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

GitPulsar — GNOME-native Git GUI written in Rust with GTK4/libadwaita. App ID: `dev.gitpulsar.Gitpulsar`.

## Build & Run

```bash
cargo build --release          # release build
cargo run -p gitpulsar-gtk     # run from source
make install                   # build + install to ~/.local (binary, desktop entry, icon, metainfo)
make uninstall                 # remove installed files
```

**System dependencies**: gtk4-devel, libadwaita-devel (Fedora) / libgtk-4-dev, libadwaita-1-dev (Debian).

There are no tests or lints configured in this project currently.

## Architecture

Two-crate workspace:

- **gitpulsar-core** — pure git operations library with zero UI dependencies. Wraps `git2` in a `GitRepo` struct. All methods return `anyhow::Result<T>`. Models (CommitInfo, BranchInfo, DiffFile, etc.) derive Serialize/Deserialize.
- **gitpulsar-gtk** — GTK4 + libadwaita frontend. Entry point in `main.rs` → `app.rs` (GtkApplication setup + keyboard shortcuts) → `widgets/window.rs` (main window, ~2700 LOC, holds all app state).

### Core modules (gitpulsar-core/src/)

| Module | Purpose |
|---|---|
| repository | Repo open, status, log, branches, ahead/behind |
| models | All shared data types |
| staging | Stage, unstage, commit, discard |
| diff | Unstaged/staged/commit diffs, hunk parsing |
| branch | Checkout local/remote, dirty checks |
| remote | fetch/pull/push (git2 + CLI fallback for SSH) |
| stash | Save, pop, list, drop, apply |
| commit_ops | Cherry-pick, revert, reset |
| tags | List, create (lightweight/annotated) |
| blame | Per-file blame |
| workspace | Multi-repo scanning (1-level deep) |

### Key patterns

- **State management**: `GitpulsarWindow` uses `RefCell`/`Cell` interior mutability (GTK is single-threaded). A background refresh loop polls repo state on a configurable interval (default 15s).
- **Hash-based refresh skipping**: `last_status_hash`, `last_commits_hash`, `last_workspace_hash` — UI updates only when hashes change.
- **Concurrency guard**: `refresh_in_progress` Cell prevents overlapping refreshes.
- **Remote operations**: git2 for local ops, shelled-out `git` CLI (`run_git_cmd`) for remote ops (push/pull/fetch) due to SSH reliability. 30-second timeout.
- **Widget builders**: Functions return `(gtk::Box, SomeRefs)` tuples — the widget and a struct of handles for later updates.
- **Layout**: Outer AdwOverlaySplitView (repo sidebar | main) → inner split (commit list | changes/graph view).
- **Config**: JSON at `~/.config/dev.gitpulsar/config.json` — date format, refresh interval, recent workspaces.
- **CLI open**: App uses `HANDLES_OPEN` flag — accepts repo path as CLI argument (`gitpulsar-gtk /path/to/repo`).
- **Branch graph**: `commit_graph.rs` renders via cairo, not standard GTK widgets — separate drawing model.

## CI

GitLab CI (`.gitlab-ci.yml`): triggers on `v*` tags, builds on Fedora 41, produces AppImage via linuxdeploy. `NO_STRIP=true` due to Fedora 41 .relr.dyn incompatibility.
