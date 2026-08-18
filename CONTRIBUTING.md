# Contributing to Gitpulsar

Thanks for taking the time. Bug reports, feature requests and merge requests are
all welcome — issues live at
<https://gitlab.com/ilshat-apps/gitpulsar/-/issues>.

## Building

System dependencies:

```sh
# Fedora
sudo dnf install gtk4-devel libadwaita-devel

# Ubuntu / Debian
sudo apt install libgtk-4-dev libadwaita-1-dev

# Arch Linux
sudo pacman -S gtk4 libadwaita
```

Then:

```sh
cargo build              # debug build
cargo run -p gitpulsar-gtk   # run from source
```

Run from source while developing. `make install` copies the binary into
`~/.local` and it is easy to end up testing yesterday's build without noticing.

## The quality gate

Every merge request must pass these three, and CI runs exactly them:

```sh
cargo build
cargo clippy --all-targets -- -D warnings
cargo test
```

Warnings are errors. If clippy is unhappy about existing code you touched,
fix it rather than adding an `#[allow]`.

## Tests

- **Core logic** — integration tests in `crates/gitpulsar-core/tests/`, each
  backed by a throwaway repository built with `tempfile`.
- **Widgets** — inline `#[cfg(test)] mod tests` next to the code they cover.

GTK pins itself to whichever thread calls `gtk::init()` first, so widget tests
must not construct widgets on the test harness's own thread. Use the helpers in
`crates/gitpulsar-gtk/src/test_support.rs`:

```rust
use crate::test_support::{gtk_available, on_gtk_thread};

#[test]
fn my_widget_test() {
    if !gtk_available() { return; }
    let result = on_gtk_thread(|| { /* build widgets, return plain data */ });
    assert_eq!(result, expected);
}
```

Without a display, `cargo test` needs either `xvfb-run -a cargo test` (runs the
widget tests for real, which is what CI does) or `GP_SKIP_GTK_TESTS=1` to
soft-skip them.

## Code style

- Code, comments, commit messages, and documentation are **English only**.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/):
  `fix: …`, `feat(commits): …`, `docs: …`, `chore: …`.
- Match the style of the file you are editing — naming, comment density, and
  idiom. New code should not stand out from its surroundings.

## Architecture

`CLAUDE.md` in the repository root is the architecture reference: the two-crate
split, what lives in each core module, and the GTK patterns this codebase
relies on (`ListView` virtualization, breakpoint tiers, the background refresh
loop). Read the relevant section before changing UI code — several of the
patterns are there to work around GTK behaviour that is not obvious, and a few
approaches have already been tried and rejected.

Two rules that are easy to trip over:

- **Never compute on the UI thread** what the background refresh can do. The
  commit graph, repository status, and diffs all run in worker threads.
- **Row state never goes through `items_changed`.** `GtkListView` skips the
  re-bind when the object at a position is unchanged, so mutating a list item in
  place and emitting `items_changed` does nothing. Push the state onto the
  realized row instead.

## Merge requests

- Branch off `main`.
- Keep a merge request to one logical change.
- CI runs build, clippy and tests on every merge request; make sure it is green.
- Describe what the change does and, for UI changes, include a screenshot or a
  short screen recording. It makes review dramatically faster.
