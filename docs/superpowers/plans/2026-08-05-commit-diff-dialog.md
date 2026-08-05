# Commit Diff Dialog Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the inline per-file diff inside commit rows with a dialog that shows one file at a time side-by-side, backed by a hideable file-list sidebar.

**Architecture:** A new `widgets/commit_diff_dialog.rs` builds an `adw::Dialog` → `adw::ToolbarView` → `adw::OverlaySplitView` (file list | `gtk::Stack` of split/unified views). Rendering reuses the already-written but currently unreferenced `diff_view::render_side_by_side` / `render_unified`, whose hardcoded light-theme colours become theme-aware first. Commit rows pass a callback up to `window.rs`, which presents the dialog with the files already cached on the `CommitObject`.

**Tech Stack:** Rust, GTK4 (`gtk4` 0.9), libadwaita (`libadwaita` 0.7), `serial_test`, existing `crate::test_support`.

## Global Constraints

- Quality gate: `cargo build --release && cargo clippy --all-targets -- -D warnings && cargo test`. On this machine, run cargo with `-j 1` or `-j 2` — the full-parallelism build has been OOM-killed twice.
- GTK widget tests must run their widget work inside `test_support::on_gtk_thread(...)` and be marked `#[serial]`. GTK is pinned to one thread; the test runner is not.
- No `gitpulsar-core` changes. `diff_commit` already returns every `DiffFile`, whose lines carry `old_lineno`/`new_lineno`.
- Row state must never be pushed with `store.items_changed` — `GtkListView` skips the re-bind when the object at that position is unchanged. Use `commit_list::find_row_outer` + `rebind_row`.
- Verify icon names exist before use (`gtk::IconTheme::has_icon`); many symbolic names are absent from Adwaita.
- `commit_files_limit` (config, exposed in Preferences) is currently applied nowhere — the ListView migration dropped the cap that the old `populate_commit_files(&files_box, &files, limit)` call enforced, so a commit row renders every file. Task 3 restores it for the row and applies the same cap to the dialog's sidebar, so the two never disagree.

---

### Task 1: Theme-aware diff colours

`diff_view::setup_tags` hardcodes light-theme hex values, so every renderer built on it is unreadable in the dark theme. Make the palette a pure function of a `dark` flag, and make `setup_tags` update tags that already exist (a buffer is re-rendered when the theme flips, so the early-return-if-present behaviour would keep the stale colours).

**Files:**
- Modify: `crates/gitpulsar-gtk/src/widgets/diff_view.rs:5-33` (`setup_tags`)
- Test: same file, new `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `pub struct DiffPalette` with fields `addition_bg`, `addition_fg`, `deletion_bg`, `deletion_fg`, `hunk_bg`, `hunk_fg`, `file_header_fg`, `lineno_fg`, `empty_bg` — all `&'static str`; `pub fn diff_palette(dark: bool) -> DiffPalette`; `pub fn setup_tags_with(buffer: &gtk::TextBuffer, palette: &DiffPalette)`. `setup_tags` keeps its current signature and picks the palette from `adw::StyleManager::default().is_dark()`.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_differ_by_theme() {
        let light = diff_palette(false);
        let dark = diff_palette(true);
        assert_ne!(light.addition_bg, dark.addition_bg);
        assert_ne!(light.deletion_bg, dark.deletion_bg);
        assert_eq!(light.addition_bg, "#d4edda");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j 1 diff_view`
Expected: FAIL — `cannot find function 'diff_palette' in this scope`.

- [ ] **Step 3: Write the implementation**

```rust
/// Colours for diff rendering, chosen per theme.
pub struct DiffPalette {
    pub addition_bg: &'static str,
    pub addition_fg: &'static str,
    pub deletion_bg: &'static str,
    pub deletion_fg: &'static str,
    pub hunk_bg: &'static str,
    pub hunk_fg: &'static str,
    pub file_header_fg: &'static str,
    pub lineno_fg: &'static str,
    pub empty_bg: &'static str,
}

