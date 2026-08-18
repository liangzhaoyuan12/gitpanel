# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

GitPulsar — GNOME-native Git GUI written in Rust with GTK4/libadwaita. App ID: `io.gitlab.ilshat_apps.gitpulsar`.

## Build & Run

```bash
cargo build --release          # release build
cargo run -p gitpulsar-gtk     # run from source
make install                   # build + install to ~/.local (binary, desktop entry, icon, metainfo)
make uninstall                 # remove installed files
make help                      # show available targets
```

**System dependencies**: gtk4-devel, libadwaita-devel (Fedora) / libgtk-4-dev, libadwaita-1-dev (Debian). The `gtk4` crate is built with the `v4_12` feature — `GtkFileLauncher::set_always_ask` needs it. Do not raise that further without checking the CI image: `.gitlab-ci.yml` builds on Fedora 41, which ships GTK 4.16, so `v4_18` would break the build for no gain.

Quality gate: `cargo build --release && cargo clippy --all-targets -- -D warnings && cargo test`. Core integration tests live in `crates/gitpulsar-core/tests/` (tempfile-backed `GitRepo`). GTK unit tests live inline under `#[cfg(test)] mod tests {…}` in widget modules + a shared `crates/gitpulsar-gtk/src/test_support.rs` (`ensure_gtk_init` + `GP_SKIP_GTK_TESTS=1` headless soft-skip).

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
- **Paginated commits**: Initial load fetches 50 commits (`COMMIT_PAGE_SIZE`); data stored in `gio::ListStore<CommitObject>`; the model grows on Load more without realizing additional widgets.
- **Async diff on expand**: `diff_commit` runs in background thread with spinner, not blocking UI.
- **Remote operations**: git2 for local ops, shelled-out `git` CLI (`run_git_cmd`) for remote ops (push/pull/fetch) due to SSH reliability. 30-second timeout.
- **Widget builders**: Functions return `(gtk::Box, SomeRefs)` tuples — the widget and a struct of handles for later updates.
- **Layout**: Outer `AdwOverlaySplitView` (repo sidebar | main) → inner `AdwOverlaySplitView` (content | right sidebar at PackType::End). OverlaySplitView is the GNOME HIG pick for utility-pane sidebars — on collapse the sidebar slides over content with a built-in edge-swipe gesture, while content stays full-width. Breakpoint setters keep `show-sidebar` off on collapse so the sidebar is opened only by toggle button or swipe.
- **Changes view**: Single unified `gtk::ListView` + `SignalListItemFactory` backed by a `gio::ListStore` of `ChangedFileObject` (custom GObject wrapping path/status/is_staged/expanded). Only viewport rows are realized. `populate_file_lists` splices the store. Per-row click handlers wired once in the factory's setup callback; they walk up to the `gp-file-row`-marked outer Box to read the current item's path.
- **Commits view**: same pattern as Changes view — `gtk::ListView` + `SignalListItemFactory` + `gio::ListStore<CommitObject>`. Per-row UI state (expanded, files_loaded, tags) lives on the `CommitObject` so virtualization can recycle row widgets without losing state. A sentinel `CommitObject` marked `is_load_more_sentinel` renders the trailing "Load more" row; the factory branches on the flag. Search is a `gtk::CustomFilter` on a `gtk::FilterListModel` wrapping the store.
- **Row state never goes through `items_changed`**: `GtkListView` skips the re-bind when the object at a position is unchanged, so emitting `store.items_changed` after mutating a `CommitObject` in place does nothing. Push state onto the realized row instead — `commit_list::find_row_outer` + `rebind_row` (see `rebind_commit_row` in window.rs). State still lives on the object so recycled rows restore it on bind.
- **Commit file diffs**: clicking a file inside an expanded commit opens `commit_diff_dialog` — `adw::Dialog` + `AdwOverlaySplitView` (file list | `gtk::Stack` of side-by-side / unified). Rendering lives in `diff_view.rs`, whose palette is theme-aware via `diff_palette(dark)`; `setup_tags` restyles existing tags so a theme flip reaches open buffers. Below 700sp the dialog collapses the file list and switches to unified. Diff panes do not wrap, and `sync_scroll` links both axes so the sides stay aligned. `adw::Dialog` needs an explicit `width-request`/`height-request` or it warns and will not size down on a phone.
- **Config cells for row factories**: `commit_date_format` and `commit_files_limit` live on the window as `Rc<Cell<_>>` shared with the row factory, and are updated when preferences are saved — a cell captured once at startup silently ignores later preference changes.
- **Undo/redo**: `UndoStack` in `undo.rs` tracks staging ops and discards (with saved file content for restore).
- **Syntax highlighting**: `syntect` crate in `syntax.rs`, lazy-loaded SyntaxSet/ThemeSet, theme-aware (dark/light via `adw::StyleManager`).
- **Config**: JSON at `~/.config/io.gitlab.ilshat_apps/config.json` — date format, refresh interval, commit files limit, recent workspaces, external editor.
- **External editor** (`external_editor.rs`): "Open in <editor>" (`win.open-in-editor`, Ctrl+Shift+O), with **two different implementations depending on the sandbox**.
  - *Unsandboxed* (distro package, source build, AppImage): the module detects installed editors with a single `sh -c` probe for the whole catalogue — not one per command — on a worker thread, refilling the preferences combo on arrival, then spawns the chosen command. Flatpak app IDs work as commands (`/var/lib/flatpak/exports/bin`). `{path}` in a custom command is substituted, otherwise the path is appended. Pure parts (`split_command`, `build_argv`, `match_detected`, `label_for_command`) are unit-tested.
  - *Under Flatpak*: `GitpulsarWindow::open_with_portal` hands the folder to `gtk::FileLauncher` with `always_ask(true)` and the host chooser picks the application. `detect_installed` returns empty and `launch` errors out as a backstop. Preferences shows an explanatory row instead of the picker, and preserves the stored command rather than overwriting it with "Not configured" — the same config file may be shared with a non-Flatpak install. `GP_SIMULATE_FLATPAK=1` forces this branch on a normal desktop; it is the only way to exercise it while developing.
  - **`flatpak-spawn --host` is a dead end — do not retry it.** It needs `--talk-name=org.freedesktop.Flatpak`, which Flathub's linter rejects as `finish-args-flatpak-spawn-access`; v1.3.0's Flathub build failed on exactly that check and the PR had to be withdrawn. An exception would have to be granted by hand in `flathub-infra/flatpak-builder-lint`, and it is not worth requesting: the portal covers the common cases, which is the first thing reviewers there ask about.
  - **Known limit of the portal, do not treat it as a bug:** its chooser only lists applications that register as `inode/directory` handlers. Checked against Flathub's appstream `provides` field: VS Code, VSCodium, Kate, IntelliJ IDEA and Android Studio register; Zed, GNOME Builder, Qt Creator, Emacs and GNOME Text Editor do not. Zed ships the MimeType line commented out on purpose, noting it made Zed the system's default file browser. Nothing on our side can change that. Verify with `curl -s https://flathub.org/api/v2/appstream/<app-id> | jq .provides` — note the `mimetypes` field is null and misleading.
