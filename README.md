# Gitpulsar

**A fast, native Git client for GNOME.** Written in Rust with GTK4 and libadwaita — small binary, low memory, no telemetry, no cloud, no terminal required.

![Gitpulsar](data/screenshots/main.png)

## Why Gitpulsar

- **Native GNOME experience** — libadwaita widgets, dark/light themes, follows your accent color
- **Multi-repository workspace** — open a folder, see every repo with live status indicators (modified, untracked, ahead of remote)
- **Adaptive layout** — runs from 4K monitors down to 360 px phone screens (Phosh, postmarketOS)
- **Lightweight** — Rust core with background polling that pauses when the window is not focused
- **Privacy-first** — no telemetry, no account, no cloud sync; everything stays local
- **Complete workflow** — staging, hunks, conflict editor, interactive rebase, blame, reflog, patches, remotes, submodules, worktrees, stashes, tags

## Features

- **Clone repository** — clone from URL with a built-in dialog
- **Multi-repo workspace** — open a folder, browse all Git repositories inside
- **Commit history** — searchable list with expandable details, paginated loading
- **Commit detail** — prominent message display, file list with configurable limit, collapsible technical details
- **Branch graph** — visual branch topology in a separate window
- **Staging area** — split unstaged/staged lists with drag-and-drop, per-file and per-hunk stage/unstage/discard
- **Partial staging** — stage/unstage individual hunks within a file
- **Syntax highlighting** — language-aware diff coloring with dark/light theme support
- **Undo/redo** — undo staging operations and file discards (Ctrl+Z / Ctrl+Shift+Z)
- **Branch management** — create, checkout local/remote branches
- **Remote operations** — fetch, pull, push via git CLI (reliable SSH support)
- **Stash** — save, pop, list, drop
- **Commit editing** — amend, edit message, cherry-pick, revert
- **Commit signing** — signed commit indicator (lock icon)
- **Merge & rebase** — conflict detection, continue/abort merge and rebase
- **Interactive rebase** — reorder, squash, fixup, reword, drop commits
- **Submodules** — list, init, update submodules
- **Worktrees** — list, add, remove git worktrees
- **.gitignore editor** — edit .gitignore from the app menu
- **Reflog browser** — recover from accidental reset/rebase via HEAD reflog
- **Remote management** — add/remove/rename remotes and edit URLs via dialog
- **Co-Author helper** — add `Co-Authored-By` trailer with a popover button
- **File history** — per-file commit log via the history button on each file row
- **Tag remote operations** — right-click tag: push to remote, delete from remote, delete locally
- **Patch import/export** — export a commit as a `.patch` file or apply an existing patch (`git am` + `git apply` fallback)
- **Adaptive layout** — 3-tier responsive design: desktop, tablet (<860sp), mobile (<500sp). Minimum 360px width
- **Auto-refresh** — configurable polling with hash-based skip
- **Preferences** — date format, refresh interval, commit files limit

## Usage

### Getting started

Open a workspace folder or a single Git repository:

```sh
gitpulsar-gtk /path/to/projects   # open a folder with multiple repos
gitpulsar-gtk /path/to/repo       # open a single repository
```

Or use **Ctrl+O** inside the app to open a folder. If the folder contains multiple Git repositories, they appear in the left sidebar. Click a repository to select it.

### Repository indicators

Each repository in the sidebar shows colored dots to the right of its name (hover for tooltip):

- **● green** — unpushed commits ahead of remote
- **● yellow** — uncommitted changes in tracked files (modified, staged, deleted)
- **● blue** — only untracked files (new files not yet added)

### Working with changes

1. Switch to the **Changes** tab (Ctrl+2) to see unstaged and staged files in separate lists.
2. Click a file to expand its inline diff with syntax highlighting.
3. Use the **+** button to stage a file, or **drag-and-drop** files between the Unstaged and Staged lists.
4. For multi-hunk files, use **Stage Hunk** buttons to stage individual hunks.
5. Use **Stage All** / **Unstage All** buttons or Ctrl+Shift+S / Ctrl+Shift+U.
6. Type a commit message and press **Commit** (Ctrl+Enter).
7. Mistakes? **Ctrl+Z** undoes staging operations, even file discards.

### Browsing history

The **Commits** tab (Ctrl+1) shows the commit log with expandable details:
- Click a commit to see the full message (prominently displayed), changed files, and collapsible technical details (SHA, parent, author, date).
- Scroll to the bottom and click **Load more commits** for older history.
- Signed commits show a lock icon.