pub fn diff_palette(dark: bool) -> DiffPalette {
    if dark {
        DiffPalette {
            addition_bg: "#1e3a24", addition_fg: "#7ee787",
            deletion_bg: "#3d1d20", deletion_fg: "#ff7b72",
            hunk_bg: "#12283f", hunk_fg: "#79c0ff",
            file_header_fg: "#8b949e", lineno_fg: "#6e7681",
            empty_bg: "#161b22",
        }
    } else {
        DiffPalette {
            addition_bg: "#d4edda", addition_fg: "#1a7f37",
            deletion_bg: "#f8d7da", deletion_fg: "#cf222e",
            hunk_bg: "#ddf4ff", hunk_fg: "#0969da",
            file_header_fg: "#656d76", lineno_fg: "#8b949e",
            empty_bg: "#f6f8fa",
        }
    }
}
```

Then rewrite `setup_tags` to delegate:

```rust
fn setup_tags(buffer: &gtk::TextBuffer) {
    let dark = adw::StyleManager::default().is_dark();
    setup_tags_with(buffer, &diff_palette(dark));
}

/// Create the diff tags, or restyle them if this buffer already has them —
/// re-rendering after a theme change must not keep the old colours.
pub fn setup_tags_with(buffer: &gtk::TextBuffer, palette: &DiffPalette) {
    let table = buffer.tag_table();
    let mut apply = |name: &str, bg: Option<&str>, fg: Option<&str>, weight: Option<i32>| {
        let tag = table.lookup(name).unwrap_or_else(|| {
            let t = gtk::TextTag::builder().name(name).build();
            table.add(&t);
            t
        });
        tag.set_background(bg);
        tag.set_foreground(fg);
        if let Some(w) = weight {
            tag.set_weight(w);
        }
    };
    apply("addition", Some(palette.addition_bg), Some(palette.addition_fg), None);
    apply("deletion", Some(palette.deletion_bg), Some(palette.deletion_fg), None);
    apply("hunk-header", Some(palette.hunk_bg), Some(palette.hunk_fg), None);
    apply("file-header", None, Some(palette.file_header_fg), Some(700));
    apply("lineno", None, Some(palette.lineno_fg), None);
    apply("empty-line", Some(palette.empty_bg), None, None);
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -j 1 diff_view && cargo clippy -j 1 --all-targets -- -D warnings`
Expected: PASS, no warnings. `diff_view` is still unreferenced at this point; if clippy flags dead code on the new items, that resolves in Task 2 — do not add `#[allow(dead_code)]` unless clippy actually fails here.

- [ ] **Step 5: Commit**

```bash
git add crates/gitpulsar-gtk/src/widgets/diff_view.rs
git commit -m "feat(diff): theme-aware diff colours"
```

---

### Task 2: The dialog module

**Files:**
- Create: `crates/gitpulsar-gtk/src/widgets/commit_diff_dialog.rs`
- Modify: `crates/gitpulsar-gtk/src/widgets/mod.rs` (add `pub mod commit_diff_dialog;`)
- Test: inside the new module

**Interfaces:**
- Consumes: `diff_view::render_side_by_side(&gtk::TextBuffer, &gtk::TextBuffer, &[DiffFile])`, `diff_view::render_unified(&gtk::TextBuffer, &[DiffFile])`, `diff_view::sync_scroll(&gtk::ScrolledWindow, &gtk::ScrolledWindow)`.
- Produces:

```rust
pub struct CommitDiffDialogRefs {
    pub dialog: adw::Dialog,
    pub split_view: adw::OverlaySplitView,
    pub stack: gtk::Stack,
    pub file_list: gtk::ListBox,
}

pub fn build_commit_diff_dialog(
    short_id: &str,
    summary: &str,
    files: &[DiffFile],
    initial_path: &str,
) -> CommitDiffDialogRefs
```

Stack page names are exactly `"split"` and `"unified"`.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    use serial_test::serial;

    use gitpulsar_core::models::{DiffFile, DiffHunk, DiffLine, DiffLineKind, DiffStats};

    use crate::test_support;

    fn sample_files() -> Vec<DiffFile> {
        let line = DiffLine {
            kind: DiffLineKind::Addition,
            content: "let x = 1;\n".into(),
            old_lineno: None,
            new_lineno: Some(10),
        };
        let hunk = DiffHunk {
            header: "@@ -9,0 +10 @@\n".into(),
            lines: vec![line],
        };
        ["src/main.rs", "src/lib.rs"]
            .iter()
            .map(|p| DiffFile {
                path: (*p).to_string(),
                status: gitpulsar_core::models::FileStatusKind::Modified,
                stats: DiffStats { insertions: 1, deletions: 0 },
                hunks: vec![hunk.clone()],
            })
            .collect()
    }

    #[test]
    #[serial]
    fn dialog_lists_files_and_starts_split() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }

        let (rows, page, page_after_toggle) = test_support::on_gtk_thread(|| {
            let files = sample_files();
            let refs = build_commit_diff_dialog("5a6607e", "fix: something", &files, "src/lib.rs");

            let mut rows = 0;
            let mut child = refs.file_list.first_child();
            while let Some(c) = child {
                rows += 1;
                child = c.next_sibling();
            }

            let page = refs.stack.visible_child_name().map(|s| s.to_string());
            refs.stack.set_visible_child_name("unified");
            let after = refs.stack.visible_child_name().map(|s| s.to_string());
            (rows, page, after)
        });

        assert_eq!(rows, 2, "sidebar lists every file in the commit");
        assert_eq!(page.as_deref(), Some("split"));
        assert_eq!(page_after_toggle.as_deref(), Some("unified"));
    }
}
```

Check the real field names of `DiffFile`/`DiffHunk`/`DiffLine`/`DiffStats` in `crates/gitpulsar-core/src/models.rs` before running; fix the literal to match rather than changing the model.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j 1 commit_diff_dialog`
Expected: FAIL — module does not exist.

