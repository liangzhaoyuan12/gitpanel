# Changelog

## v1.3.3 (2026-09-04)

### Fixed

- **Clicking "Show all" under Local branches tried to check out a branch named
  `show-more-row`.** The row is synthetic, but the checkout handler read a
  branch name off whatever row was activated. Reported by @rachaalaraj.
- **Right-clicking "Show all" offered branch and tag operations** on that same
  non-existent name — Checkout, Merge, Rename, Delete. Both context menus read
  the row under the cursor directly; every path now goes through one filter.
  Reported by @rachaalaraj.
- **"Show all" stopped working after a while.** The row limit was re-applied on
  every background refresh and connected a fresh handler each time, so the
  accumulated handlers toggled the rows once each and cancelled out.
- **`gitpulsar-gtk /path/to/repo` now opens that repository.** It never did:
  the application declares `HANDLES_OPEN`, which makes GApplication emit `open`
  instead of `activate`, and no `open` handler had ever been connected — so the
  window was not created at all. The path is also no longer overwritten a
  moment later by the deferred "restore last workspace".

## v1.3.2 (2026-08-18)

### Added

- **"Open With…" is back in the Flatpak build**, going through the desktop
  portal so the host chooses the application. v1.3.1 hid the feature there on
  the assumption that the portal reached no editors; that was wrong. VS Code,
  VSCodium, Kate, IntelliJ IDEA and Android Studio register as handlers for
  `inode/directory` and do appear in the chooser. Zed, GNOME Builder, Qt
  Creator and Emacs do not register and will not appear — that is set by each
  application's own desktop entry. Outside Flatpak nothing changes: the editor
  chosen in Preferences is still launched directly.

## v1.3.1 (2026-08-18)

### Changed

- **"Open in your editor" is hidden in the Flatpak build.** Flathub does not
  permit `--talk-name=org.freedesktop.Flatpak`, without which the sandbox can
  neither detect nor start an application on the host, so v1.3.0 never built
  for Flathub. The menu entry and the preference are now omitted there rather
  than offered in a form that cannot work. The desktop portal is not a
  substitute: it only lists applications that register as handlers for
  `inode/directory`, which most editors do not — Zed ships that line commented
  out on purpose.
- Everywhere else — distribution packages, source builds, the AppImage — the
  feature is unchanged.

## v1.3.0 (2026-08-18)

### Added

- **Open the repository in your editor.** Pick an editor in Preferences →
  External Tools and the menu gains an "Open in …" entry, bound to
  Ctrl+Shift+O. Installed editors are detected automatically — VS Code,
  VSCodium, Cursor, Windsurf, Zed, GNOME Builder, Kate, KDevelop, Sublime Text,
  Qt Creator, Emacs, Android Studio and the JetBrains IDEs — and anything else
  can be entered as a custom command, with an optional `{path}` placeholder.
  Under Flatpak the editor is launched on the host.
- `CONTRIBUTING.md`, and a CI job that runs build, clippy and the test suite on
  every merge request.

### Changed

- **The primary menu is grouped into sections.** Fourteen entries in one flat
  list became four short blocks, with everything rarely reached moved under a
  `Tools` submenu. Stash moved out of `Remote`, where it never belonged — it is
  a local operation.

### Fixed

- **Opening a single repository no longer pulls in its siblings.** The
  background refresh recomputed the workspace root from the first entry's
  parent directory, so a few seconds after opening `~/Projects/MyRepo` every
  other repository in `~/Projects` appeared in the sidebar. Reported by
  @kmwallio.
