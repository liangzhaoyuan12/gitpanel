use anyhow::{Context, Result};

use crate::model::{RebaseAction, RebaseEntry};
use crate::utils::repository::GitRepo;

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
    // 与 git prepare_shell_cmd (run-command.c) 使用的字符集保持一致：
    // "|&;<>()$`\"' \t\n*?[#~=%" —— 含反斜杠。
    const SHELL_SPECIAL: &str = "|&;<>()$`\\\"' \t\n*?[#~=%";
    if path
        .chars()
        .any(|c| c.is_whitespace() || SHELL_SPECIAL.contains(c))
    {
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
    let Some(src) = std::env::var_os(SEQ_TODO_SRC_ENV) else {
        return;
    };
    let Some(dst) = std::env::args_os().nth(1) else {
        return;
    };
    match std::fs::copy(std::path::Path::new(&src), std::path::Path::new(&dst)) {
        Ok(_) => std::process::exit(0),
        Err(e) => {
            eprintln!("gitpanel: sequence editor copy failed: {e}");
            std::process::exit(1); // git 会报 "problem with the editor"
        }
    }
}

impl GitRepo {
    /// List commits that would be rebased (from HEAD back `count` commits).
    /// Returns entries in reverse order (oldest first) matching rebase todo format.
    pub fn list_rebase_commits(&self, count: usize) -> Result<Vec<RebaseEntry>> {
        let commits = self.log_page(0, count)?;
        let entries: Vec<RebaseEntry> = commits.into_iter().rev().map(|c| {
            RebaseEntry {
                commit_id: c.id,
                short_id: c.short_id,
                message: c.summary,
                action: RebaseAction::Pick,
            }
        }).collect();
        Ok(entries)
    }

    /// Execute an interactive rebase using ourselves as the sequence editor.
    ///
    /// 我们把自己的可执行文件设为 `GIT_SEQUENCE_EDITOR`：git 拉起我们时
    /// （见 [`maybe_run_seq_edit`]）把准备好的 todo 拷进 git 给的路径。
    /// 源路径经 env 传递、目标路径是 git 的 argv —— 都不经过 shell 拆词，
    /// 所以空格、反斜杠、非 ASCII 路径都不需要转义，也没有任何外部
    /// 脚本/`cp`/可执行位依赖，三平台同一套代码。
    ///
    /// `entries` 定义 rebase todo 列表；`onto` 是基底 commit。
    pub fn execute_rebase(&self, entries: &[RebaseEntry], onto: &str) -> Result<String> {
        let repo_path = self.path().to_string_lossy().to_string();

        // Build the todo content
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

        // 源 todo 写到临时文件；路径经 env 传给我们的再入进程。
        let todo_path = std::env::temp_dir().join("gitpanel_rebase_todo");
        std::fs::write(&todo_path, &todo_content).context("Failed to write rebase todo")?;

        let editor = std::env::var(SEQ_EDITOR_OVERRIDE_ENV)
            .unwrap_or_else(|_| sequence_editor_value());

        let output = crate::utils::process::command("git")
            .args(["rebase", "-i", onto])
            .env("GIT_SEQUENCE_EDITOR", &editor)
            .env(SEQ_TODO_SRC_ENV, &todo_path)
            // ":" 是 git 内建 no-op 编辑器（editor.c strcmp(editor, ":") 直接
            // 跳过 spawn），防止 reword/edit 拉起 $EDITOR 挂住后台线程，
            // 同时抑制凭据提示。Windows 上没有 "true" 可执行文件。
            .env("GIT_EDITOR", ":")
            .env("GIT_TERMINAL_PROMPT", "0")
            .current_dir(&repo_path)
            .output()
            .context("Failed to run git rebase")?;

        // Cleanup temp file
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

    #[test]
    fn seq_edit_needs_both_env_and_arg() {
        // 双条件门：缺任一条件都不该动作（这里只验证“不动作即正常返回”
        // 不会 panic/exit —— 直接调用后仍能继续执行即为通过）。
        std::env::remove_var(SEQ_TODO_SRC_ENV);
        maybe_run_seq_edit();
        // 走到这里说明没有误入 seq-edit 分支。
    }
}
