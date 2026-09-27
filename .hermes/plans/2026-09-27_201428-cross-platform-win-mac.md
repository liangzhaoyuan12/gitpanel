# GitPanel 跨平台（Windows / macOS）改造实施计划

> **For Hermes:** 按任务顺序执行本计划；每个任务自含代码、测试与提交步骤。

**Goal:** 让 gitpanel 源码在 Windows 和 macOS 上能编译、能正常运行核心功能（打开仓库、git 操作、交互式 rebase、外部编辑器探测/启动），不改 UI、不动打包。

**Architecture:** 只做平台适配层改造——把 3 处 Unix 假设（`sh -c` 探测、`.sh` 脚本编辑器、`true` 假编辑器、`$HOME`）替换为跨平台实现；新增一个统一的子进程 spawn 助手，在 Windows 上消除控制台窗口闪烁。GTK4/libadwaita 依赖本身三平台均有官方或 MSYS2/Homebrew 支持，不需要换框架。

**Tech Stack:** Rust 1.97 / edition 2021，gtk4-rs 0.9 + libadwaita 0.7（不变）。

---

## 当前上下文 / 已验证的关键事实

调研结论（来自 git 源码与 Rust 文档，实施时可直接依赖）：

1. **git 启动编辑器的机制**（`editor.c: launch_specified_editor`）：`p.use_shell = 1`，
   `run-command.c: prepare_shell_cmd` 把 `GIT_SEQUENCE_EDITOR` 的值拼成
   `sh -c '<值> "$@"' <值> <todo路径>`。规则：
   - 值**不含** shell 特殊字符（`|&;<>()$\`"' 空格 *?[]#~=%` 等）→ **不经过 sh**，直接 exec 该值 + 参数；
   - 值含特殊字符 → 走 `sh -c`，sh 负责拆词。
2. **`GIT_EDITOR=":"` 是 git 内建的“无操作编辑器”**：`editor.c` 里 `if (strcmp(editor, ":"))` 为假时**完全不 spawn 进程**（rebase.c 官方代码自己就这么用）。比现在的 `"true"`（Windows 上不存在 `true` 可执行文件）可靠。
3. **Windows 上 git 找 sh 的方式**：`git_shell_path() = locate_in_PATH("sh")`（Git for Windows 必带 `usr/bin/sh.exe`）。`core.editor = "code --wait"` 这类含空格值在 cmd 下可用是公认事实，故含特殊字符的 editor 值在 Windows 上可用。
4. **Rust std `Command` 在 Windows 能跑 `.bat`/`.cmd`**（CVE-2024-24576 修复保留了支持，自动经 `cmd.exe /C` 并正确转义）。VS Code 的 `code` 在 PATH 上是 `code.cmd`，所以探测必须做 PATHEXT 后缀匹配，不能只找 `.exe`。
5. **GUI 程序在 Windows spawn 控制台子进程（git）会弹控制台窗口**，需 `CREATE_NO_WINDOW (0x08000000)`。
6. **`std::env::current_exe()` 在 Windows 可能返回 `\\?\C:\...` 扩展路径**，直接塞给 sh 会坏，需剥前缀并转正斜杠。
7. **macOS Finder/Dock 启动的 GUI 进程 PATH 极简**（无 Homebrew `/opt/homebrew/bin`），编辑器探测要补目录。

已确认的代码阻塞点（grep 全仓，共 6 类，均在本计划内）：

| # | 位置 | 问题 |
|---|------|------|
| 1 | `src/utils/external_editor.rs:150-226` | `sh -c "command -v ..."` 探测；Windows 无 sh 必挂 |
| 2 | `src/utils/external_editor.rs:189,243` | basename 只切 `/`；裸命令不经 PATHEXT 解析直接 spawn |
| 3 | `src/utils/rebase.rs:42-74` | 生成 `#!/bin/sh` 脚本 + `cp` + `#[cfg(unix)] chmod`，GIT_SEQUENCE_EDITOR 值含反斜杠/空格时被 sh 拆坏 |
| 4 | `src/utils/merge.rs:55,74`、`rebase.rs:66` | `GIT_EDITOR="true"`，Windows 无 `true` |
| 5 | `src/utils/remote.rs:31` | `env::var("HOME")`，Windows 应为 USERPROFILE → 用 `dirs::home_dir()` |
| 6 | 全仓 12 处 `Command::new` | Windows 弹控制台窗口；`src/utils/rebase.rs:57` 是全仓唯一 `std::os::unix` |

