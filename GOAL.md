# GitPanel 性能优化与测试 · 目标文档 (GOAL.md)

项目: gitpanel 1.4.0 — GTK4/libadwaita + git2 的 Git GUI 客户端 (Rust, edition 2021)
平台: Deepin 25 / LoongArch64 (3A5000, 性能敏感, 无独显加速预期)

---

## 反幻觉与反重复机制 (每次会话必读)

1. **先读后写**: 改任何源文件前必须 `read_file` 读当前内容, 不凭记忆打补丁。
2. **先跑再报**: 没有真实跑过 `cargo test` / bench 就不许说"通过""变快了"; 性能结论必须给出数字前后对比。
3. **不重复提交**: 提交前先 `git log --oneline -10`; 已提交的内容不许再来一遍。
4. **不重复修复**: 动手前先读本文件的"已完成/已知问题"段; 已修的不再修。
5. **每次任务后更新本文件**: 完成一项就勾选 `[x]`, 并把新测到的数字写进"状态快照", 覆盖旧数字。
6. **单次会话只做 2-3 项**: 一个小批次做完、验证过, 再开下一批。
7. **测试数量前后对比**: 任务开始 `cargo test 2>&1 | grep "test result"`, 结束再跑一次, 测试只许增不许减。
8. **不许重复同一个探测**: 工具返回空/被截断时, 改读落盘文件或换一个不同的命令, 不许循环重发同一条命令。
9. **长命令走 `terminal`** (构建、bench、覆盖率), 不许放进会 300s 超时的 `execute_code` 循环。
10. **本地提交不自动 push** (用户既定工作流)。

---

## 当前状态快照 (2026-09-23 实测)

代码规模:
- 59 个 .rs 文件 (src + tests + build.rs), 共 19,475 行; src 主体 14,863 行
- 最大文件 `src/window.rs` = 6,083 行 (god object, 占 src 的 41%)
- 其次: `src/widgets/changes_view.rs` 1,236 行, `src/widgets/commit_list.rs` 1,126 行, `src/i18n.rs` 965 行

质量信号 (实测):
- `.unwrap()/.expect()` 37 处 (源码含测试辅助)
- `.clone()` 604 处 (潜在多余分配的主要来源)
- `cargo build` 警告 46 条; `cargo clippy --all-targets` 警告 70 条
  (最多的一类: 11 条 needless borrow; 大量 i18n 枚举命名 non_camel_case)
- TODO/FIXME/HACK: 0 处

测试现状:
- `cargo test` 全绿: 51 (lib) + 5 + 7 (integration) = **63 passed / 0 failed**, 0.46s + 0.05s + 0.05s
- 测试文件仅 2 个集成测试: `tests/repository_test.rs`, `tests/remote_positions_test.rs`
- `src/utils/` 21 个文件中 **17 个没有任何 `#[test]`** (只有 remote/external_editor/undo/logging 有)
- 无任何 benchmark (无 criterion), 无覆盖率数据, 无启动耗时/内存基线

构建产物:
- release 二进制 12M (`target/release/gitpanel`), `target/` 占 4.5G

用户报告的两个功能问题 (2026-09-23, 已在代码中定位到根因, 详见 Phase 5):
- **外部改动不自动刷新**: 已有 30s 定时刷新 (`window.rs:4061 setup_auto_refresh`, `config.refresh_interval_secs` 默认 30)
  和窗口重获焦点时刷新 (`connect_is_active_notify`), 但没有任何文件系统监听 (全仓无 `FileMonitor`/inotify),
  且后台刷新对 workspace 扫描做了 tick 节流 (`window.rs:4790` 起, 每 4 或 16 个 tick 才全扫),
  当前选中文件的 diff 视图刷新后是否重渲染未确认 —— 用户实测: 进仓库后外部改代码, 界面不更新。
