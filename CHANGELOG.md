# Changelog

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
