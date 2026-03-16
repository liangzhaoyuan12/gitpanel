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
make help                      # show available targets
```

**System dependencies**: gtk4-devel, libadwaita-devel (Fedora) / libgtk-4-dev, libadwaita-1-dev (Debian).

There are no tests or lints configured in this project currently.

## Architecture

Two-crate workspace:

- **gitpulsar-core** — pure git operations library with zero UI dependencies. Wraps `git2` in a `GitRepo` struct. All methods return `anyhow::Result<T>`. Models derive Serialize/Deserialize.
- **gitpulsar-gtk** — GTK4 + libadwaita frontend. Entry point in `main.rs` → `app.rs` (GtkApplication setup + keyboard shortcuts) → `widgets/window.rs` (main window, holds all app state).

### Core modules (gitpulsar-core/src/)

| Module | Purpose |
|---|---|
| repository | Repo open, status (with recurse control), log (paginated), branches, ahead/behind |
| models | All shared data types (CommitInfo, BranchInfo, DiffFile, ConflictFile, RebaseEntry, etc.) |
| staging | Stage, unstage, commit, discard — file-level and hunk-level (`stage_hunk`/`unstage_hunk` via `git2::apply`) |
| diff | Unstaged/staged/commit diffs, hunk parsing |
| branch | Checkout local/remote, dirty checks |
| remote | fetch/pull/push (git2 + CLI fallback for SSH) |
| stash | Save, pop, list, drop, apply |
| commit_ops | Cherry-pick, revert, reset |
| tags | List, create (lightweight/annotated) |
| blame | Per-file blame |
| workspace | Multi-repo scanning (1-level deep, parallel indicator computation) |
| merge | Conflict detection, list conflicts, mark resolved, continue/abort merge & rebase |
| rebase | Interactive rebase via `GIT_SEQUENCE_EDITOR` trick |
| submodules | List, init, update submodules |
| worktrees | List, add, remove worktrees |
| gitignore | Read/write .gitignore, add patterns |

### Key patterns

- **State management**: `GitpulsarWindow` uses `RefCell`/`Cell` interior mutability (GTK is single-threaded). A background refresh loop polls repo state on a configurable interval (default 15s).
- **Hash-based refresh skipping**: `last_status_hash`, `last_commits_hash`, `last_workspace_hash` — UI updates only when hashes change.
- **Concurrency guard**: `refresh_in_progress` Cell prevents overlapping refreshes.
- **Lazy diffs**: Background refresh only computes diffs when status hash changes.
- **Parallel operations**: Workspace scanning uses up to 8 threads; initial repo load runs log/tags and status/branches in parallel threads; workspace scan and repo status run in parallel during refresh.
- **Paginated commits**: Initial load fetches 50 commits (`COMMIT_PAGE_SIZE`), "Load more" button appends next page.
- **Async diff on expand**: `diff_commit` runs in background thread with spinner, not blocking UI.
- **Remote operations**: git2 for local ops, shelled-out `git` CLI (`run_git_cmd`) for remote ops (push/pull/fetch) due to SSH reliability. 30-second timeout.
- **Widget builders**: Functions return `(gtk::Box, SomeRefs)` tuples — the widget and a struct of handles for later updates.
- **Layout**: Outer AdwOverlaySplitView (repo sidebar | main) → inner split (commit list | changes/graph view).
- **Changes view**: Split into unstaged/staged ListBoxes with drag-and-drop between them.
- **Undo/redo**: `UndoStack` in `undo.rs` tracks staging ops and discards (with saved file content for restore).
- **Syntax highlighting**: `syntect` crate in `syntax.rs`, lazy-loaded SyntaxSet/ThemeSet, theme-aware (dark/light via `adw::StyleManager`).
- **Config**: JSON at `~/.config/dev.gitpulsar/config.json` — date format, refresh interval, commit files limit, recent workspaces.
- **CLI open**: App uses `HANDLES_OPEN` flag — accepts repo path as CLI argument (`gitpulsar-gtk /path/to/repo`).
- **Branch graph**: `commit_graph.rs` renders via cairo, not standard GTK widgets — separate drawing model.
- **Hunk staging**: Builds partial unified-diff patches and applies via `git2::Repository::apply` to index.

## CI

GitLab CI (`.gitlab-ci.yml`): triggers on `v*` tags, builds on Fedora 41, produces AppImage via linuxdeploy. `NO_STRIP=true` due to Fedora 41 .relr.dyn incompatibility.