- [ ] **Step 3: Write the implementation**

Build in this order inside `build_commit_diff_dialog`:

```rust
let dialog = adw::Dialog::builder()
    .title(short_id)
    .content_width(1000)
    .content_height(700)
    .build();

// Two panes for the split view, joined so they scroll together.
let left_view = gtk::TextView::builder()
    .editable(false).monospace(true).cursor_visible(false)
    .wrap_mode(gtk::WrapMode::None).build();
let right_view = gtk::TextView::builder()
    .editable(false).monospace(true).cursor_visible(false)
    .wrap_mode(gtk::WrapMode::None).build();
let left_scroll = gtk::ScrolledWindow::builder().hexpand(true).vexpand(true).build();
let right_scroll = gtk::ScrolledWindow::builder().hexpand(true).vexpand(true).build();
left_scroll.set_child(Some(&left_view));
right_scroll.set_child(Some(&right_view));
diff_view::sync_scroll(&left_scroll, &right_scroll);

let split_page = gtk::Box::new(gtk::Orientation::Horizontal, 0);
split_page.append(&left_scroll);
split_page.append(&gtk::Separator::new(gtk::Orientation::Vertical));
split_page.append(&right_scroll);

let unified_view = gtk::TextView::builder()
    .editable(false).monospace(true).cursor_visible(false)
    .wrap_mode(gtk::WrapMode::None).build();
let unified_scroll = gtk::ScrolledWindow::builder().hexpand(true).vexpand(true).build();
unified_scroll.set_child(Some(&unified_view));

let stack = gtk::Stack::new();
stack.add_named(&split_page, Some("split"));
stack.add_named(&unified_scroll, Some("unified"));
stack.set_visible_child_name("split");
```

Selection + rendering. Keep the files in an `Rc<Vec<DiffFile>>` so the row handlers share one copy:

```rust
let files_rc = std::rc::Rc::new(files.to_vec());

let render = {
    let files_rc = files_rc.clone();
    let left = left_view.buffer();
    let right = right_view.buffer();
    let unified = unified_view.buffer();
    std::rc::Rc::new(move |path: &str| {
        let Some(file) = files_rc.iter().find(|f| f.path == path) else { return };
        let one = std::slice::from_ref(file);
        diff_view::render_side_by_side(&left, &right, one);
        diff_view::render_unified(&unified, one);
    })
};
```

Sidebar: a `gtk::ListBox` with `navigation-sidebar` CSS, one row per file (status icon via the same rule as `commit_list::diff_file_icon`, path label, `+N −M`). On `connect_row_selected`, call `render` with that row's path — store the path with `row.set_widget_name(&file.path)`. Select the row whose path equals `initial_path`, falling back to the first row when it is absent, and call `render` once for it.

