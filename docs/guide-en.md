# Gitpulsar User Guide

Gitpulsar is a GNOME-native Git GUI built with Rust, GTK4, and libadwaita.

## Installation

### Requirements

- GTK 4.12+
- libadwaita 1.4+
- git (for remote operations)

#### Fedora

```sh
sudo dnf install gtk4-devel libadwaita-devel
```

#### Ubuntu / Debian

```sh
sudo apt install libgtk-4-dev libadwaita-1-dev
```

#### Arch Linux

```sh
sudo pacman -S gtk4 libadwaita
```

### Build & Install

```sh
git clone https://gitlab.com/ilshat.ishdavletov/gitpulsar.git
cd gitpulsar
make install
```

The binary, desktop entry, icon, and metainfo are installed to `~/.local`. Make sure `~/.local/bin` is in your `PATH`.

### Uninstall

```sh
make uninstall
```

## Launch

```sh
gitpulsar-gtk                      # open the app
gitpulsar-gtk /path/to/projects    # open a folder with multiple repos
gitpulsar-gtk /path/to/repo        # open a single repository
```

Or use **Ctrl+O** inside the app.

## Interface

The app has three main panels:

```
┌─────────────┬──────────────────────────┬──────────────────┐
│  Sidebar    │  Center area             │  Right panel     │
│             │                          │                  │
│  Repo list  │  Commits / Changes       │  Branches        │
│             │  (switch: Ctrl+1/2)      │  Tags            │
│             │                          │  Stashes         │
└─────────────┴──────────────────────────┴──────────────────┘
```

- **Sidebar** (left) — repository list. Yellow dot = uncommitted changes, green arrow = unpushed commits.
- **Center** — switches between Commits and Changes tabs.
- **Right panel** — branches (local/remote), tags, stashes. Search filters all lists.

## Working with Changes

### Viewing changes

Switch to the **Changes** tab (Ctrl+2). Files are split into two lists:

- **Unstaged Changes** — working tree modifications not yet in the index.
- **Staged Changes** — changes ready to commit.

Click a file to expand its inline diff with syntax highlighting. Colors adapt to light/dark theme.

### Staging files

Several ways to stage a file:

1. **+ button** — hover over a file in Unstaged, click "+".
2. **Drag-and-drop** — drag a file from Unstaged to Staged.
3. **Stage All** (Ctrl+Shift+S).

To unstage:
1. **− button** on a file in Staged.
2. Drag from Staged to Unstaged.
3. **Unstage All** (Ctrl+Shift+U).

### Partial staging (hunk-level)

If a file contains multiple changed sections (hunks), you can stage them individually:

1. Expand the file's diff by clicking on it.
2. If there's more than one hunk, **Stage Hunk 1**, **Stage Hunk 2** buttons appear above the diff.
3. Click the desired button — only that hunk is staged.

This is useful when a file contains both finished work and work-in-progress.

### Discarding changes

Hover over a file in Unstaged and click the trash button. The app asks for confirmation, then restores the file from the index.

### Undo / Redo

- **Ctrl+Z** — undo the last staging operation (stage, unstage, discard).
- **Ctrl+Shift+Z** — redo.

Discards can also be undone — the app saves file content before discarding.

### Committing

1. Type a commit message in the text field at the top of the Changes tab.
2. Click **Commit** or press Ctrl+Enter.
3. Check "Amend" to amend the previous commit.

## Browsing Commit History

The **Commits** tab (Ctrl+1) shows the commit log:

- Each row: hash, message, author, time, tags.
- Green dot = commit not yet pushed.
- Lock icon = signed commit (GPG/SSH).

### Expanding a commit

Click a commit to see:

1. **Commit message** in large font — summary and body.
2. **File list** — limited to 10 by default (configurable). "Show all" expander for more.
3. **Details** (collapsed by default) — SHA, parent, author, date.

Files load asynchronously — the UI doesn't freeze even for large commits.

### Pagination

The first 50 commits are loaded on open. Click **Load more commits** at the bottom to fetch the next page.

### Search

Use the search bar (Ctrl+F) to filter commits by message text.

## Remote Operations

| Action | Shortcut |
|---|---|
| Fetch | Ctrl+Shift+F |
| Pull | Ctrl+Shift+L |
| Push | Ctrl+Shift+P |

Force push is available via the menu (☰ → Force Push).

Remote operations use git CLI (not git2) for reliable SSH key and configuration support.

## Branches & Tags

The right panel contains:

- **Local branches** — current branch is highlighted. "+" button to create a new branch.
- **Remote branches** — double-click to checkout.
- **Tags** — list of tags.
- **Stashes** — with Apply and Drop buttons.

The search bar at the top filters all four lists.

## Stash

| Action | Shortcut |
|---|---|
| Stash save | Ctrl+Alt+S |
| Stash pop | Ctrl+Alt+P |

Stash management (apply, drop) is also available in the right panel.

## .gitignore

Menu ☰ → **Edit .gitignore** opens a text editor with the current `.gitignore` content. Saving automatically refreshes file status.

## Preferences

Menu ☰ → **Preferences**:

- **Date Format** — European (dd.MM.yyyy), ISO (yyyy-MM-dd), American (MM/dd/yyyy).
- **Auto-refresh interval** — polling interval in seconds (0 = disabled, default 15).
- **Commit files limit** — max files shown in expanded commit (0 = unlimited, default 10).
- **Refresh Now** — force an immediate refresh.

## Multi-Repository Workspace

Open a folder containing multiple Git repositories. Gitpulsar scans one level deep and displays them in the sidebar with indicators:

- Repository name
- Current branch
- Yellow dot = has changes
- Green arrow = has unpushed commits

Scanning runs in parallel (up to 8 threads), which matters for large workspaces.

## All Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| Ctrl+O | Open folder/repo |
| Ctrl+Enter | Commit |
| Ctrl+Shift+S | Stage all |
| Ctrl+Shift+U | Unstage all |
| Ctrl+Z | Undo |
| Ctrl+Shift+Z | Redo |
| Ctrl+Shift+F | Fetch |
| Ctrl+Shift+P | Push |
| Ctrl+Shift+L | Pull |
| Ctrl+1 | Commits tab |
| Ctrl+2 | Changes tab |
| Ctrl+F | Focus search |
| Ctrl+Alt+S | Stash save |
| Ctrl+Alt+P | Stash pop |
