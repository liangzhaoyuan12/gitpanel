//! 子进程 spawn 统一入口。
//!
//! 这是 GUI 程序：在 Windows 上，控制台子进程（git）默认会给父进程
//! 分配一个新的控制台窗口，每次刷新仓库都闪一下黑框。CREATE_NO_WINDOW
//! 抑制窗口，同时不影响管道 stdio。

use std::ffi::OsStr;
use std::process::Command;

/// 构造一个适合 GUI 宿主的子进程 `Command`。
pub fn command(program: impl AsRef<OsStr>) -> Command {
    // `mut` 只在 Windows 分支里被用到（creation_flags），Unix 上是死的。
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