Header and adaptivity:

```rust
let header = adw::HeaderBar::new();
let sidebar_toggle = gtk::ToggleButton::builder()
    .icon_name("sidebar-show-symbolic")
    .tooltip_text("Toggle File List")
    .active(true)
    .build();
header.pack_start(&sidebar_toggle);

let view_toggle = gtk::ToggleButton::builder()
    .icon_name("view-dual-symbolic")
    .tooltip_text("Side-by-side")
    .active(true)
    .build();
header.pack_end(&view_toggle);
```

`view-dual-symbolic` is not guaranteed in Adwaita — check with `gtk::IconTheme::for_display(&display).has_icon(name)` and fall back to `"view-paged-symbolic"`, or ship a custom icon under `data/icons/hicolor/scalable/actions/` and add it to the Makefile's icon list, following the `vertical-arrows-none-symbolic` precedent.

Wire `sidebar_toggle` to `split_view.set_show_sidebar(...)` and `view_toggle` to the stack page, then add the breakpoint:

```rust
let bp = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
    adw::BreakpointConditionLengthType::MaxWidth,
    700.0,
    adw::LengthUnit::Sp,
));
bp.add_setter(&split_view, "collapsed", Some(&true.to_value()));
bp.add_setter(&split_view, "show-sidebar", Some(&false.to_value()));
bp.add_setter(&stack, "visible-child-name", Some(&"unified".to_value()));
dialog.add_breakpoint(bp);
```

Finally, re-render on theme changes so the tags restyle:

```rust
let render_for_theme = render.clone();
let file_list_for_theme = file_list.clone();
adw::StyleManager::default().connect_dark_notify(move |_| {
    if let Some(row) = file_list_for_theme.selected_row() {
        render_for_theme(&row.widget_name());
    }
});
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -j 1 commit_diff_dialog && cargo clippy -j 1 --all-targets -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/gitpulsar-gtk/src/widgets/commit_diff_dialog.rs crates/gitpulsar-gtk/src/widgets/mod.rs
git commit -m "feat(commits): commit diff dialog with split view"
```

---

### Task 3: Open the dialog from a commit's file row

**Files:**
- Modify: `crates/gitpulsar-gtk/src/widgets/commit_list.rs` — `build_file_row` (currently line 538), `populate_files_into_outer` (506), `build_row_template`, `build_row_factory`, `build_commit_list_view`, `RowWidgets`
- Modify: `crates/gitpulsar-gtk/src/widgets/window.rs:637` (callback wiring) and the commits-tab block at 631-680
- Test: `commit_list.rs` tests module

**Interfaces:**
- Consumes: `commit_diff_dialog::build_commit_diff_dialog` from Task 2.
- Produces: `pub type FileOpenCallback = std::rc::Rc<dyn Fn(&str, &str)>;` — arguments are `(commit_id, file_path)`. `build_commit_list_view` gains two parameters, `on_open_file: FileOpenCallback` and `files_limit: Rc<Cell<u32>>`, both threaded to `build_row_factory` → `build_row_template`, stored in `RowWidgets` as `on_open_file` and `files_limit`, and read by `populate_files_into_outer`. `build_file_row(file: &DiffFile, commit_id: &str, on_open: FileOpenCallback) -> gtk::Box`. `window.rs` feeds `files_limit` from `imp.config.borrow().commit_files_limit` and updates the cell wherever it already updates `date_format_cell` after a preferences change.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
#[serial]
fn file_row_click_reports_its_path() {
    test_support::ensure_gtk_init();
    if !test_support::gtk_available() {
        return;
    }

    let seen = test_support::on_gtk_thread(|| {
        let seen: Rc<std::cell::RefCell<Vec<(String, String)>>> = Rc::default();
        let sink = seen.clone();
        let cb: FileOpenCallback = Rc::new(move |commit, path| {
            sink.borrow_mut().push((commit.to_string(), path.to_string()));
        });

        let file = DiffFile {
            path: "src/main.rs".into(),
            status: gitpulsar_core::models::FileStatusKind::Modified,
            stats: gitpulsar_core::models::DiffStats { insertions: 1, deletions: 0 },
            hunks: vec![],
        };
        let row = build_file_row(&file, "abc123", cb);
        // The row's only child is the clickable header button.
        let btn = row.first_child().and_then(|c| c.downcast::<gtk::Button>().ok()).expect("button");
        btn.emit_clicked();

        seen.borrow().clone()
    });

    assert_eq!(seen, vec![("abc123".to_string(), "src/main.rs".to_string())]);
}