另外 `split_command`（`external_editor.rs:73-120`）把 `\` 一律当转义符，Windows 自定义命令
`"C:\Program Files\IDE\ide.exe"` 里的反斜杠会被吃掉 —— 一并修。

基线：`GP_SKIP_GTK_TESTS=1 cargo test --lib` → **58 passed**（改造后必须仍全绿）。

**不在范围内（用户明确表态）：** 打包脚本（deb/dmg/msi）、动态库收集、外观（libadwaita 保持）、CI。
真实 Win/mac 编译验证需要对应机器或 CI，本机（loong64 Linux）只能验证逻辑层 + 全量测试 + clippy。

---

## 实施任务

### Task 1: 子进程助手 `utils::process`（消除 Windows 控制台闪烁）

**Objective:** 所有 spawn `git` 的地方走统一入口，Windows 上带 `CREATE_NO_WINDOW`。

**Files:**
- Create: `src/utils/process.rs`
- Modify: `src/utils/mod.rs`（加 `pub mod process;`）
- Modify: `src/utils/merge.rs:53,72,91,107`、`src/utils/worktrees.rs:51,68`、
  `src/utils/submodules.rs:43`、`src/window.rs:226,4304`（共 8 处 `Command::new("git")`）

**Step 1: 新建 `src/utils/process.rs`**

```rust
//! 子进程 spawn 统一入口。
//!
//! 这是 GUI 程序：在 Windows 上，控制台子进程（git）默认会给父进程
//! 分配一个新的控制台窗口，每次刷新仓库都闪一下黑框。CREATE_NO_WINDOW
//! 抑制窗口，同时不影响管道 stdio。

use std::ffi::OsStr;
use std::process::Command;

/// 构造一个适合 GUI 宿主的子进程 `Command`。
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
```

**Step 2: `src/utils/mod.rs` 加一行 `pub mod process;`。**

**Step 3:** 8 处调用点把 `std::process::Command::new("git")` / 局部 `Command::new("git")`
改成 `crate::utils::process::command("git")`（`window.rs` 两处函数内有
`use std::process::Command;` 的，删掉该局部 import 或改指向）。
链式调用 `.args(...) .env(...) .current_dir(...) .output()` 不变。

**Verify:**

```bash
cargo build 2>&1 | grep -E 'warning|error'   # 期望无 unused import 警告
```

**Commit:** `feat: 统一子进程入口，Windows 下抑制控制台窗口闪烁`

---

### Task 2: `GIT_EDITOR` 假编辑器 `"true"` → `":"`（git 内建 no-op）

**Objective:** Windows 上 git 找不到 `true` 可执行文件会导致 merge/rebase continue 失败；
`":"` 是 git 源码内建的无操作编辑器（完全不 spawn 进程），三平台通用。

**Files:** `src/utils/merge.rs:55`、`src/utils/merge.rs:74`

**Change:** 两处 `.env("GIT_EDITOR", "true")` → `.env("GIT_EDITOR", ":")`，
注释同步改为说明 `":"` 是 git 内建 no-op（editor.c `strcmp(editor, ":")` 跳过 spawn）。

**Verify:** `cargo build` 通过；本任务无独立测试（行为等价，由 Task 8 集成测试覆盖 merge/rebase continue 路径不回归）。

**Commit:** `fix: 用 git 内建 no-op 编辑器 ":" 替代 "true"（Windows 兼容）`

---

### Task 3: 自研序列编辑器 —— 重写 `rebase.rs` 的 todo 注入机制

**Objective:** 干掉 `.sh` 脚本 + `chmod` + `cp` 依赖：改为把**我们自己的可执行文件**
当作 `GIT_SEQUENCE_EDITOR`，git 调它时它把准备好的 todo 拷贝到 git 给的路径。
纯 Rust `fs::copy`，不依赖 sh 拆词、不依赖 `cp`、不依赖可执行位。

**Files:**
- Modify: `src/utils/rebase.rs`（整体重写 `execute_rebase`，新增 4 个函数）
- Modify: `src/main.rs`（入口最前面挂 seq-edit 钩子）

**Step 1: `rebase.rs` 顶部新增（含单测）：**

```rust
/// 环境变量：指向我们准备好的 todo 源文件，只设在 git 子进程环境里。
pub const SEQ_TODO_SRC_ENV: &str = "GP_SEQ_TODO_SRC";
/// 测试/调试钩子：覆盖默认的 GIT_SEQUENCE_EDITOR 值。
pub const SEQ_EDITOR_OVERRIDE_ENV: &str = "GP_SEQUENCE_EDITOR";