### Remote operations

- **Fetch** (Ctrl+Shift+F), **Pull** (Ctrl+Shift+L), **Push** (Ctrl+Shift+P)
- Force push is available from the hamburger menu.

### Other tools

- **Stash**: Ctrl+Alt+S to save, Ctrl+Alt+P to pop. Manage stashes in the right sidebar.
- **Branches & Tags**: Create, checkout, and search in the right sidebar panel.
- **.gitignore**: Edit from the hamburger menu (Menu → Edit .gitignore).
- **Preferences**: Date format, auto-refresh interval, commit files limit.

## Architecture

```
crates/
├── gitpulsar-core/        # Git operations library (git2-rs + git CLI)
│   ├── repository.rs      # Repo open, status, log (paginated), branches, ahead/behind
│   ├── staging.rs          # Stage, unstage, commit, discard, hunk-level staging
│   ├── remote.rs           # Fetch, pull, push (git2 for local, CLI for remote)
│   ├── branch.rs           # Checkout, create branches
│   ├── diff.rs             # Commit, staged, unstaged diffs
│   ├── stash.rs            # Stash save, pop, list, drop
│   ├── workspace.rs        # Multi-repo workspace scanning (parallel)
│   ├── merge.rs            # Conflict detection, continue/abort merge & rebase
│   ├── rebase.rs           # Interactive rebase via GIT_SEQUENCE_EDITOR
│   ├── submodules.rs       # Submodule list, init, update
│   ├── worktrees.rs        # Worktree list, add, remove
│   ├── gitignore.rs        # Read/write .gitignore
│   └── models.rs           # Data types (CommitInfo, BranchInfo, DiffFile, etc.)
└── gitpulsar-gtk/          # GTK4 + libadwaita frontend
    ├── app.rs              # Application setup + keyboard shortcuts
    ├── main.rs             # Entry point
    ├── config.rs           # Preferences (date format, refresh interval, files limit)
    ├── undo.rs             # Undo/redo stack for staging operations
    └── widgets/
        ├── window.rs              # Main window, layout, actions, state
        ├── commit_list.rs         # Expandable commit rows with new layout
        ├── commit_graph.rs        # Branch graph (cairo rendering)
        ├── changes_view.rs        # Split staged/unstaged lists, DnD, hunk actions
        ├── branches_tags_panel.rs # Right sidebar (branches, remotes, tags, stashes)
        ├── repo_tree.rs           # Repository tree with indicators
        ├── preferences_dialog.rs  # Settings dialog
        ├── gitignore_editor.rs    # .gitignore editor dialog
        └── syntax.rs              # Syntax highlighting via syntect
```

Core is a standalone library with no UI dependencies, designed for pluggable frontends.

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| Ctrl+O | Open workspace/repo |
| Ctrl+Enter | Commit |
| Ctrl+Shift+S | Stage all |
| Ctrl+Shift+U | Unstage all |
| Ctrl+Z | Undo |
| Ctrl+Shift+Z | Redo |
| Ctrl+Shift+F | Fetch |
| Ctrl+Shift+P | Push |
| Ctrl+Shift+L | Pull |
| Ctrl+1 | Show commits |
| Ctrl+2 | Show changes |
| Ctrl+F | Focus search |
| Ctrl+Alt+S | Stash save |
| Ctrl+Alt+P | Stash pop |

## Requirements

- Rust 1.70+
- GTK 4.12+
- libadwaita 1.4+
- git (for remote operations)

### Fedora

```sh
sudo dnf install gtk4-devel libadwaita-devel
```

### Ubuntu / Debian

```sh
sudo apt install libgtk-4-dev libadwaita-1-dev
```

### Arch Linux

```sh
sudo pacman -S gtk4 libadwaita
```

## Install

```sh
make install   # builds release and installs to ~/.local
```

This installs the binary, desktop entry, icon, and metainfo.

## Building

```sh
cargo build --release
```

## Running

```sh
gitpulsar-gtk
```

Or from source:

```sh
cargo run -p gitpulsar-gtk
```

## Uninstall

```sh
make uninstall
```

## Documentation

- [User Guide (English)](docs/guide-en.md)
- [Руководство пользователя (Русский)](docs/guide-ru.md)

## License

GPL-3.0-or-later