/// `commit_files_limit` is a live setting in Preferences that the ListView
/// migration stopped honouring. A commit touching more files than the cap must
/// list the cap plus a "… and N more files" note.
#[test]
#[serial]
fn file_list_respects_the_configured_cap() {
    test_support::ensure_gtk_init();
    if !test_support::gtk_available() {
        return;
    }

    let children = test_support::on_gtk_thread(|| {
        let refs = build_commit_list_view(
            Rc::new(|_| {}),
            Rc::new(|| {}),
            Rc::new(|_| {}),
            Rc::new(|_, _| {}),
            Rc::new(std::cell::Cell::new(DateFormat::Iso)),
            Rc::new(std::cell::Cell::new(2u32)),
        );
        let outer = build_row_template(Rc::new(|| {}), Rc::new(|_| {}), Rc::new(|_, _| {}), Rc::new(std::cell::Cell::new(2u32)));
        let obj = CommitObject::from_info(&sample_commit(), vec![], true, false);
        obj.set_files(vec![sample_diff_file("a.rs"), sample_diff_file("b.rs"), sample_diff_file("c.rs")]);
        obj.set_files_loaded(true);
        obj.set_expanded(true);
        rebind_row(&outer, &obj, DateFormat::Iso);

        drop(refs);
        let files_box = row_widgets(&outer).map(|w| w.files_box.clone()).expect("files box");
        let mut n = 0;
        let mut child = files_box.first_child();
        while let Some(c) = child {
            n += 1;
            child = c.next_sibling();
        }
        n
    });

    assert_eq!(children, 3, "two file rows plus the overflow note");
}
```

Add a `sample_diff_file(path: &str) -> DiffFile` helper to the tests module alongside `sample_commit`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j 1 file_row_click_reports_its_path`
Expected: FAIL — `build_file_row` takes one argument.

- [ ] **Step 3: Write the implementation**

In `build_file_row`, delete the inline diff entirely — the `diff_box`, the `diff_tv`, the `super::changes_view::render_file_diff` call, the height-cap arithmetic, and the toggle handler at the end of the function. Delete the expand `arrow` too; the row no longer expands. Then:

```rust
fn build_file_row(file: &DiffFile, commit_id: &str, on_open: FileOpenCallback) -> gtk::Box {
    // ...icon, path label, stats as today, minus the arrow...
    let header_btn = gtk::Button::builder().child(&file_row).css_classes(["flat"]).build();
    let path = file.path.clone();
    let commit_id = commit_id.to_string();
    header_btn.connect_clicked(move |_| on_open(&commit_id, &path));
    container.append(&header_btn);
    container
}
```

`populate_files_into_outer` passes `obj.id()` and the callback from `row_widgets(outer)`, and restores the dropped cap. The limit lives in config, which `commit_list` cannot read, so thread it in the same way `date_format_cell` already is — a `Rc<Cell<u32>>` handed to `build_commit_list_view` and stored in `RowWidgets`:

```rust
let files = obj.files();
let limit = files_limit.get() as usize;
let shown = files.len().min(limit.max(1));
for f in &files[..shown] {
    files_box.append(&build_file_row(f, &obj.id(), on_open.clone()));
}
if shown < files.len() {
    let more = gtk::Label::builder()
        .label(format!("… and {} more files", files.len() - shown))
        .css_classes(["caption", "dim-label"])
        .xalign(0.0)
        .build();
    files_box.append(&more);
}
```

The dialog gets the same treatment: `build_commit_diff_dialog` takes the already-capped slice, so its sidebar and the row list always agree. `window.rs` passes `&obj.files()[..shown]` computed the same way.