- **The commit message field no longer swallows clicks** on its "Commit
  message" placeholder, which also covered the whole field rather than its
  top-left corner. Thanks to [Iyaan Azeez](https://gitlab.com/gxhamster).

## v1.2.0 (2026-08-05)

### Added

- **Commit diff dialog.** Clicking a file inside an expanded commit opens a
  dialog showing that file side-by-side — old on the left, new on the right,
  scrolling together on both axes — with a hideable list of the commit's files.
  Below 700sp the list collapses to an overlay and the diff falls back to the
  unified view. Replaces the inline diff that expanded inside the commit row.

### Fixed

- **Clicking a commit works again.** After the v1.1.0 list migration, commit
  rows only responded to a double-click, and even then the expanded detail
  never appeared.
- **The window-close button no longer disappears** with the Branches & Tags
  panel, and the hamburger menu no longer disappears with the repository
  sidebar. Both, plus the open-workspace button, move to whichever header bar
  is on screen.
- **The commit-files limit works again.** The preference had no effect since
  the list migration, so a commit row rendered every file it touched.
- **Changing the date format applies immediately** instead of waiting for a
  restart.
- **Signed-commit locks appear on their own**, filled in by a background
  lookup after each page renders, rather than only after expanding a commit.
- **Staging buttons on recycled rows.** A scrolled-away row could keep the
  previous file's Stage/Unstage action.
- Diff colours follow the dark theme.

## v1.1.0 (2026-05-29)

### Performance

- **Commits tab virtualized.** Replaced the `gtk::ListBox` Commits list with a
  `gtk::ListView` + `gio::ListStore<CommitObject>` backed by a
  `SignalListItemFactory`. After many "Load more" pages, the list now holds
  data rather than realized widgets — scrolling and refreshing stay smooth
  on repos with thousands of commits.
- **Lazy GPG signature lookup.** `repository::log_page` no longer calls
  `extract_signature` for every commit; instead, signature state is fetched
  on-demand when a commit row is expanded. Cuts the bulk of per-page cost on
  large logs.
- **Graph tab early-exit.** Switching to the Graph tab no longer recomputes
  and re-renders when the commit set hasn't changed.
- **Graph tab labels capped at 500.** Long histories rendered thousands of
  `GtkLabel` widgets in a single Box; we now cap at 500 with a footer that
  points to "Load more" on the Commits tab.

### Notes

Addresses the GNOME Software user report about sluggishness on projects
with more than 2000 commits.

## v1.0.0 (2026-05-17)

First stable release. Focus is performance on huge repositories and a properly adaptive UI down to 360 px phone widths.

### Performance

- **Virtualized changes list.** The Changes tab now uses `GtkListView` + `SignalListItemFactory` backed by a `GioListStore` of `ChangedFileObject` GObjects. Per-row widgets are realized only for items in the viewport. On a 940-file `git status`, initial render drops from a multi-second freeze to instantaneous.
- **Diff cache cap.** Cached unstaged/staged diffs are capped at ~5 MB total each. Trailing entries that push the cache over the limit are dropped so memory stays bounded on huge repos.
- **Adaptive background refresh.** Default refresh interval bumped from 15 s to 30 s. The workspace scan throttles from every 4 ticks to every 16 once the workspace hash has been stable for 3 consecutive scans (idle backoff).
- **Single hover controller.** The per-row `EventControllerMotion` is replaced with one ListView-level controller that toggles the row-actions box under the pointer.

### Mobile / adaptive layout

- **Sidebars overlay on collapse.** Both the repo-sidebar split and the branches-sidebar split continue to use `AdwOverlaySplitView` (the GNOME HIG sidebar pattern for utility panes). On collapse the sidebars are hidden by default — content takes the full width — and a built-in edge-swipe gesture brings each side back. Tap the corresponding toggle button in the header to pin/unpin.
- **Native compact view switcher.** Custom `ToggleButton` row replaced with `AdwViewSwitcherBar` at the bottom on narrow widths — fewer signal connections, automatic sync with the view stack.
- **Adaptive file dialogs.** All four `gtk::FileChooserDialog` uses (open workspace, save patch, save archive, apply patch, save graph PNG) migrated to `gtk::FileDialog` — modern adaptive sheets on mobile.
- **Conflict / bisect banner wraps.** Banner action buttons (Good / Bad / Skip / Reset, Continue / Abort) live in a `FlowBox` and wrap onto a second row on narrow widths.
- **Touch targets.** Per-row stage / unstage / discard / blame / history buttons bumped to 36×36 px. On mobile, file rows clamp to a 48 px minimum height.
- **Bottom bar collapses.** Stash hidden at <600 sp. Fetch / pull / push moved to a new "Sync" `MenuButton` in the header (icon: `vertical-arrows-none-symbolic`) that pops up the three operations on tap — also reachable through the hamburger "Remote" submenu.
- **Right sidebar close button.** A dedicated "Close panel" button at the start of the right header bar appears on collapse so users don't reach for the window-close X by accident.
- **ViewSwitcherBar polish.** Switching the bar's `reveal` (not just `visible`) so it actually animates in on narrow widths. On <600 sp the bar drops to icon-only via a CSS rule (`.gp-narrow viewswitcherbar button label { font-size: 0 }`) with a tightened vertical footprint.
- **Commit action row collapses.** Conventional-commit template and co-author trailer buttons hidden at <600 sp. Amend and allow-empty checkboxes hidden at <500 sp.

### Other

- `gtk4` feature bumped to `v4_10` (needed for `gtk::FileDialog`).
- New `bp_narrow` `AdwBreakpoint` tier at <600 sp; each tier now also toggles a CSS class on the window (`gp-compact` / `gp-collapsed` / `gp-narrow` / `gp-mobile`) so CSS-driven rules and toggle handlers can react.
- `vertical-arrows-none-symbolic.svg` shipped under `data/icons/hicolor/scalable/actions/` (sourced from the GNOME Icon Library — not part of the standard Adwaita icon theme).
- Pre-existing clippy lints cleaned up so `cargo clippy -- -D warnings` is green.

## v0.10.0 (2026-05-14)

### Bug fixes

- **Per-file Stage/Unstage/Discard/Blame/History buttons now work.** They used to be wired via a `GestureClick` on the parent `ListBox`, but GTK4 `Button` widgets claim the click sequence so the listbox controller never fired. Buttons are now wired with direct `connect_clicked` after each populate.
- **Refuse empty commits by default.** Committing with nothing staged was silently allowed and could leave the repository in a confusing state (and made the subsequent Reset flow unstable). The commit bar now has an "Allow empty" checkbox that the user must explicitly tick.

### New features

- **Export commit as Archive** — right-click any commit → "Export as Archive…". Writes a `tar.gz`, `tar`, or `zip` snapshot via `git archive` (format inferred from chosen file extension).
- **Export branch graph as PNG** — "Export Graph as PNG…" in the hamburger menu. Renders the full graph to a cairo `ImageSurface` with commit short SHA and summary alongside each row.
- **Bisect UI** — "Start Bisect…" in the hamburger menu lets you pick a bad and a good ref. While bisecting, the top banner shows the current commit and Good / Bad / Skip / Reset buttons. Drives `git bisect` under the hood.

### UX

- **Unified changes list.** Replaced the top/bottom split (Unstaged / Staged) with a single list sorted staged-first, so you don't have to scroll past hundreds of files to reach the staged section.
- File row status icon (left of the filename) now switches to a green checkmark whenever the file is staged, so you can spot staged entries at a glance.
- Staged file paths render in bold (Pango weight) so they don't blend with the unstaged rows above them.
- Section header: `Changes — N staged / M unstaged` with live counts.
- Right sidebar header gets the "Branches & Tags" title and the "New branch" button is now icon + ellipsizable label so it survives narrow widths.
- New `Compact` breakpoint at `<1080sp`: when the window is narrower than ~1080 px the right sidebar collapses to an overlay instead of stealing horizontal space.
- `GP_WIDTH` / `GP_HEIGHT` env vars override the default 1200×800 window size (useful for screenshotting at a specific resolution, e.g. `GP_WIDTH=1000 GP_HEIGHT=700 gitpulsar-gtk`).
- `Ctrl+Q` now quits the app.
- Removed the ▲/▼ ahead/behind counters from the bottom bar — the repo sidebar dots already convey that.

### Performance

- Removed the `GestureClick` controllers on the changes list; per-row buttons now use plain signal handlers, avoiding redundant pick/walk work on every click.
- Per-row `connect_clicked` is wired at row build time — eliminates a post-populate walk that hit `rows × buttons` widget traversals on large repos.
- `TextView` for each file row is built lazily on first expand instead of eagerly for every row.
- Background refresh now uses a cheap `status(false)` probe to detect change and only escalates to the full recursive scan when the probe hash actually differs. Cuts steady-state cost in repos with hundreds of untracked files.

### Fixes (internal)

- Silenced `gtk_text_tag_set_priority` GLib criticals in the diff viewer — syntax-highlight tags were calling `set_priority` before being added to the tag table.

## v0.9.0 (2026-05-05)

### New features

- **Branch compare** — diff between any two refs (branches, tags, commits) via "Compare Branches…" in the hamburger menu. Shows the file list with line counts.
- **Restore file from commit** — restore button next to each entry in the file history dialog. Confirms before overwriting working tree.
- **Word-level diff** — when a deletion immediately precedes a matching-count addition, only the changed tokens get emphasized within the line, like GitHub's intra-line highlighting. Computed lazily per hunk; falls back to plain line highlighting when lines share too few tokens.

### Performance

- Word-level diff uses a custom in-process LCS over tokenized lines, capped at 256 tokens per side to keep render time below a millisecond per pair. No new dependencies.

## v0.8.1 (2026-04-29)

### Improvements

- Rewrote Flathub description to highlight the strong points: native GNOME, Rust, multi-repo workspace, adaptive layout (down to 360 px), no telemetry, full Git workflow
- New summary line: "Fast native Git client for GNOME"
- Added `<categories>` and `<keywords>` to AppStream metainfo for better Flathub search and discovery
- README intro rewritten with a "Why Gitpulsar" section

## v0.8.0 (2026-04-29)

### New features

- **Reflog browser** — view recent HEAD movements (reset/rebase/checkout history) via "Reflog" in the hamburger menu. Acts as a safety net for recovering lost commits.
- **Remote management** — add, remove, rename remotes and edit URLs in a dedicated dialog ("Manage Remotes…" in the hamburger menu).
- **Co-Author helper** — new button next to the conventional-commit prefix picker. Opens a popover to add a `Co-Authored-By: Name <email>` trailer to the commit message.

### Improvements

- Repo indicators now use a custom CSS provider with explicit colors (yellow/blue/green) — fully immune to user's GNOME accent color preference.

## v0.7.1 (2026-04-15)

### Fixes

- Untracked indicator now uses explicit blue (#3584e4) instead of the `accent` CSS class. Fixes the bug where untracked dot rendered green for users whose GNOME accent color is set to green
- Tracked-changes indicator switched to explicit yellow (#e5a50a) for the same reason

## v0.7.0 (2026-04-13)

### New features

- **Clone repository** — clone from URL via hamburger menu, opens cloned repo automatically
- **File history** — per-file commit history via new button in file row (document-open-recent icon)
- **Tag remote operations** — right-click on tag: Push to remote, Delete from remote, Delete locally
- **Patch workflow** — Export commit as `.patch` (commit context menu) and Apply Patch… from hamburger menu (`git am` with fallback to `git apply`)

### Fixes

- Repository dirty indicator now includes untracked files (was missing for new files like `.claude/`)
- Active repo indicator refreshes immediately on status change instead of waiting for full workspace scan

### Improvements

- Repository indicators now color-coded by change type:
  - Green dot — unpushed commits ahead of remote
  - Yellow dot — uncommitted changes in tracked files
  - Blue dot — only untracked files
- Hover tooltips on each indicator explain its meaning
- Unified dot glyph for all indicators (green ●N instead of ▲N)

## v0.6.0 (2026-04-09)

### Improvements

- Skip background refresh when window is not active (saves 10-15% CPU when on another desktop)
- Auto-refresh on window focus regain — instant update when switching back
- Throttle workspace scan to every 4th tick (~60s) instead of every tick
- Cache ahead/behind counts to avoid redundant UI redraws
- Icon canvas footprint adjusted (8px margin) for Flathub guidelines

## v0.5.4 (2026-04-01)

### Improvements

- Migrated all dialogs to AdwDialog API (adaptive: floating on desktop, bottom sheet on mobile)
- AboutWindow → AboutDialog, PreferencesWindow → PreferencesDialog, MessageDialog → AlertDialog
- Blame, conflict editor, rebase editor, gitignore editor now use AdwDialog
- Enabled libadwaita v1_5 features
- Mobile: hide title and non-essential header buttons at <500sp, keep sidebar toggles visible
- Reduced split view min sidebar widths for narrow screens

## v0.5.3 (2026-03-31)

### Improvements

- Redesigned icon: GNOME HIG squircle shape, pulsar rays, better dark background contrast
- Wider and centered git branch symbol within safe zone

## v0.5.2 (2026-03-25)

### Improvements

- Rewritten Flathub description (prose instead of bullet list)
- Screenshot URLs pinned to version tag
- Updated branding colors
- Consolidated release history in metainfo
- Improved desktop entry keywords

## v0.5.1 (2026-03-25)

### Improvements

- Redesigned app icon — simplified, fits GNOME HIG safe zone
- Updated screenshots to 1000x700 (Flathub quality guidelines)

## v0.5.0 (2026-03-25)

### New Features

- **Mobile-adaptive layout** — 3-tier responsive design: tablet (<860sp), narrow (<600sp), mobile (<500sp)
- **Blame view** — per-line annotations (author, date, commit) with theme-aware colors
- **Inline diff in commit files** — click a file in commit detail to see full diff
- **Integrated graph tab** — branch graph as ViewStack tab (Ctrl+3), computed in background
- **Custom GNOME icons** — branch-fork, commit, tag-outline, git, branch-compare

### Improvements

- Minimum window size 360x294 (GNOME HIG phone portrait)
- Breakpoints use sp units (scale with Large Text accessibility)
- Graph toggle in compact switcher for narrow screens
- Colored status icons replace text badges
- Slide animations for commit detail, diff accordion, sidebar sections
- Fixed unpushed indicators not clearing after push
- Git CLI bundled in Flatpak (fixes push/pull/fetch in sandbox)
- SSH auth support in Flatpak
- Redesigned app icon (GNOME HIG compliant)

## v0.4.3 (2026-03-19)

### Bug Fixes

- Fixed unpushed commit indicators not clearing after push
- Fixed sidebar repo indicators not updating after push/pull/fetch
- Force commit list rebuild after remote operations to reflect pushed state

## v0.4.2 (2026-03-17)

### New Features

- **Inline diff in commit files** — click a file in expanded commit to see full diff with syntax highlighting
- **Colored status icons** — replaced text badges (A/M/D) with symbolic icons in both changes view and commit detail
- **Slide animations** — smooth Revealer transitions for commit detail, diff accordion, and sidebar sections
- **Custom GNOME icons** — branch-fork, commit, tag-outline, git, branch-compare, pull-request from Icon Library
- **"Gitpulsar" title** in content header bar

### Improvements

- Graph tab refreshes when switching repos while on graph view
- Replaced all missing icons (emblem-ok, tag) with available alternatives
- Git repos in sidebar use `git-symbolic` icon
- Branch rows show `branch-fork-symbolic` icon (green for HEAD)
- Tag rows use `tag-outline-symbolic`
- Changes tab uses `branch-compare-symbolic`
- Removed separate graph button from header (accessible via tab)
- Removed "Workspace" label from sidebar
- Open folder button no longer hidden on narrow windows
- Minimum window size set to 800x500 for proper desktop layout
- Icons installed via Makefile to system icon theme

## v0.4.1 (2026-03-16)

### New Features

- **Blame view** — hover a file in changes view, click "Blame" to see per-line annotations (author, date, commit) with theme-aware colors
- **Graph tab** — branch graph moved from separate window to a third ViewStack tab (Ctrl+3), computed in background with spinner
- Graph button in header bar now switches to the Graph tab

### Improvements

- Removed separate graph window — graph is now integrated into the main UI
- Graph computed lazily on tab switch (no performance impact on repo load)
- Fixed hang when switching repos (graph no longer computed automatically)

## v0.4.0 (2026-03-16)

### New Features

- **Conflict resolution editor** — 3-panel view (Ours | Result | Theirs) with "Accept All Ours/Theirs" and "Mark Resolved". Opens automatically when clicking a conflicted file
- **Line-level staging** — "Select Lines" button in diff view shows per-line checkboxes, stage/unstage individual lines within a hunk
- **Conventional commits** — prefix selector button in commit bar (feat/fix/docs/refactor/perf/test/ci/chore/revert), auto-replaces existing prefix
- **Keyboard navigation** — Tab/Shift+Tab cycles focus between panels, Escape collapses expanded sections, arrow keys navigate lists
- **Theme-aware diff badge colors** — Modified files use `accent` CSS class matching the changes view

## v0.3.1 (2026-03-16)

### New Features

- **Submodules panel** in sidebar — list, init, update submodules
- **Worktrees panel** in sidebar — list worktrees, open in new workspace
- **Merge/rebase conflict banner** — shown at top with Continue/Abort buttons
- **Interactive rebase editor** — right-click commit → Interactive Rebase, pick/squash/fixup/reword/edit/drop
- **Collapsible sidebar sections** — click header to expand/collapse
- **Section item counts** — headers show counts, e.g. "Tags (73)"
- **Sidebar items limit** — configurable in Preferences (default 5, "Show all" expander)
- Empty sections (Stashes, Submodules, Worktrees) auto-hide when unused

## v0.3.0 (2026-03-16)

Major feature release with 2400+ new lines across 31 files.

### New Features

**Staging & Changes**
- Split staged/unstaged file lists with drag-and-drop between them
- Partial staging — stage/unstage individual hunks within a file
- Undo/redo for staging operations and file discards (Ctrl+Z / Ctrl+Shift+Z)
- Syntax-highlighted diffs via syntect — language-aware coloring with dark/light theme support

**Commit History**
- Redesigned commit detail layout — message shown prominently, files with configurable limit and "Show all" expander, technical details collapsed by default
- Paginated commit list — loads 50 commits initially, "Load more" for older history
- Async diff loading — expanding a commit no longer blocks the UI
- Signed commit indicator — lock icon on GPG/SSH-signed commits

**Git Operations**
- Merge conflict detection with continue/abort for merge and rebase
- Interactive rebase — reorder, squash, fixup, reword, drop commits
- Submodules support — list, init, update
- Worktrees management — list, add, remove
- .gitignore editor — edit from the hamburger menu

### Performance

- Parallel workspace scanning — up to 8 threads (significant speedup with 20+ repos)
- Parallel initial repo load — log/tags and status/branches/diffs in concurrent threads
- Lazy diffs in background refresh — only recomputed when status changes
- Parallel background refresh — workspace scan and repo status run simultaneously
- Faster status polling — skip recurse_untracked_dirs during background refresh
- Optimized branch listing — ahead_behind only for HEAD branch

### Other Changes

- Sidebar search now filters stashes
- Commit files limit configurable in Preferences (default 10)
- Stash shortcuts moved to Ctrl+Alt+S / Ctrl+Alt+P
- Makefile `help` target
- About dialog: developer name updated
- Removed unused struct fields (zero compiler warnings)
- User documentation in English and Russian (docs/)

## v0.2.0 (2026-03-09)

- Branch management — create, checkout local/remote branches
- Reset (soft, mixed, hard)
- Stash sidebar panel
- Branch graph window improvements

## v0.1.1 (2026-03-06)

- Auto set-upstream on push
- Layout and stability fixes

## v0.1.0 (2026-03-06)

- Initial release with core Git functionality