/// 剥掉 Windows 扩展长度前缀（current_exe 可能返回 `\\?\C:\...`，
/// sh 不认这个前缀）。
fn strip_extended_prefix(p: &str) -> &str {
    p.strip_prefix(r"\\?\").unwrap_or(p)
}

/// 把可执行文件路径变成 git 接受的 GIT_SEQUENCE_EDITOR 值。
///
/// - 路径不含 shell 特殊字符 → 原样返回，git 直接 exec，**完全不经过 sh**
///   （最稳的路径，Windows 上连 sh 都不用找）。
/// - 含空格等特殊字符 → 单引号包起来走 sh 分支；单引号内部按 `'\''` 转义。
/// - Windows 路径转正斜杠（sh/MSYS 与 CreateProcess 都认 `C:/...`）。
pub fn sequence_editor_value_for(raw: &str, windows: bool) -> String {
    let path = if windows {
        strip_extended_prefix(raw).replace('\\', "/")
    } else {
        raw.to_string()
    };
    // 与 git prepare_shell_cmd 使用的字符集保持一致。
    const SHELL_SPECIAL: &str = r#"|&;<>()$`"' \t\n*?[#~=%"#;
    if path.chars().any(|c| c.is_whitespace() || SHELL_SPECIAL.contains(c)) {
        format!("'{}'", path.replace('\'', r"'\''"))
    } else {
        path
    }
}

/// 本进程可执行文件对应的序列编辑器值。
pub fn sequence_editor_value() -> String {
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("gitpanel"));
    sequence_editor_value_for(&exe.to_string_lossy(), cfg!(windows))
}

/// git 把我们再次当作序列编辑器拉起时的入口（见 `main.rs` 第一行）。
///
/// 只有 `GP_SEQ_TODO_SRC` 存在**且** argv[1]（git 给的 todo 路径）存在才动作；
/// 否则什么都不做直接返回，避免污染正常启动。动作 = 拷贝 + exit，
/// 绝不初始化 GTK/GApplication（否则单实例转发会吞掉这次调用）。
pub fn maybe_run_seq_edit() {
    let Some(src) = std::env::var_os(SEQ_TODO_SRC_ENV) else { return; };
    let Some(dst) = std::env::args_os().nth(1) else { return; };
    match std::fs::copy(std::path::Path::new(&src), std::path::Path::new(&dst)) {
        Ok(_) => std::process::exit(0),
        Err(e) => {
            eprintln!("gitpanel: sequence editor copy failed: {e}");
            std::process::exit(1); // git 会报 "problem with the editor"
        }
    }
}

#[cfg(test)]
mod seq_editor_tests {
    use super::*;

    #[test]
    fn clean_unix_path_is_passed_through_unquoted() {
        assert_eq!(
            sequence_editor_value_for("/home/u/work/gitpanel/target/release/gitpanel", false),
            "/home/u/work/gitpanel/target/release/gitpanel"
        );
    }

    #[test]
    fn windows_path_is_slash_flattened_and_quoted() {
        assert_eq!(
            sequence_editor_value_for(r"C:\Program Files\GitPanel\gitpanel.exe", true),
            "'C:/Program Files/GitPanel/gitpanel.exe'"
        );
    }

    #[test]
    fn windows_extended_prefix_is_stripped() {
        assert_eq!(
            sequence_editor_value_for(r"\\?\C:\tools\gitpanel.exe", true),
            "C:/tools/gitpanel.exe"
        );
    }

    #[test]
    fn single_quotes_inside_path_are_escaped() {
        assert_eq!(
            sequence_editor_value_for("/home/o'brien/bin", false),
            r"'/home/o'\''brien/bin'"
        );
    }