- **二进制文件 diff 提示时有时无 (根因已确认)**: `parse_diff` (`utils/diff.rs:124`) 从 git2 delta flags 取
  `is_binary`, unstaged/staged/commit diff 都走这条路 → 会显示提示; 但未跟踪文件走 `diff_untracked`
  (`utils/diff.rs:9-41`), 它用 `read_to_string` 读文件且 `is_binary` 硬编码 `false`,
  二进制文件读取必然 Err, 调用处 `window.rs:3102` 是 `if let Ok(f)` —— **失败被静默吞掉, 什么都不显示**。
  所以"已跟踪的二进制改动显示提示、未跟踪的二进制文件不显示", 与用户观察到的"有时显示有时不显示"完全吻合。

性能基线快照 (2026-09-24 实测, 优化前; 复测命令: `cargo bench` / `examples/repo_probe`):
- criterion 基线 (小仓库 gitpanel 自身, 907 文件): status_collapsed 6.11ms, status_recursive 6.24ms,
  diff_unstaged 16.3ms, diff_staged 752µs, log_50 1.47ms, i18n_t_5keys 390ns, collect_changed_files_500 166µs
- 大仓库 (~/work/bench/large-repo, 50,500 文件 / 37,037 commits, 20 脏文件):
  open 21ms, status(collapsed) 346ms, status(recursive) 270ms, diff_unstaged 241ms,
  diff_staged 34ms, **log(50) 1.44s**, **log_all_page(0,50) 713ms**, tags_by_commit 1.1ms, ahead_behind 0.64ms
- 热点排序 (大仓库, 单次刷新/打开的耗时占比):
  ① log(50) 1.44s — 全历史 TIME+TOPOLOGical 排序, 打开仓库时直接可见的卡顿来源
  ② log_all_page 713ms — 应用打开仓库实际调用的入口 (push_head + 2 个 push_glob 全排序)
  ③ status 270-356ms — 每次后台刷新的 probe, 定了刷新延迟下限
  ④ diff_unstaged 241ms — status hash 变化后重算, 大仓库刷新的实际成本
  ⑤ diff_staged 34ms / status 内部无单点热点 (i18n 390ns 级, 可忽略)
- 启动耗时/RSS: **未测, 见 0.2 (阻塞: 单实例)**

已知性能问题线索 (来自 git log 与代码结构, 待 Phase 0 量化确认):
- 侧栏/变更列表曾两次出现"后台刷新导致重建、滚动位置丢失"问题 (commit 4508164, 614314e) —— 说明刷新路径是**全量重建**, 是首要优化点
- syntect 语法高亮在源码中被引用 56 处, 疑似在 UI 线程对整个 diff/文件做同步解析
- git2 同步调用 (`Repository::discover` / `statuses` / `revwalk` / `diff`) 共 19 处, 疑似直接跑在 GTK 主线程上, 大仓库会卡 UI
- 仅用 `async-channel` 做了部分异步 (glib/MainContext/spawn_local 相关引用 44 处), 但主刷新链路未确认异步

---

## 工作阶段

### Phase 0: 建立可复现的性能基线 (必须第一个做, 没有基线不许优化)

- [x] 0.1 criterion 基线 (2026-09-24 完成): `criterion = "0.8"` + `benches/hot_paths.rs`
      (status_collapsed/recursive、diff_unstaged/staged、log_50、i18n_t、collect_changed_files_500)。
      基线数字见下方"性能基线快照"。运行: `cargo bench`。
- [ ] 0.2 启动耗时与内存基线 — **阻塞, 待用户输入**: 脚本 `scripts/startup-bench.sh` 已就绪
      (xdotool 等窗口出现计时 + VmRSS, 5 次取中位数), 但 GitPanel 是 GApplication 单实例
      (application_id 硬编码), 桌面上已有实例 (pid 1157349, 9/23 09:40 启动) 时新进程转发后即退出,
      测不到冷启动。需用户同意临时关闭该实例 (测完立即重启), 或提供绕开单实例的办法。
      注: 二进制不支持 `--version` (GTK app, 走 HANDLES_OPEN)。
- [x] 0.3 大仓库样本 (2026-09-24 完成): `~/work/bench/large-repo`
      = hermes-agent 的 `git clone --shared` (对象库不复制, 省磁盘) + 生成 36,601 个文件分 8 次提交。
      实测: **50,500 文件 / 37,037 commits / 375M**。含 20 个脏文件 (diff 场景)。
      配套探针: `cargo run --release --example repo_probe <path>` — 对任意仓库测
      open/status/diff/log_all_page 耗时 (真实代码路径, 非 git CLI 近似)。
