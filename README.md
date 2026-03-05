# Gitpulsar

A lightweight, GNOME-native Git GUI built with Rust, GTK4, and libadwaita.

## Features

- **Multi-repo workspaces** — open a folder and browse all Git repositories inside it
- **3-panel layout** — repository tree | commits/changes | diff view (side-by-side & unified)
- **Staging area** — stage/unstage/discard files individually or all at once
- **Commits** — create commits, browse history, search by summary/author/SHA
- **Remote operations** — fetch, pull (fast-forward), push, force push with confirmation
- **Branch management** — switch branches, checkout remote tracking branches, create new branches
- **Responsive design** — adaptive layout with libadwaita breakpoints for narrow screens
- **Auto-refresh** — staging area and indicators update automatically every 5 seconds
- **Non-blocking UI** — remote and branch operations run in background threads

## Architecture

```
crates/
├── gitpulsar-core/    # Git operations library (git2-rs)
│   ├── repository.rs  # Repo open, status, log, branches, ahead/behind
│   ├── staging.rs     # Stage, unstage, commit, discard
│   ├── remote.rs      # Fetch, pull, push with SSH credentials
│   ├── branch.rs      # Checkout, create branches
│   ├── diff.rs        # Commit, staged, unstaged diffs
│   ├── workspace.rs   # Multi-repo workspace scanning
│   └── models.rs      # Data types (CommitInfo, BranchInfo, DiffFile, etc.)
└── gitpulsar-gtk/     # GTK4 + libadwaita frontend
    ├── app.rs         # Application setup
    ├── main.rs        # Entry point
    └── widgets/
        ├── window.rs       # Main window, layout, actions, async operations
        ├── commit_list.rs  # Commit list rows
        ├── staging_area.rs # Unstaged/staged file lists with action buttons
        ├── diff_view.rs    # Unified and side-by-side diff rendering
        └── repo_tree.rs    # Repository tree with dirty/ahead indicators
```

Core is a standalone library with no UI dependencies, designed for pluggable frontends (GTK first, Qt/macOS/Windows later).

## Requirements

- Rust 1.75+
- GTK 4.12+
- libadwaita 1.4+
- libgit2 (via git2-rs)

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

## Building

```sh
cargo build --release
```

## Running

```sh
cargo run -p gitpulsar-gtk
```

Or after building:

```sh
./target/release/gitpulsar-gtk
```

## License

GPL-3.0-or-later