    #[test]
    fn backslash_in_unquoted_unix_path_forces_quoting_not_lossy() {
        // 含 `\` 必然进 sh 分支并被单引号保护，反斜杠不能被吃掉。
        assert_eq!(sequence_editor_value_for(r"/opt/a\b", false), r"'/opt/a\b'");
    }
}
```

**Step 2: 重写 `execute_rebase`**（替换现 25-84 行整段）：

```rust
    /// Execute an interactive rebase using ourselves as the sequence editor.
    /// The `entries` define the rebase todo list; `onto` is the base commit.
    pub fn execute_rebase(&self, entries: &[RebaseEntry], onto: &str) -> Result<String> {
        let repo_path = self.path().to_string_lossy().to_string();

        let todo_content = entries.iter().map(|e| {
            let action = match e.action {
                RebaseAction::Pick => "pick",
                RebaseAction::Squash => "squash",
                RebaseAction::Fixup => "fixup",
                RebaseAction::Reword => "reword",
                RebaseAction::Edit => "edit",
                RebaseAction::Drop => "drop",
            };
            format!("{} {} {}", action, e.short_id, e.message)
        }).collect::<Vec<_>>().join("\n");

        // 源 todo 写到临时文件；路径经 env 传给我们的再入进程，不经 shell，
        // 所以空格、反斜杠、非 ASCII 都不需要转义。
        let todo_path = std::env::temp_dir().join("gitpanel_rebase_todo");
        std::fs::write(&todo_path, &todo_content).context("Failed to write rebase todo")?;

        let editor = std::env::var(SEQ_EDITOR_OVERRIDE_ENV)
            .unwrap_or_else(|_| sequence_editor_value());

        let output = crate::utils::process::command("git")
            .args(["rebase", "-i", onto])
            .env("GIT_SEQUENCE_EDITOR", &editor)
            .env(SEQ_TODO_SRC_ENV, &todo_path)
            // ":" 是 git 内建 no-op 编辑器，防止 reword/edit 拉起 $EDITOR 挂住
            // 后台线程；同时抑制凭据提示。
            .env("GIT_EDITOR", ":")
            .env("GIT_TERMINAL_PROMPT", "0")
            .current_dir(&repo_path)
            .output()
            .context("Failed to run git rebase")?;

        let _ = std::fs::remove_file(&todo_path);

        if output.status.success() {
            Ok("Rebase completed".to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let msg = if stderr.trim().is_empty() { stdout } else { stderr };
            anyhow::bail!("{}", msg.trim());
        }
    }
```

删除项：`script_path`/`script_content` 的生成、`#[cfg(unix)] set_permissions` 块、脚本清理 ——
全仓 `std::os::unix` 归零。`use anyhow::{Context, Result};` 保留。

**Step 3: `src/main.rs`**：

```rust
#![cfg_attr(windows, windows_subsystem = "windows")]

use gitpanel::app::GitpanelApp;
use gitpanel::i18n;
use gitpanel::utils::{config::AppConfig, logging, rebase};

fn main() {
    // git 把本程序再次作为序列编辑器拉起：拷贝 todo 后立即退出，
    // 不碰日志/GTK/GApplication（单实例机制会吞掉二次调用）。
    rebase::maybe_run_seq_edit();

    if let Err(e) = logging::init_logging() { ... }   // 其余不变
```

`#![cfg_attr(windows, ...)]` 必须是文件第一个条目（顺带修掉 Windows 主程序黑框问题）。

**Verify:**

```bash
cargo test --lib rebase::          # 5 个新单测 + 原有 rebase 相关测试
grep -rn 'std::os::unix' src/      # 期望 0 行
GP_SKIP_GTK_TESTS=1 cargo test --lib   # 全绿
```

**Commit:** `refactor: 交互式 rebase 改用自进程序列编辑器，移除 sh 脚本依赖`

---

### Task 4: 外部编辑器探测 —— 纯 Rust PATH 扫描（PATHEXT + macOS 补目录）

**Objective:** 删掉 `sh -c command -v`，实现跨平台 `which`；启动时把裸命令解析成
完整路径（Windows 上 `code` → `code.cmd` 必须靠 PATHEXT 才找得到）。

**Files:** `src/utils/external_editor.rs`

**Step 1: 用下面内容替换 `detect_script`（150-153 行）与其调用者 `detect_installed`
中的 shell 探测段（200-226 行），并删除 `detect_script` 的单测（327-331 行）：**

```rust
/// 探测用的可执行目录：PATH + 平台补充目录。
///
/// macOS 上 Finder/Dock 启动的 GUI 进程只有 launchd 的极简 PATH，
/// Homebrew/MacPorts 目录不在里面，GUI 宿主必须自己补。
pub fn executable_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    #[cfg(target_os = "macos")]
    for extra in ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"] {
        let p = PathBuf::from(extra);
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    }
    dirs
}

/// Windows 的可执行后缀表（PATHEXT）；非 Windows 返回空表，
/// 空表即“Unix 模式”（要求可执行位）。
pub fn pathext() -> Vec<String> {
    #[cfg(windows)]
    {
        let raw = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        return raw
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_uppercase())
            .collect();
    }
    #[cfg(not(windows))]
    Vec::new()
}

/// Unix：文件存在且带可执行位（对齐 `command -v` 语义）。
fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return path
            .metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }
    #[cfg(not(unix))]
    true
}

/// 在给定目录表里解析一个命令名，纯函数，便于测试。
///
/// - 名字含路径分隔符 → 当作显式路径，存在即用；
/// - Windows 模式（`pathext` 非空）→ 依次尝试 `名+后缀`（文件系统大小写不敏感，
///   后缀大小写不用管）；
/// - Unix 模式 → 直接拼接并检查可执行位。
pub fn which_in(name: &str, dirs: &[PathBuf], pathext: &[String]) -> Option<PathBuf> {
    if name.contains('/') || name.contains('\\') {
        let p = PathBuf::from(name);
        return p.is_file().then_some(p);
    }
    for dir in dirs {
        if pathext.is_empty() {
            let p = dir.join(name);
            if is_executable_file(&p) {
                return Some(p);
            }
        } else {
            for ext in pathext {
                let p = dir.join(format!("{name}{ext}"));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// 解析当前平台上的一个命令名。
pub fn which(name: &str) -> Option<PathBuf> {
    which_in(name, &executable_dirs(), &pathext())
}
```

**Step 2: `detect_installed` 改为：**

```rust
pub fn detect_installed() -> Vec<DetectedEditor> {
    if in_flatpak() {
        return Vec::new();
    }
    let found: Vec<String> = KNOWN_EDITORS
        .iter()
        .flat_map(|e| e.commands.iter().copied())
        .filter(|c| which(c).is_some())
        .map(|c| c.to_string())
        .collect();
    match_detected(&found)
}
```

文件顶部 import 调整：`use std::path::{Path, PathBuf};`，删掉不再使用的
`use std::process::{Command, Stdio};` 中的 `Command`（`Stdio` 仍被 `launch` 用）。
`launch` 里 spawn 改为（Task 1 的助手 + 解析裸命令）：

```rust
    let mut argv = build_argv(command_line, &repo_path.to_string_lossy())?;
    let program = argv.remove(0);
    // Windows 上 CreateProcess 只补 .exe，`code` 这种 .cmd 光靠 std 解析不到，
    // 必须自己按 PATHEXT 落到完整路径；解析不到就回退原名，让 spawn 报原始错误。
    let resolved = if program.contains('/') || program.contains('\\') {
        PathBuf::from(&program)
    } else {
        which(&program).unwrap_or_else(|| PathBuf::from(&program))
    };
    tracing::info!("Opening {} with: {} {:?}", repo_path.display(), resolved.display(), argv);
    let child = crate::utils::process::command(&resolved)
        .args(&argv)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| anyhow!("Could not run `{}`: {e}", resolved.display()))?;
```

**Step 3: 单测**（追加进该文件 `mod tests`；用临时目录模拟 PATH，Windows 分支在
Linux 上也能通过显式传 `pathext` 参数测到）：

```rust
    #[test]
    fn which_finds_executable_in_unix_mode() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("code");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let dirs = vec![dir.path().to_path_buf()];
        assert_eq!(which_in("code", &dirs, &[]), Some(bin));
    }

    #[test]
    fn which_simulated_windows_tries_pathext_suffixes() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("code.cmd");
        std::fs::write(&bin, "@echo off\r\n").unwrap();
        let dirs = vec![dir.path().to_path_buf()];
        let exts = vec![".COM".to_string(), ".EXE".to_string(), ".CMD".to_string()];
        assert_eq!(which_in("code", &dirs, &exts), Some(bin));
        assert_eq!(which_in("codium", &dirs, &exts), None);
    }

    #[test]
    fn which_honours_explicit_paths_verbatim() {
        let dirs = vec![];
        assert_eq!(which_in("/no/such/place/tool", &dirs, &[]), None);
    }
```

**Step 4:** `preferences_dialog.rs:230` 注释里 “costs a `flatpak-spawn` round trip” 陈旧
（早已不是 flatpak-spawn），顺手改为 “a PATH probe”。

**Verify:**

```bash
cargo test --lib external_editor
grep -rn 'command -v\|Command::new("sh")' src/   # 期望 0 行
```

**Commit:** `feat: 外部编辑器探测改为纯 Rust PATH 扫描（PATHEXT/macOS 补目录）`

---

### Task 5: `split_command` 反斜杠规则 + `label_for_command` 双分隔符

**Objective:** Windows 自定义命令 `C:\Program Files\IDE\ide.exe` 不能被拆散；
`{path}` 占位与引号语义保持向后兼容。

**Files:** `src/utils/external_editor.rs`（`split_command` 73-120、`label_for_command` 189）

**Step 1: `split_command` 重写为按下标扫描（替换整个函数体）：**

```rust
/// Split a command line into argv.
///
/// 反斜杠规则（与旧实现的关键差异，为了 Windows 路径）：
/// - 引号外：只有 `\` 后跟空白 / 引号 / 另一个 `\` 时才算转义
///   （保住 `/opt/my\ dir` 的老行为，同时让 `C:\Users\...` 原样存活；
///   `\\` 仍然折叠成 `\`）；
/// - 双引号内：只有 `\"` 和 `\\` 是转义（对齐 POSIX sh 在双引号内的语义，
///   `"C:\Program Files\x"` 里的反斜杠不再被吃掉）；
/// - 单引号内：一切字面量（POSIX 语义，不变）。
pub fn split_command(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut argv = Vec::new();
    let mut current = String::new();
    let mut has_token = false;
    let mut quote: Option<char> = None;
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if let Some(q) = quote {
            match ch {
                c if c == q => {
                    quote = None;
                    has_token = true;
                }
                '\\' if q == '"' && i + 1 < chars.len() && matches!(chars[i + 1], '"' | '\\') => {
                    current.push(chars[i + 1]);
                    i += 1;
                    has_token = true;
                }
                c => {
                    current.push(c);
                    has_token = true;
                }
            }
        } else {
            match ch {
                '\'' | '"' => {
                    quote = Some(ch);
                    has_token = true;
                }
                '\\'
                    if i + 1 < chars.len()
                        && (chars[i + 1].is_whitespace()
                            || chars[i + 1] == '\''
                            || chars[i + 1] == '"'
                            || chars[i + 1] == '\\') =>
                {
                    current.push(chars[i + 1]);
                    i += 1;
                    has_token = true;
                }
                c if c.is_whitespace() => {
                    if has_token {
                        argv.push(std::mem::take(&mut current));
                        has_token = false;
                    }
                }
                c => {
                    current.push(c);
                    has_token = true;
                }
            }
        }
        i += 1;
    }
    if has_token {
        argv.push(current);
    }
    argv
}
```

**Step 2: `label_for_command` 的 `.rsplit('/')` → `.rsplit(['/', '\\'])`。**

**Step 3: 单测（追加到 `mod tests`）：**

```rust
    #[test]
    fn windows_path_with_spaces_survives_unquoted() {
        assert_eq!(
            split_command(r"C:\Program Files\IDE\ide.exe {path}"),
            vec![r"C:\Program Files\IDE\ide.exe", "{path}"]
        );
    }

    #[test]
    fn windows_path_inside_double_quotes_keeps_backslashes() {
        assert_eq!(
            split_command(r#""C:\Program Files\IDE\ide.exe" --flag"#),
            vec![r"C:\Program Files\IDE\ide.exe", "--flag"]
        );
    }

    #[test]
    fn posix_backslash_space_escape_still_works() {
        assert_eq!(split_command(r"/opt/my\ dir/x --go"), vec!["/opt/my dir/x", "--go"]);
    }

    #[test]
    fn doubled_backslash_collapses_to_one() {
        assert_eq!(split_command(r"C:\\tools\\ide --x"), vec![r"C:\tools\ide", "--x"]);
    }

    #[test]
    fn label_handles_backslash_paths() {
        assert_eq!(label_for_command(r"C:\tools\myide.exe --flag"), "myide.exe");
    }
```

**Verify:** `cargo test --lib external_editor` 全绿（含原有 8 个老测试，回归由老测试兜底）。

**Commit:** `fix: 命令行拆分兼容 Windows 反斜杠路径`

---

### Task 6: `remote.rs` 的 `$HOME` → `dirs::home_dir()`

**Objective:** Windows 上没有 `HOME`（是 USERPROFILE），SSH key 回退路径会失效。

**Files:** `src/utils/remote.rs:31-37`

**Change:**

```rust
        // dirs::home_dir() 三平台通用（Windows 取 USERPROFILE），
        // 本 crate 已依赖 dirs；env HOME 在 Windows 上通常不存在。
        let home = dirs::home_dir().unwrap_or_default();
        let key_path = home.join(".ssh").join("id_ed25519");
        let key_path = if key_path.exists() { key_path } else { home.join(".ssh").join("id_rsa") };
```

（`dirs` 已在 Cargo.toml，无需加依赖；`clone_dialog.rs:32` 已是同样写法。）

**Verify:** `cargo build`；`cargo test --lib remote`。

**Commit:** `fix: SSH 私钥回退路径改用 dirs::home_dir()`

---

### Task 7: `window.rs` clone 仓库名推导兼容 `\` 分隔的本地路径

**Objective:** Windows 本地路径 clone（`C:\repos\proj`）推导出的仓库名目前是整串路径。

**Files:** `src/window.rs:4311-4317`

**Change:** `url.trim_end_matches('/')` → `url.trim_end_matches(['/', '\\'])`，
`.rsplit('/')` → `.rsplit(['/', '\\'])`。

**Verify:** `cargo build && GP_SKIP_GTK_TESTS=1 cargo test --lib`（该逻辑在 window.rs，
无独立测试点，靠编译 + 全量测试兜底）。

**Commit:** `fix: clone 后仓库名推导兼容 Windows 路径分隔符`

---

### Task 8: 端到端集成测试 —— 真跑一次 `git rebase -i`（含新序列编辑器机制）

**Objective:** 证明 Task 3 的机制在真实 git 下工作：env 传递、sh 解释、todo 覆盖生效、
重排结果正确。用 `GP_SEQUENCE_EDITOR` 覆盖钩子喂一个纯 sh 的等价实现
（`cp "$GP_SEQ_TODO_SRC"`），我们的 exe 路径引号逻辑已由 Task 3 单测覆盖。

**Files:** Create `tests/rebase_test.rs`

**Step 1: 写测试**（4 个互不冲突的文件提交 → 重排最后两个 → 断言日志顺序变了）：

```rust
//! 端到端：execute_rebase 真实驱动 git rebase -i，验证 todo 注入机制。

mod common;

use std::path::Path;

use git2::{Repository, Signature, Time};
use gitpanel::model::RebaseAction;
use serial_test::serial;
use tempfile::TempDir;

/// 与 common::make_repo_with_commits 不同：每个提交写不同文件，
/// 这样重排/丢弃不会在同一行上撞冲突。
fn make_disjoint_repo() -> (TempDir, gitpanel::utils::repository::GitRepo) {
    let dir = tempfile::tempdir().expect("tempdir");
    let raw = Repository::init(dir.path()).expect("git init");
    let sig = Signature::new("Test", "test@example.com", &Time::new(1_700_000_000, 0)).unwrap();
    let files = ["base.txt", "a.txt", "b.txt", "c.txt"];
    for (i, f) in files.iter().enumerate() {
        std::fs::write(dir.path().join(f), format!("{i}\n")).unwrap();
        let mut index = raw.index().unwrap();
        index.add_path(Path::new(f)).unwrap();
        index.write().unwrap();
        let tree = raw.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = raw.head().ok().and_then(|h| h.target()).map(|o| raw.find_commit(o).unwrap());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        raw.commit(Some("HEAD"), &sig, &sig, &files[i], &tree, &parents).unwrap();
    }
    let repo = gitpanel::utils::repository::GitRepo::open(dir.path().to_str().unwrap()).unwrap();
    (dir, repo)
}

#[test]
#[serial] // 会改进程级环境变量 GP_SEQUENCE_EDITOR
fn execute_rebase_applies_the_prepared_todo() {
    let (_dir, repo) = make_disjoint_repo();

    let mut entries = repo.list_rebase_commits(3).expect("entries");
    assert_eq!(entries.len(), 3);
    // 默认 todo = pick A pick B pick C；我们改成 B A C（乱序），文件互不相交故不冲突。
    entries.swap(0, 1);

    // 用覆盖钩子喂一个等价的“拷贝型”序列编辑器：源路径由我们经 env 传给 git，
    // 与生产路径同一套 env 管道，只把“我们 exe 的引号+再入”换成 sh 内联。
    std::env::set_var("GP_SEQUENCE_EDITOR", r#"cp "$GP_SEQ_TODO_SRC""#);
    let result = repo.execute_rebase(&entries, "HEAD~3");
    std::env::remove_var("GP_SEQUENCE_EDITOR");
    result.expect("rebase should succeed");

    let log = repo.log_page(0, 10).expect("log");
    let summaries: Vec<&str> = log.iter().map(|c| c.summary.as_str()).collect();
    // newest-first：重排后 c 还在顶，a/b 互换位置。
    assert_eq!(summaries, vec!["c.txt", "a.txt", "b.txt", "base.txt"]);
}
```

> 若 `log_page` 返回的字段名不是 `summary`，以 `src/model` 实际结构为准调整；
> `list_rebase_commits` 返回 oldest-first（`rebase.rs:17` 注释明确），`swap(0,1)` 即
> 把最早两个提交互换。断言方向按实现时实际日志顺序核对一次再定稿
> （先跑一个只打印 summaries 的临时版本看真实输出）。

**Verify:**

```bash
cargo test --test rebase_test -v          # 1 passed
GP_SKIP_GTK_TESTS=1 cargo test            # 全仓全绿（lib + 4 个集成测试）
```

**Commit:** `test: 端到端覆盖交互式 rebase 的 todo 注入机制`

---

### Task 9: 全量验证 + 收尾审计

**Objective:** 确认没有漏网的平台假设，质量不回退。

**Steps:**

```bash
# 1. 全量测试（含 GTK 测试，本机有显示则不 skip；无显示用 GP_SKIP_GTK_TESTS=1）
GP_SKIP_GTK_TESTS=1 cargo test

# 2. clippy 不许新增告警
cargo clippy --all-targets 2>&1 | tail -30

# 3. 平台假设审计 —— 以下每条都必须是 0 或有正当理由
grep -rn 'Command::new' src/                    # 期望只剩 utils/process.rs 里 1 处
grep -rn 'std::os::unix' src/                    # 期望 0（仅 cfg(unix) 内的可执行位检查）
grep -rn 'env::var("HOME")\|command -v\|/bin/sh' src/   # 期望 0
grep -rn '"true"' src/ --include='*.rs' | grep GIT_EDITOR  # 期望 0
grep -rn 'rsplit(.\/.)' src/                     # 期望 0（都已改双分隔符）

# 4. release 构建
cargo build --release
```

**Windows/macOS 真机验证清单（本机无法执行，交付时如实说明）：**

- Windows（MSYS2 UCRT64）：`pacman -S mingw-w64-ucrt-x86_64-{gtk4,libadwaita,pkgconf,git}` →
  `cargo build --release` → 手测：打开仓库、commit、`git pull`、交互式 rebase 重排、
  Preferences 里 VS Code 探测 + “Open in Editor”、全程无控制台黑框。
- macOS：`brew install gtk4 libadwaita pkg-config` → `cargo build --release` →
  手测同上，另验 Finder 启动时能探测到 `/opt/homebrew/bin` 下的 `code`。

**Commit:**（如 clippy 有修）`chore: clippy 清理`

---

## 风险与权衡

1. **本机只能验证逻辑层。** Windows/macOS 编译与手测需真机或 CI（GitHub Actions 矩阵 +
   MSYS2/brew 装 GTK），是否补 CI 由用户后续决定——不在本计划内（避免过度工程）。
2. **`GP_SEQUENCE_EDITOR` 覆盖钩子**（Task 3）是为可测试性留的后门：仅 `GP_` 前缀、
   只影响自己 spawn 的 git 子进程，泄露面可控。
3. **`split_command` 行为微调**：引号外 `\x`（x 非空白/引号/反斜杠）从“吃掉反斜杠”
   变为“保留两个字符”。旧行为只对 Unix 怪异路径有意义，且无测试依赖它；
   老测试全部保留并须全绿。
4. **编辑器启动在 Windows 不加 `CREATE_NO_WINDOW` 的问题不存在**（统一加了）——
   代价是极端场景（用户自定义命令指向终端编辑器如 vim）在 Windows 会无窗口运行；
   已知编辑器目录表全是 GUI 程序，可接受，代码内注释说明。
5. **`maybe_run_seq_edit` 的双条件门**（env + argv[1]）：用户误导出 `GP_SEQ_TODO_SRC`
   时正常启动不受影响。
