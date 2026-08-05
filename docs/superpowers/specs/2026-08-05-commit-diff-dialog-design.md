# Commit diff dialog — design

Date: 2026-08-05
Status: approved, not yet implemented

## Problem

Clicking a file inside an expanded commit row opens the file's diff inline, in a
`TextView` capped at 25 lines (~450 px) nested inside a `ListView` row. Collapsing
it again is unreliable: the click often leaves the diff open and scrolls down into
the file's code instead. A tall, scrollable text widget inside a virtualized list
row is the underlying awkwardness, not a single misplaced handler.

Separately, `crates/gitpulsar-gtk/src/widgets/diff_view.rs` (188 lines) is dead
code — nothing in the tree references it — and it already implements the
side-by-side rendering this feature needs.

## Decisions

| Question | Decision |
|---|---|
| Inline vs dialog | Dialog only; inline per-file diff in commit rows is removed |
| Dialog contents | Selected file, plus a list of the commit's files to switch between |
| File list | Hideable `AdwOverlaySplitView` sidebar with a header toggle |
| Narrow screens | Below 700sp: sidebar collapses to overlay, view switches to unified |
| Syntax highlighting | No — diff colours only |
| Signature icon | Computed in a background batch after each page renders |

## Architecture

### New module: `widgets/commit_diff_dialog.rs`

A builder returning `(adw::Dialog, CommitDiffDialogRefs)`, matching the shape of
the other dialogs in this crate.

```
adw::Dialog  (content_width/height set; add_breakpoint available directly —
              no BreakpointBin needed)
└── adw::ToolbarView
    ├── adw::HeaderBar
    │   ├── start: sidebar toggle (ToggleButton)
    │   ├── title: "<short_id> · <summary>"
    │   └── end:   split/unified toggle (ToggleButton)
    └── AdwOverlaySplitView
        ├── sidebar: file list (status icon, path, +N −M)
        └── content: gtk::Stack
            ├── "split":   two ScrolledWindow + TextView, joined by
            │              diff_view::sync_scroll
            └── "unified": one ScrolledWindow + TextView
```

Rendering reuses the existing `diff_view::render_side_by_side` and
`diff_view::render_unified`, each called with a one-element slice holding the
selected `DiffFile`.

### Adaptive behaviour

One `adw::Breakpoint` on the dialog, `max-width: 700sp`:

- `split_view.collapsed = true` — the file list becomes an overlay with the
  built-in edge-swipe gesture, consistent with the main window's sidebars.
- The stack shows `"unified"`.

The header toggle stays live at every width, so a wide-window user can still
choose the unified view.

### Theme-aware diff colours

`diff_view::setup_tags` hardcodes light-theme hex values (`#d4edda`, `#f8d7da`,
`#ddf4ff`, `#f6f8fa`), which would be unreadable in the dark theme. The palette
becomes a pure function of `adw::StyleManager::is_dark()`; the dialog subscribes
to `notify::dark` and re-renders the current file when the theme flips.

### Data flow

The sidebar lists exactly the files the commit row lists — the same
`commit_files_limit` cap from config applies, so the two never disagree.

No `gitpulsar-core` changes. `diff_commit` already returns every `DiffFile` with
hunks whose lines carry `old_lineno`/`new_lineno` — exactly what side-by-side
alignment needs. The dialog is handed the files already cached on the
`CommitObject`, so opening it costs no further git reads.

### Wiring

`commit_list::build_commit_file_row` loses its `diff_box`, inline `TextView`,
`render_file_diff` call and toggle handler. It gains a callback, threaded through
`build_commit_list_view` → `build_row_factory` → `populate_files_into_outer`,
fired with the commit id and file path. `window.rs` handles it by building and
presenting the dialog.

### Signature icon batch

After a commits page renders (initial load and each "Load more"), a background
thread computes `commit_is_signed` for the newly added ids and returns
`Vec<(String, bool)>`. On the main thread each flag is set on its `CommitObject`
and the row is refreshed via `rebind_commit_row`. Results are dropped if the
repository changed while the batch ran — the same guard added to the lazy diff
fetch.

Note: `store.items_changed` cannot be used to refresh a row. `GtkListView` skips
the re-bind when the object at that position is unchanged, which it is whenever a
`CommitObject` is mutated in place. Row state must be pushed through
`rebind_commit_row`.

## Testing

- Palette selection (light/dark) — pure unit test.
- Dialog smoke test on the GTK test thread: builds, the sidebar lists the
  expected number of files, toggling the view switches the stack page.
- File row activation invokes the callback with the correct path; the row is
  built by a pure function, so its button can be activated directly.

## Out of scope

- Syntax highlighting inside the dialog.
- Word-level diff.
- Reusing the dialog for the Changes tab (its inline expansion stays — hunk
  staging depends on it) or for branch comparison.

## Known gap

The inline collapse misbehaviour was never root-caused; this design removes the
mechanism it lived in. If inline file expansion is ever reintroduced elsewhere,
the investigation starts from scratch.