- [x] 0.4 tracing span (2026-09-24 代码完成): `trigger_background_refresh` 后台线程加
      `info_span!("background_refresh")` + probe/apply 两段 `debug!(elapsed_ms=...)`;
      `load_repo_data` 后台线程加 `info_span!("load_repo_data")` + 加载耗时 debug。
      查看方式: `RUST_LOG=gitpanel=debug gitpanel <repo>`。热点清单见下方"性能基线快照"
      (由 repo_probe + criterion 直接测得, 无需起 GUI)。

### Phase 1: 静态质量清零 (低风险, 独立可交付)

- [ ] 1.1 `cargo build` 警告 46 → 0。验证: `cargo build 2>&1 | grep -c "^warning"` = 0
- [ ] 1.2 `cargo clippy --all-targets` 警告 70 → 0 (needless borrow 11 条优先;
      i18n 枚举命名用 `#[allow(non_camel_case_types)]` 统一处理, 不逐个改名以免破坏翻译键)。
      验证: `cargo clippy --all-targets 2>&1 | grep -c "^warning"` = 0
- [ ] 1.3 源码 (非测试) 中 `unwrap()/expect()` 37 → ≤10, 全部改 `?` 或带上下文的 `anyhow::Context`;
      确实不变的 invariant 就地写注释说明为什么不会失败。
- [ ] 1.4 拆分 `src/window.rs` (6,083 行 → 目标 ≤3 个文件、单文件 ≤1,500 行):
      按已有的语义分组切 (菜单/快捷键, 侧栏与刷新链路, 远程与状态栏), **纯移动代码, 不改行为**。
      验证: 拆分前后 `cargo test` 均为 63+ 全绿, 二进制能启动。

### Phase 2: 测试补齐 (与 Phase 1 可并行)

- [ ] 2.1 给 17 个无测试的 `src/utils/*.rs` 每个文件至少补 2 个单测 (纯逻辑, 不起 GTK)。
      目标: `cargo test` 用例数 63 → ≥120, 且 `src/utils/` 无测试文件数 17 → 0。
      验证: `grep -L "#\[test\]" src/utils/*.rs | wc -l` = 0
- [ ] 2.2 覆盖率: 引入 `cargo-llvm-cov`, 记录 line coverage 基线, 目标 `utils/` 模块 ≥70%。
      验证: `cargo llvm-cov --lib 2>&1 | grep -A2 "TOTAL"` 数字写入快照。
- [ ] 2.3 错误路径测试: 现有测试几乎全是 happy path —— 补: 不存在的仓库路径、损坏的 .git、
      只读文件系统上的写操作、冲突状态下的 merge/rebase、空仓库首提交。
- [ ] 2.4 集成测试补场景: 用 `tempfile` + `git2` 造仓库, 覆盖 commit/stash/branch/rebase/undo 全链路,
      目标集成测试用例 12 → ≥40。
- [ ] 2.5 GTK 相关测试保持 `gtk_available()` 守卫, 无显示环境 (`cargo test` 于 ssh) 必须仍全绿。

### Phase 3: 性能优化 (每一项必须先有 Phase 0 的测量证据, 改完必须复测)

- [ ] 3.1 **主线程阻塞**: 所有 git2 重活 (status 扫描、log/revwalk、diff 计算、clone/fetch) 移到
      后台线程, 结果经 channel 回主线程更新 UI; 主线程只做渲染。
      验收: 打开 0.3 的大仓库样本时 UI 不掉帧 (连续拖动侧栏无卡顿, 人肉 + tracing span 双确认),
      status 计算耗时与优化前对比 ≥2x 或"UI 线程上该操作耗时 = 0"。
- [ ] 3.2 **列表全量重建**: `refresh_changes_list` / 侧栏刷新改为按状态 diff 增量更新 ListStore,
      保留滚动位置与选中项 (历史 bug 4508164、614314e 的根治)。
      验收: 刷新后滚动位置与选中行不变 (补一个回归测试覆盖该行为), 大仓库刷新耗时对比数字入快照。