- **Workspace root is stored, never derived**: `imp.workspace_root` is set in `open_workspace` from the folder the user actually picked. It used to be recomputed each background tick as `entries[0].path.parent()`, which meant opening a single repository silently widened the workspace to its parent and pulled in every sibling repo (issue #2).

- **CLI open**: App uses `HANDLES_OPEN` flag — accepts repo path as CLI argument (`gitpulsar-gtk /path/to/repo`).
- **Branch graph**: `commit_graph.rs` renders via cairo, not standard GTK widgets — separate drawing model.
- **Hunk staging**: Builds partial unified-diff patches and applies via `git2::Repository::apply` to index.
- **Adaptive layout**: 4-tier `AdwBreakpoint` system — compact (<1080sp, inner split collapsed), tablet (<860sp), narrow (<600sp), mobile (<500sp). Uses `sp` units for Large Text scaling. Min window 360x294. Each breakpoint also toggles a CSS class on the window (`gp-compact` / `gp-collapsed` / `gp-narrow` / `gp-mobile`) — CSS rules use these to lift row heights to 48 px on mobile and collapse `AdwViewSwitcherBar` labels to icon-only at <600 sp (the bar's `reveal` property, not `visible`, is the animated show/hide knob).
- **Mobile remote ops**: on `<600sp` the bottom-bar fetch/pull/push buttons hide and a "Sync" `gtk::MenuButton` in the content header (icon `vertical-arrows-none-symbolic` from the GNOME Icon Library, shipped under `data/icons/hicolor/scalable/actions/`) pops up the three operations.
- **Right sidebar overlay UX**: when the inner split collapses, the right header gains a "Close panel" button at the start so users dismiss the overlay instead of closing the window via the X — the window-close X stays visible too.

## Known cosmetic warning

Scrolling in the commit diff dialog prints `Trying to snapshot GtkGizmo … without a current allocation` (sometimes naming `GtkScrolledWindow` instead). Rendering is unaffected; the message only shows in a terminal.

Investigated 2026-08-05 and **not** root-caused. What is established:

- The parent chain is the dialog's own panes: `GtkGizmo(trough)` → `GtkScrollbar` → `GtkScrolledWindow` → `GtkBox` (split page) → `GtkStack` → `AdwOverlaySplitView` → `AdwToolbarView` → `AdwBreakpointBin` → `AdwDialog`. Not the main window's `AdwViewStack`.
- The whole backtrace is inside GTK (`gtk_scrolled_window_snapshot` → `gtk_widget_snapshot_child`); no frame of ours appears.
- It needs live pointer/scroll input. Eight scripted scenarios (open dialog, toggle view, switch file, close, narrow to the breakpoint, programmatic scrolling, expand a commit, signature batch) never reproduced it, so it cannot be caught in a test. Wayland rules out synthesising input with `xdotool`.

Three fixes were tried and **refuted** — do not retry them:

1. Sharing one `GtkAdjustment` per axis between the panes instead of copying values in `value_changed`.
2. `overlay_scrolling(false)` on the panes (also makes the scrollbars unpleasantly wide).
3. Deferring the buffer fill to `glib::idle_add_local_once` instead of rendering straight from the row-selected handler.

To reproduce and inspect it again: catch the message with `glib::log_set_writer_func` (GTK sends it through structured logging, so `log_set_default_handler` never sees it), parse the widget address out of the text, and walk `parent()` printing `type_()`, `widget_name()`, `css_name()` and `allocation()`.

## CI

GitLab CI (`.gitlab-ci.yml`):
- `test` — runs on merge requests and pushes to the default branch: `cargo build`, `cargo clippy --all-targets -- -D warnings`, then `xvfb-run -a cargo test` (widget tests need a display; `GP_SKIP_GTK_TESTS=1` would soft-skip them and silently drop coverage).
- `appimage` + `release` — trigger on `v*` tags, build on Fedora 41, produce an AppImage via linuxdeploy. `NO_STRIP=true` due to Fedora 41 .relr.dyn incompatibility.

Release notes auto-extracted from `CHANGELOG.md` via `sed -n` (not `awk` — cascades; not `head -n -1` — BusyBox incompatible).

## Release Process

1. Update version in `Cargo.toml` (workspace level)
2. Add section to `CHANGELOG.md` (`## vX.Y.Z (date)`)
3. Add release entry to `data/io.gitlab.ilshat_apps.gitpulsar.metainfo.xml`
4. About dialog version auto-reads from `env!("CARGO_PKG_VERSION")`
5. `git tag vX.Y.Z && git push origin vX.Y.Z` — CI builds AppImage + creates release with notes from CHANGELOG

## Gotchas

- **Icons**: Many symbolic icons (tag-symbolic, emblem-ok-symbolic) don't exist in standard Adwaita. Verify with `Gtk.IconTheme.has_icon()`. Custom icons in `data/icons/hicolor/scalable/actions/`, installed via Makefile.
- **RefCell borrows**: Can't store `&T` from `RefCell::borrow()` in a Vec that outlives the borrow scope. Clone the Option first: `let x = imp.field.borrow().clone();`
- **Revealer vs set_visible**: Use `gtk::Revealer` for animated show/hide. `find_child_by_name` must recurse into Revealers (not just Boxes).
- **Performance**: Never compute `commit_graph::compute_graph` on UI thread — use background thread. Use `Rc` for shared data across DrawingArea closures, not `.to_vec()` per row.
- **Alpine/BusyBox**: CI release image is Alpine-based. `head -n -1` and `tail -n +2` may not work. Use `sed` instead.
- **gio::Menu icons**: `MenuItem::set_attribute_value("icon", ...)` doesn't render in libadwaita PopoverMenu.
- **status(bool)**: `repo.status(true)` for full recursive untracked scan, `repo.status(false)` for fast background polling.