In `window.rs`, add the fifth argument next to the existing `on_activate`:

```rust
let win_for_file = self.clone();
let on_open_file: commit_list_v::FileOpenCallback = std::rc::Rc::new(move |commit_id, path| {
    win_for_file.open_commit_file_diff(commit_id, path);
});
```

and a method that reads the files already cached on the object — no git call:

```rust
fn open_commit_file_diff(&self, commit_id: &str, path: &str) {
    let imp = self.imp();
    let Some(store) = imp.commit_store.borrow().clone() else { return };
    let n = store.n_items();
    for i in 0..n {
        let Some(obj) = store.item(i)
            .and_then(|o| o.downcast::<super::commit_object::CommitObject>().ok())
        else { continue };
        if obj.id() != commit_id {
            continue;
        }
        let refs = super::commit_diff_dialog::build_commit_diff_dialog(
            &obj.short_id(), &obj.summary(), &obj.files(), path,
        );
        refs.dialog.present(Some(self));
        return;
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -j 1 && cargo clippy -j 1 --all-targets -- -D warnings`
Expected: PASS, no warnings. Confirm `changes_view::render_file_diff` still has callers inside `changes_view.rs`; if the only caller was the code just deleted, that is a mistake — the Changes tab needs it.

- [ ] **Step 5: Verify in the running app**

Run: `cargo run -j 1 -p gitpulsar-gtk`
Check: expanding a commit lists its files with no expander arrows; clicking one opens the dialog on that file; the sidebar switches files; the sidebar toggle hides and shows the list; narrowing the window below ~700sp collapses the sidebar and switches to the unified view; the dark/light toggle restyles an open diff.

- [ ] **Step 6: Commit**

```bash
git add crates/gitpulsar-gtk/src/widgets/commit_list.rs crates/gitpulsar-gtk/src/widgets/window.rs
git commit -m "feat(commits): open file diffs in the dialog, drop inline expansion"
```

---

### Task 4: Signature icons without a click

The lock icon only appears after a commit is expanded, because v1.1.0 moved `extract_signature` out of `log_page`. Compute it for the loaded page in the background instead.

**Files:**
- Modify: `crates/gitpulsar-gtk/src/widgets/commit_list.rs` (new `apply_signature_flags`)
- Modify: `crates/gitpulsar-gtk/src/widgets/window.rs` — `populate_commit_list` (1673) and the `load_more_commits` completion handler (1694)
- Test: `commit_list.rs` tests module

