// Gitpanel — a GTK4/libadwaita Git client.
//
// Module layout (see docs/项目结构规划书.md):
//   * `utils`  — pure logic, no GTK (git operations, config, undo, editor launch)
//   * `model`  — plain data structures shared between utils and UI
//   * `widgets`— reusable composite UI controls
//   * `app`/`window` — application shell + main window
//   * `main`   — binary entry point

pub mod i18n;
pub mod utils;
pub mod model;
pub mod widgets;
pub mod generated;
pub mod app;
pub mod window;

#[cfg(test)]
pub mod test_support;
