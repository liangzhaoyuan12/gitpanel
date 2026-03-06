# Gitpulsar

A lightweight, GNOME-native Git GUI built with Rust, GTK4, and libadwaita.

![Gitpulsar](data/screenshots/main.png)

## Features

- **Multi-repo workspace** — open a folder, browse all Git repositories inside
- **Commit history** — searchable list with expandable details, file diffs, tags
- **Branch graph** — visual branch topology in a separate window
- **Staging area** — per-file stage/unstage/discard with inline diffs
- **Branch management** — create, checkout local/remote branches
- **Remote operations** — fetch, pull, push via git CLI (reliable SSH support)
- **Stash** — save, pop, list, drop
- **Commit editing** — amend, edit message, cherry-pick, revert
- **Adaptive layout** — responsive 3-panel design for desktop and mobile
- **Auto-refresh** — configurable polling with hash-based skip
- **Preferences** — date format, refresh interval

## Architecture

```
crates/
├── gitpulsar-core/        # Git operations library (git2-rs + git CLI)
│   ├── repository.rs      # Repo open, status, log, branches, ahead/behind
│   ├── staging.rs          # Stage, unstage, commit, discard
│   ├── remote.rs           # Fetch, pull, push (git2 for local, CLI for remote)
│   ├── branch.rs           # Checkout, create branches
│   ├── diff.rs             # Commit, staged, unstaged diffs
│   ├── stash.rs            # Stash save, pop, list, drop
│   ├── workspace.rs        # Multi-repo workspace scanning
│   └── models.rs           # Data types (CommitInfo, BranchInfo, DiffFile, etc.)
└── gitpulsar-gtk/          # GTK4 + libadwaita frontend
    ├── app.rs              # Application setup
    ├── main.rs             # Entry point
    ├── config.rs           # Preferences (date format, refresh interval)
    └── widgets/
        ├── window.rs              # Main window, layout, actions
        ├── commit_list.rs         # Expandable commit rows
        ├── commit_graph.rs        # Branch graph (cairo rendering)
        ├── changes_view.rs        # Staging area with inline diffs
        ├── branches_tags_panel.rs # Right sidebar (branches, remotes, tags)
        ├── repo_tree.rs           # Repository tree with indicators
        └── preferences_dialog.rs  # Settings dialog
```

Core is a standalone library with no UI dependencies, designed for pluggable frontends.

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

## License

GPL-3.0-or-later