**Interfaces:**
- Consumes: `GitRepo::commit_is_signed(&self, oid_hex: &str) -> bool`, `commit_list::find_row_outer`, `commit_list::rebind_row`, `GitpulsarWindow::rebind_commit_row`.
- Produces: `pub fn apply_signature_flags(store: &gio::ListStore, flags: &[(String, bool)]) -> Vec<String>` — sets `is_signed` on matching objects and returns the ids whose value actually changed, so callers rebind only those rows.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
#[serial]
fn signature_flags_land_on_matching_commits() {
    test_support::ensure_gtk_init();
    if !test_support::gtk_available() {
        return;
    }

    let (changed, signed, untouched) = test_support::on_gtk_thread(|| {
        let store = gio::ListStore::new::<CommitObject>();
        let a = CommitObject::from_info(&sample_commit(), vec![], true, false);
        store.append(&a);

        let changed = apply_signature_flags(
            &store,
            &[(a.id(), true), ("no-such-commit".to_string(), true)],
        );
        (changed, a.is_signed(), store.n_items())
    });

    assert_eq!(changed.len(), 1, "only the matching commit is reported");
    assert!(signed);
    assert_eq!(untouched, 1, "unknown ids must not add rows");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j 1 signature_flags_land_on_matching_commits`
Expected: FAIL — `apply_signature_flags` not found.

- [ ] **Step 3: Write the implementation**

```rust
pub fn apply_signature_flags(store: &gio::ListStore, flags: &[(String, bool)]) -> Vec<String> {
    let mut changed = Vec::new();
    for i in 0..store.n_items() {
        let Some(obj) = store.item(i).and_then(|o| o.downcast::<CommitObject>().ok()) else {
            continue;
        };
        if obj.is_load_more_sentinel() {
            continue;
        }
        let id = obj.id();
        if let Some((_, signed)) = flags.iter().find(|(fid, _)| *fid == id) {
            if *signed && !obj.is_signed() {
                obj.set_is_signed(true);
                changed.push(id);
            }
        }
    }
    changed
}
```

In `window.rs`, add the batch and call it at the end of `populate_commit_list` and after `append_commits_to_store` in the load-more handler, passing only the ids that page added:

```rust
fn spawn_signature_batch(&self, ids: Vec<String>) {
    if ids.is_empty() {
        return;
    }
    let Some(repo_path) = self.repo_path_string() else { return };
    let path_for_thread = repo_path.clone();

    let (tx, rx) = async_channel::bounded::<Vec<(String, bool)>>(1);
    std::thread::spawn(move || {
        let Ok(repo) = GitRepo::open(&path_for_thread) else { return };
        let flags = ids
            .into_iter()
            .map(|id| {
                let signed = repo.commit_is_signed(&id);
                (id, signed)
            })
            .collect();
        let _ = tx.send_blocking(flags);
    });

    let win = self.clone();
    glib::spawn_future_local(async move {
        let Ok(flags) = rx.recv().await else { return };
        // Drop the result if the user switched repositories meanwhile.
        if win.repo_path_string().as_deref() != Some(repo_path.as_str()) {
            return;
        }
        let imp = win.imp();
        let Some(store) = imp.commit_store.borrow().clone() else { return };
        let changed = super::commit_list::apply_signature_flags(&store, &flags);
        for id in changed {
            let obj = (0..store.n_items())
                .filter_map(|i| store.item(i))
                .filter_map(|o| o.downcast::<super::commit_object::CommitObject>().ok())
                .find(|o| o.id() == id);
            if let Some(obj) = obj {
                win.rebind_commit_row(&id, &obj);
            }
        }
    });
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -j 1 && cargo clippy -j 1 --all-targets -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 5: Verify in the running app**

Run: `cargo run -j 1 -p gitpulsar-gtk` against a repository with at least one signed commit.
Check: the lock appears shortly after the list renders, without any click; scrolling and "Load more" keep it correct; switching repositories quickly does not stamp locks from the previous repository.

- [ ] **Step 6: Commit**

```bash
git add crates/gitpulsar-gtk/src/widgets/commit_list.rs crates/gitpulsar-gtk/src/widgets/window.rs
git commit -m "feat(commits): fill signature icons from a background batch"
```

---

### Task 5: Documentation and release notes

**Files:**
- Modify: `CLAUDE.md` (key patterns)
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Update CLAUDE.md**

Add to the key-patterns list, next to the Commits-view entry:

- **Commit file diffs**: click a file inside an expanded commit to open `commit_diff_dialog` — `adw::Dialog` + `AdwOverlaySplitView` (file list | `gtk::Stack` of side-by-side / unified). Rendering lives in `diff_view.rs`; its palette is theme-aware via `diff_palette(dark)`. Below 700sp the dialog collapses the sidebar and switches to unified.
- **Row state never goes through `items_changed`**: `GtkListView` skips the re-bind when the object at a position is unchanged. Mutating a `CommitObject` in place requires `commit_list::find_row_outer` + `rebind_row` (see `rebind_commit_row`).

- [ ] **Step 2: Add the CHANGELOG entry**

Under a new `## Unreleased` heading, listing: the commit diff dialog, single-click commit expansion, the header chrome that follows the sidebars, background signature icons, the row widget cache, and the recycled-row stage/unstage fix.

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md CHANGELOG.md
git commit -m "docs: commit diff dialog + this cycle's fixes"
```

---

## Verification before release

The release gate has not been run this cycle — the machine ran out of memory twice during builds. Before tagging:

```bash
cargo build --release -j 1
cargo clippy -j 1 --all-targets -- -D warnings
cargo test -j 1
```

Then follow the release process in `CLAUDE.md`: version in `Cargo.toml`, `CHANGELOG.md` section, `data/io.gitlab.ilshat_apps.gitpulsar.metainfo.xml` release block, tag, and the Flathub manifest update.