- [ ] 3.3 **syntect 高亮**: 高亮移到后台 + 按 (文件, 哈希) 缓存结果; >200KB 或 >5,000 行的文件
      不高亮直接原文展示; 句子只解析可见窗口。
      验收: 打开 1MB 文件的耗时对比数字入快照, 目标 ≤100ms 出内容。
- [ ] 3.4 **分配削减**: 从 `cargo flamegraph`/tracing 热点出发, 消掉热点路径上的 `.clone()`
      (全仓 604 处, 只改热点, 不搞全仓机械替换); `to_string` 大循环改 `write!`/`Cow`。
      验收: 0.1 的 criterion bench 中至少 2 项 ≥20% 提升, 且测试全绿。
- [ ] 3.5 **启动耗时**: 复测 0.2 的启动数字, 目标较基线 ≥30% 提升或绝对值 ≤500ms。
- [ ] 3.6 二进制体积 (可选): 12M → 检查 `strip = true`、`opt-level = "z"`、lto 对龙芯编译时间的影响,
      只在不明显拖慢构建时采纳。

### Phase 4: 回归防护 (防止优化把性能改回去)

- [ ] 4.1 把 0.1 的 criterion bench 接进常规流程, 记录阈值: 关键 bench 下降 >10% 即视为回归。
- [ ] 4.2 写 `scripts/perf-check.sh`: 一条命令跑 clippy(0 警告) + test + bench + 启动计时,
      输出对比表; 以后每次优化前后各跑一次。
- [ ] 4.3 本文件"状态快照"更新为优化后的最终数字, 形成前后对照表。

### Phase 5: 用户报告的缺陷与自动刷新 (功能正确性, 优先级最高, 可先于 Phase 3 做)

- [x] 5.1 **自动刷新 (外部改动实时更新)** (2026-09-23 代码完成, 待人工 GUI 验收):
      根因 (读码确认, 共三处):
      ① `hash_status` 只哈希路径集合 — 已在列表里的文件内容再变, 哈希不变 → 后台刷新短路跳过,
         界面永远显示旧 diff (用户报的"改了代码不显示"主因);
      ② `populate_file_lists` 每次 splice 重建全部 ChangedFileObject, expanded 状态全丢 →
         刷新后展开的行塌回去; unbind 还会清空 diff 面板;
      ③ 只有 30s 定时器 + 窗口重获焦点两个触发点, 无文件系统监听。
      实现:
      a) `hash_status(status, root)` 增加每个变更路径的 size+mtime (`hash_entry`), 内容改动即换哈希;
      b) `setup_repo_watchers` (在 `load_repo_data` 挂载, 换仓库自动重定向): `gio::FileMonitor`
         目录级监听 workdir + `.git` + `refs` + `refs/heads` + `refs/tags`, 400ms debounce
         (`arm_monitor_debounce`, 每事件重置单发定时器, 事件汇聚成一次刷新);
      c) 监听事件/焦点/定时器统一汇入 `trigger_background_refresh`; `refresh_in_progress` 替换为
         `RefreshGate` (运行中的请求排队不丢, 完成后补跑);
      d) 展开状态跨刷新保留 (`populate_file_lists` 先收集 expanded 路径再回填); 行 re-bind 仍展开时
         经 `RowRenderCallback` → `render_row_diff` 从新缓存重渲染 (含 hunk 按钮重新接线);
         原 activation 渲染逻辑抽取为共享的 `render_row_diff`, 不重复实现。
      自动验收 (已通过): 单测 4 个 — `status_hash_sees_content_edits_of_same_paths` (内容改动换哈希)、
      `refresh_gate_queues_request_during_a_run` (刷新期事件不丢)、
      `monitor_debounce_collapses_event_bursts` (5 连发合并为 1 次 + 可复用)、
      `populate_preserves_expanded_rows_across_refresh` (展开状态跨刷新保留); `cargo test` 76 全绿。
      人工验收 (待用户 GUI 确认): 外部编辑器改文件 / 新增未跟踪文件 / 外部 `git commit` / 切分支,
      回到 GitPanel ≤1s 内变更列表与 diff 视图更新, 滚动位置和展开行不丢。
- [x] 5.2 **二进制 diff 提示一致性**: 修复 `diff_untracked` 是唯一不设 `is_binary` 的 diff 入口这一缺陷 (2026-09-23 完成):
      a) `utils/diff.rs` 增加统一的二进制检测 `looks_binary` (NUL 字节判定, 与 git 同口径),
         `diff_untracked` 改读字节, 二进制返回 `is_binary: true` 空 hunks;
         非二进制非 UTF-8 文本改 `from_utf8_lossy` 宽松解码, 不再 read_to_string 报错;
      b) `parse_diff` 改为先按 `diff.deltas()` 预置全部文件条目 (二进制文件可能 0 行 patch,
         旧逻辑靠 print 回调发现文件 → 二进制文件会整个消失), 打印时回填 hunk 并持续 OR 合并 BINARY 标志;
      c) `window.rs` `get_file_diff` 为 None 时渲染新 i18n 键 `diff_unavailable`, 不再静默空白;
      d) 全部 5 个 diff 入口已审查 (diff_untracked/unstaged/staged/commit/refs, 后四个共用 parse_diff)。
      验收 (已达成): 三条路径 ①已跟踪修改 ②未跟踪 ③commit 内 各有集成测试断言 `is_binary == true`
      (`tests/binary_diff_test.rs`, 共 7 例含对照组); UI 层 3 个测试断言渲染出 `binary_diff_not_supported`
      (diff_view unified/side-by-side + changes_view render_file_diff)。
      "不显示"的唯一合法情形: 二进制文件内容无变化 (如仅权限位变更)。

---

## 测试矩阵 (每个性能/功能改动都要按此过一遍)

| 维度 | 取值 |
|---|---|
| 仓库规模 | 空仓库 / 本仓库 (小, 59 文件) / 0.3 的大仓库样本 (≥5 万文件, ≥1 万 commit) |
| 工作区状态 | 干净 / 脏(多文件修改) / 有冲突 / rebase 中 / stash 后 |
| diff 规模 | 单行改动 / 500 文件批量改动 / 单文件 >1MB / 二进制文件 |
| 操作 | status 刷新、commit、log 图谱、diff 查看、branch/rebase/merge、clone/fetch、undo |
| 环境 | 有显示 (正常跑) / 无显示 ssh (`cargo test` 必须仍全绿) |
| 语言 | zh-CN / en (i18n 键不许因重构丢失: `grep -c '"[a-z_]*"' src/i18n.rs` 前后对比) |

性能场景固定脚本 (Phase 0 建好, 之后每次复用):
1. 冷启动到可交互 — 计时
2. 打开大仓库 → 首次 status 完成 — 计时
3. 连续 10 次手动刷新侧栏 — tracing span 最大值
4. 打开 1MB 文件 diff → 首帧 — 计时
5. 外部编辑器改文件 → 回到 GitPanel 列表更新 — 计时 (Phase 5.1, 目标 ≤1s)
6. 未跟踪二进制文件点击 → 是否显示提示 — 断言 (Phase 5.2)

---

## 停止条件 (达到即算完成, 不许继续加活)

- `cargo build` 与 `cargo clippy --all-targets` 均 0 警告
- `cargo test` ≥120 个用例全绿, `src/utils/` 无测试文件数 = 0, 无显示环境也全绿
- Phase 0 四个性能场景的数字全部有"优化前/优化后"对照, 且 3.1-3.5 的验收标准逐条达标
- Phase 5 两个用户报告问题的验收逐条通过: 外部改动 ≤1s 自动刷新 (含滚动/选中不丢),
  二进制 diff 三条路径 (已跟踪修改/未跟踪/commit 内) 均显示 `binary_diff_not_supported` 且有单测
- `scripts/perf-check.sh` 存在且能一条命令跑通
- 本文件所有 `- [ ]` 变 `- [x]`, 快照已更新为最终数字

阻塞项 (遇到就停下问用户, 不许自行绕路):
- 大仓库样本若需联网克隆, 先问用户给本地路径
- criterion/flamegraph/llvm-cov 若在 loong64 上装不上, 记录报错后降级为手工计时方案, 不硬折腾