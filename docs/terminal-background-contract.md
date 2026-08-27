# 终端后台运行与绘制契约

本文记录 NyaTerm 当前实现中的终端后台契约。它描述的是代码已经执行的行为，而不是未来设计提案。相关实现主要位于：

- `crates/nyaterm-transport/src/session_event_queue.rs`
- `crates/nyaterm-desktop/src/models/session_event_bridge.rs`
- `crates/nyaterm-desktop/src/models/terminal/mod.rs`
- `crates/nyaterm-desktop/src/features/terminal/terminal_runtime/buffer/mod.rs`
- `crates/nyaterm-desktop/src/features/terminal/terminal_runtime/view_io/mod.rs`
- `crates/nyaterm-desktop/src/features/terminal/terminal_surface_entity/mod.rs`
- `crates/nyaterm-terminal/src/lib.rs`
- `crates/nyaterm-terminal-gpui/src/element/mod.rs`

本文中的“后台”是工作区 presentation 状态，不等同于操作系统窗口被遮挡、最小化、切换虚拟桌面，也不等同于浏览器的 document visibility。

## 三态矩阵

权威状态为 `TerminalPresentation`。`is_visible` 表示会话是否拥有当前挂载的可见 terminal surface；`is_active` 表示是否为活动会话。

| 状态 | `is_active` | `is_visible` | 推进 parser/protocol | 每个 output frame 生成 live snapshot | 通知 terminal surface | active-only decorations |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `VisibleActive` | `true` | `true` | 是 | 是 | 是 | 是 |
| `VisibleInactive` | `false` | `true` | 是 | 是 | 是 | 否 |
| `Background` | 任意 | `false` | 是 | 否 | 否 | 否 |

必须保持以下不变量：

1. 三态都令 `parse_output = true`。后台优化只能省略派生的 snapshot、surface notify 和绘制工作，不能暂停权威 parser、grid、字符集流状态、协议状态、effects 或 revision。
2. 可见但非活动的 split pane 是 `VisibleInactive`，不是 `Background`；它仍需实时 snapshot 和 surface notify。
3. `Background` 由工作区的挂载/可见关系决定。当前实现没有把原生窗口遮挡或最小化映射为 `Background`，不得假定三个桌面平台具有浏览器式统一 visibility 语义。

## 端到端数据路径

### 1. Transport 读取与源队列

Local PTY、SSH、Telnet/Raw TCP、Serial 等 transport 产生 `SessionEvent::Output { session_id, data }`。`SessionEventQueue`：

- 将大 output 切为最多 256 KiB 的事件；
- 以 8 MiB 为 output 高水位、4 MiB 为低水位；
- 高水位后让生产者在条件变量上等待，消费降至低水位后恢复；
- 通过 producer-order 锁保证一次大事件的切片不会与另一个生产者交错。

这一层的背压目标是“不删除已经接受的终端字节”，不是按绘制帧丢弃旧输出。

### 2. Session event bridge 分流

`SessionEventBridge` 每轮最多 drain 512 个事件和 128 KiB output。普通 output 在满足以下条件时直接提交给 terminal frame pipeline：

- 会话没有被强制 UI route；
- frame pipeline 未达到 direct-output 背压水位；
- 当前字节不需要 ZMODEM/TRZSZ sideband probe。

需要 sideband/capture/transfer 等 UI 检查的 output 进入 bridge UI queue，由 GPUI event pump 处理后再批量提交 frame pipeline。event pump 在 dropped/cwd/command/exited/error 等边界事件前先 flush 已累积 output，以维持顺序。

bridge 发现压力时停止继续 drain transport source；已经 drain 的 chunk 仍会提交或排队，不因跨过水位而被裁剪。

### 3. Frame command queue 与 worker

`TerminalFramePipeline` 把非空 payload 作为 `TerminalFrameCommand::Output` 交给独立的 `nyaterm-terminal-frame-processor` 线程。

- command queue 软容量为 512。
- 相邻且 session、encoding、scrollback limit 相同的 output 可在入队时合并至 8 KiB。
- worker 只合并已经到达且形状相同的 output，一批最多继续累计至 128 KiB。
- worker 不等待未来 chunk；否则会人为增加按键回显和洪峰尾块延迟。
- 超过 command queue 容量时，只压缩/移除低优先级的非 priority snapshot 或 search 等派生请求；原始 output 和 lifecycle/resize/priority snapshot 不为维持容量而丢弃，因此队列必要时可暂时超过 512。

每个 worker session 持有权威 `TerminalScreen`、显示 decoder、录制 decoder、revision 和 snapshot-priority 状态。output 依次经过录制、过载保护、`TerminalScreen::advance`、visible-text tail、`screen.take_effects()`；parser/grid、ANSI/OSC、shell integration、graphics 和 PTY protocol reply 都在此处推进。

### 4. Worker event 到 GPUI state

worker 输出 `TerminalFrameOutputEvent`，其中包括：

- 最近的 visible-text tail；
- 可选 viewport snapshot；
- protocol state 和 effects；
- accepted/skipped byte 计数；
- revision、snapshot/process 统计。

GPUI drain 有事件数和 wall-time budget。可见会话应用 snapshot；后台会话即使没有 snapshot，也继续应用 protocol、revision、effects 和 skipped 状态。title/reset-title、cwd、shell command edges、OSC 52 clipboard、bell 和 PTY writes 的执行不以 surface 可见性为前提。

### 5. Snapshot 到绘制

`sync_terminal_surface_paint` 选择 worker live snapshot、scrollback cache 或 surface retained snapshot，并更新对应 `TerminalSurface`。surface 的 `Render` 构造 `NyaTerminalElement`；GPUI 再执行 layout/prepaint/paint。

`NyaTerminalElement::prepaint` 只 shape 当前 clip 内的可见行；完整热缓存时最多预取邻近一行。隐藏 tab 没有挂载 surface，因此不会进入这条绘制路径。终端输出通常只 notify 对应 surface；只有 unread/effects 等 chrome 状态需要时才触发 full-shell notify。

## 完整性边界：什么不是静默丢失

### 配置的 scrollback 淘汰

权威历史位于 worker 的 `TerminalScreen`/Alacritty grid，默认配置为 5,000 行。`set_scrollback_limit` 更新 `scrolling_history` 并同步 grid；超出用户配置历史上限的旧行由 ring/history 正常淘汰。关联的 line metadata 和 graphics placement 也只保留在当前物理 retained range 内。

这是公开、可配置的有界历史语义，不是后台性能优化造成的静默丢失。alternate screen 的 `scrollback_len()` 为 0。GPUI 侧最多 16 个 scrollback snapshots 只是可重建缓存，其淘汰也不删除权威 grid 历史。

### 显式 `> 1_000_000` bytes overload tail skip

代码常量 `TERMINAL_OUTPUT_VISIBLE_BACKLOG_CAP` 是十进制 **1,000,000 bytes**，不是二进制 1 MiB（1,048,576 bytes）。条件是严格的大于：

- `data.len() <= 1_000_000`：整块进入 terminal parser，`skipped_output_bytes = 0`。
- `data.len() > 1_000_000`：记录 `len - 1_000_000` 个 skipped bytes，仅将最后 1,000,000 bytes 交给显示 parser/grid。

skip 前会重置 terminal stream state 和显示 decoder，避免被保留尾部与已跳过的 UTF-8/GBK 多字节字符、ANSI、OSC 或 graphics 片段错误拼接。完整 chunk 在该保护之前已经送入 recording decoder/write path。

skip 会累计到 `skipped_output_bytes`，使 UI 进入 `Overloaded` 状态并显示保护提示；恢复后显示 3 秒 `Recovered` 提示，昂贵 decorations 需经过 400 ms calm window 才恢复。因此它是显式、可观测的保护边界，不得描述成“后台静默丢 output”。字段 `skipped_output_chars` 保留了历史命名，但这里累计的是 bytes。

### 事件压缩不是 parser 状态丢失

worker 在产生 frame event 前已经推进权威 parser/grid。event queue 可以把同一 session 的多个无 effect output frame 压缩为最新 frame，并合并 recording byte count、accepted/skipped byte count、visible-text tail 和 process duration。被替换的是中间 UI frame/snapshot，不是尚未解析的 transport bytes 或最终 terminal state。

任何真正的 queue rejection、poison 或 overload skip 都必须保留 error/计数/overlay 等显式信号；不能把这些路径宣传为无条件 lossless。

## 高低水位与容量

| 层 | 高水位/容量 | 低水位 | 行为 |
| --- | ---: | ---: | --- |
| transport `SessionEventQueue` output bytes | 8 MiB | 4 MiB | 生产者等待；每个 output event 最多 256 KiB |
| bridge → frame direct queued bytes | 2 MiB | 1 MiB | 暂停 source drain |
| bridge UI queued output bytes | 1 MiB | 512 KiB | 暂停 source drain；不裁掉超过旧 1 MiB limit 的 output |
| frame command queue | 512 commands | 无 | 优先压缩低优先级派生工作；必要时可临时超容，不丢 output |
| frame event queue | 1,024 events | consumer drain | 压缩/淘汰无 effect output；全为关键事件时生产者等待，drain 或 close 唤醒 |

bridge 使用滞回：未处于 backpressure 时，任一 gauge `>=` 高水位即暂停；已经 backpressured 时，只要任一 gauge `>` 对应低水位就继续暂停，两个 gauge 都 `<=` 低水位才恢复。这样避免阈值附近反复启停。

transport queue 的 8/4 MiB 滞回在正常过载期间保持无损：到达高水位的 producer 等待，只有 drain 到低水位才恢复。shutdown 是显式边界：`SessionManager::close(session_id)` 先取消该 session、清除其已排队事件并唤醒所有等待者，再关闭 transport 和 join reader；取消后的 `Output`、`Exited`、`Error` 等事件全部拒绝，不会阻塞或进入 queue，也不影响其他 session 继续 push/drain。`SessionManager` 整体析构先全局关闭 queue 并唤醒 producer/consumer，再关闭和 join 全部 session；全局关闭后拒绝新事件，已排队的非取消事件仍可 drain，空 queue 上的 blocking consumer 立即返回。完整 producer event 的 chunk 仍不交错，但取消和全局关闭的状态变更与唤醒不依赖 producer 顺序占用。

这些水位与单 chunk 的 1,000,000-byte overload tail protection 是不同层次的机制：前者调节生产/消费速率，后者处理已经作为单个 worker chunk 到达的极端输入。

## Worker snapshot omission 与恢复

snapshot priority 集合来自全部可见 session ID，而不是只有 active session。因此 `VisibleActive` 和 `VisibleInactive` 都是高优先级；`Background` 是低优先级。

后台 output 的契约是：

1. 继续解析全部未触发显式 overload skip 的字节；
2. 继续更新 protocol state、effects、command-running 和 revision；
3. output event 的 `snapshot` 为 `None`，snapshot duration/stats 为零；
4. UI 可释放重型 `frame_snapshot` 和 action-link cache；
5. 不 notify 未挂载的 terminal surface。

恢复路径如下：

1. 会话变为可见时，snapshot priority 集合会包含它。
2. 若可见 output 暂时仍没有 snapshot，UI 先应用 protocol/revision，并立即请求 offset 0 live snapshot。
3. `drive_terminal_render_requests` 也会扫描“可见、live offset 为 0、`frame_snapshot` 缺失”的会话并补发请求；`pending_snapshot_offsets` 对相同请求去重。
4. worker 返回显式 snapshot 后，UI 拒绝 grid geometry 不匹配或 revision 落后于当前状态的结果。
5. 合格的 offset 0 snapshot 通过 `apply_terminal_live_snapshot_frame` 恢复绘制；revision 必须追上后台最终状态。

因此 omission 只省略派生 viewport 物化，不省略后台状态演进。priority 切换和 snapshot 请求之间允许短暂窗口，恢复逻辑必须保留。

## Effect-bearing event queue 策略

worker → GPUI event queue 容量为 1,024。只有不含以下任何 effect 的 `Output` 才可在压力下压缩或淘汰：

- bell；
- title 或 reset-title；
- cwd；
- shell command started/finished；
- PTY writes/protocol replies；
- clipboard store/load。

`Snapshot` 和 `Search` reply 也不可压力淘汰。新 pure-output frame 入队前，可以压缩同 session 已排队的 pure-output frames；effect-bearing frame 不参与压缩，因而维持与周围事件的顺序。

队列已满时，先移除队列中最早的 droppable pure-output frame。若队列全部为 effect-bearing output、snapshot 或 search reply，则生产 worker 在不持有其他 pipeline 锁的情况下等待 consumer drain；`drain_into` 移出事件后通知等待者，producer 醒来后按原 FIFO 顺序入队。因此正常运行期间 critical event 和 snapshot/search reply 不会因容量压力被拒绝，pending snapshot marker 最终可由 reply 清理。

等待可被取消：`TerminalFrameEventQueue::close` 设置 closed 状态并通知所有 producer；被唤醒的 push 返回 `Closed` 且不入队。`TerminalFramePipeline` 使用独立于 queue clone 的 handle 计数，最后一个 pipeline handle drop 时显式 close，所以 frame worker 自己持有的 queue clone 不会让关闭条件永远无法满足。`Closed` 是正常 shutdown，不记录 overload/error；mutex poison 仍记录 `terminal_frame_event_queue_poisoned` error。

output/snapshot/search 各自拥有 wake-interest bit；data-plane drain arm `ALL`。消费 output wake 不得同时清除 snapshot/search interest，否则安静终端的恢复 snapshot 可能被永久搁置。

## 自动化验证：确定性断言优先

### 可进入 CI 的契约断言

以下测试不以机器速度、sleep 或 wall-clock 阈值判定成功：

- `terminal_presentation_work_policy_is_total_and_distinguishes_visible_inactive`：三态策略完整且区分可见非活动 pane。
- `bridge_pauses_source_drain_with_high_low_watermark_hysteresis`：bridge 高低水位和边界比较符。
- `bridge_ui_queue_preserves_output_above_the_old_limit`：UI queue 超过旧 1 MiB limit 后仍按序保留 output，dropped bytes 为零。
- `terminal_frame_pipeline_background_chunks_are_deterministic_after_priority_snapshot`：96 个固定后台 chunks 不产生 snapshot、accepted bytes 精确；恢复 snapshot 的 revision、cursor、geometry 和逐行 signature 与独立 reference screen 一致。
- `terminal_frame_event_queue_coalesces_pure_output_to_latest`、`terminal_frame_event_queue_preserves_output_effects`、`terminal_frame_event_queue_waits_for_room_without_reordering_critical_events`、`terminal_frame_event_queue_close_releases_waiting_push_without_enqueuing`、`terminal_frame_event_queue_delivers_snapshot_reply_after_critical_pressure`：pure output 压缩、effect 保序、critical backpressure、close cancellation 和 snapshot reply 最终到达。
- `layout_cache_stats_report_local_deltas_without_resetting_cache`：实例级累计 stats 与 `delta_since` 语义。
- `real_window_draw_renders_visible_inactive_but_skips_unmounted_surface`：通过真实 `TestAppContext/open_window/window.draw` 证明可见 inactive surface 会 shape、热 viewport 不新增 shape、unmount 后 draw delta 全零、remount 后 revision 追上。

定向复现命令：

```powershell
cargo test -p nyaterm-desktop --locked terminal_presentation_work_policy_is_total_and_distinguishes_visible_inactive
cargo test -p nyaterm-desktop --locked bridge_pauses_source_drain_with_high_low_watermark_hysteresis
cargo test -p nyaterm-desktop --locked bridge_ui_queue_preserves_output_above_the_old_limit
cargo test -p nyaterm-desktop --locked terminal_frame_pipeline_background_chunks_are_deterministic_after_priority_snapshot
cargo test -p nyaterm-desktop --locked terminal_frame_event_queue_preserves_output_effects
cargo test -p nyaterm-terminal-gpui --locked layout_cache_stats_report_local_deltas_without_resetting_cache
cargo test -p nyaterm-desktop --locked real_window_draw_renders_visible_inactive_but_skips_unmounted_surface
```

仓库 CI 的准确 Rust 命令为：

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked
cargo test --workspace --locked --no-fail-fast
```

CI 在 Linux x64、macOS arm64、Windows x64 上运行 workspace tests；这证明代码和确定性断言，不证明真机 GPU、IME、窗口遮挡、睡眠恢复或各显示缩放组合。

### 本契约对应实现阶段的验证记录

实现阶段记录的命令与结果如下；这些命令不包含 wall-clock pass/fail 阈值：

```powershell
cargo test -p nyaterm-terminal-gpui --lib
# 130 passed, 1 ignored

cargo test -p nyaterm-desktop terminal --lib
# 441 passed, 4 ignored

cargo test -p nyaterm-desktop --lib real_window_draw_renders_visible_inactive_but_skips_unmounted_surface
cargo check -p nyaterm-terminal-gpui
cargo check -p nyaterm-desktop
cargo fmt --all -- --check
git diff --check
```

## Wall-clock benchmark 只用于调查

以下测试带 `#[ignore = "performance benchmark; run manually with --ignored --nocapture"]`。它们打印 `Instant::elapsed()`，没有稳定时间阈值，默认 CI 不运行：

```powershell
cargo test -p nyaterm-desktop --release --locked dense_action_link_selection_drag_benchmark -- --ignored --nocapture --test-threads=1
cargo test -p nyaterm-desktop --release --locked overview_marker_fast_scroll_benchmark -- --ignored --nocapture --test-threads=1
cargo test -p nyaterm-desktop --release --locked selected_occurrence_search_large_scrollback_benchmark -- --ignored --nocapture --test-threads=1
cargo test -p nyaterm-desktop --release --locked root_render_hundred_sessions_eight_terminal_leaves_benchmark -- --ignored --nocapture --test-threads=1
```

这些命令退出 0 只表示测试逻辑未 panic，不表示性能达标。调查记录必须同时保存 commit、OS/架构、CPU/GPU、显示缩放、字体、构建 profile、负载、样本数和原始输出；不得把不同机器或 debug/release 数字直接比较。CI 回归 gate 应优先使用 accepted bytes、revision、shape calls、cache delta、snapshot omission 等确定性工作量/状态不变量，而不是脆弱的 elapsed 阈值。

## `NYATERM_GPUI_PERF` 与 PowerShell

`NYATERM_GPUI_PERF` 只有值严格等于字符串 `1` 时启用，并由 `OnceLock` 缓存，所以必须在进程启动前设置。采样按 key 聚合至少 1 秒，以 `debug` 级别记录 `diagnostic = "gpui_perf"`，包含 count、avg/p95/max、cache hit、full-shell/surface paint counts 等上下文。

只设置 `NYATERM_GPUI_PERF=1` 不够：默认 `RUST_LOG` 不显示该 debug target。PowerShell 中使用：

```powershell
$env:NYATERM_GPUI_PERF = '1'
$env:RUST_LOG = 'warn,nyaterm=info,nyaterm_core=info,nyaterm_transport=info,nyaterm_desktop::features::perf=debug'
try {
    cargo run -p nyaterm-app --bin nyaterm
} finally {
    Remove-Item Env:NYATERM_GPUI_PERF -ErrorAction SilentlyContinue
    Remove-Item Env:RUST_LOG -ErrorAction SilentlyContinue
}
```

纯本地终端调查不需要 RDP/VNC helper。若同一次手工验收还要打开 RDP/VNC，应先运行：

```powershell
cargo build -p nyaterm-rdp-helper -p nyaterm-vnc-helper
```

perf samples 是各调用点包围的 Rust 代码段耗时，不应一概解释为完整 GPUI draw/layout/paint。绘制契约以真实 `window.draw()` 测试和 layout-cache stats 为准。

## Windows/macOS/Linux 手工验收矩阵

每项记录 OS/架构、显示后端与缩放、shell/transport、输入法、presentation 状态、负载字节数、预期、结果和相关 diagnostic。

| 场景 | Windows | macOS | Linux | 通过标准 |
| --- | --- | --- | --- | --- |
| 本地 shell 与 resize | PowerShell、cmd；重点检查本地 PTY echo/working directory/resize | zsh/bash/fish login shell；Retina resize | bash/zsh、`/bin/sh`；X11 与 Wayland 各一次 | rows/cols/pixel dimensions 与 TUI 一致，无 stale geometry |
| 三态与 split | 两个 split 同时输出，再切 tab | 同左 | 同左 | 非焦点可见 pane 实时更新；后台 tab 返回后最终 grid/revision/title/cwd 正确 |
| 大输出与背压 | 1 KiB、256 KiB、超过 8 MiB burst，期间持续键入 | 同左 | 同左 | echo 仍可用，洪峰尾块出现；无未报告截断；显式 skip 有计数/overlay |
| scrollback | 配置小 history 后持续输出和回滚 | 同左 | 同左 | 只按配置淘汰；滚离底部时 new-output 状态正确，回到底部恢复 live |
| alternate screen/TUI | PowerShell/cmd 中可用 TUI | 常用 TUI | 常用 TUI | 进入/退出 alternate screen、mouse mode 后主历史和输入恢复 |
| Unicode/IME | CJK IME、emoji、combining、125%/150% DPI | CJK IME，`interaction_mac_ime_compatibility` 开/关，Retina/非 Retina 切换 | X11/Wayland IME、fractional scaling | marked text 不提前发送，commit 一次；候选窗、双宽字符、光标对齐 |
| 键盘/鼠标协议 | Ctrl/Alt/Win 保留、Kitty、application cursor/keypad | Option-as-meta 与系统快捷键、Kitty | Alt/Meta 在 X11/Wayland、Kitty | TUI 收到正确 press/repeat/release；退出 mouse mode 后本地选择恢复 |
| 最小化/遮挡/桌面切换 | 多显示器、最小化、休眠恢复 | Hide/最小化/Spaces/睡眠恢复 | 遮挡、workspace 切换、X11/Wayland | 恢复后 parser 状态连续、无黑屏；不得假定等价于 `Background` |
| 光标与洪峰恢复 | 观察静止 prompt 与洪峰后恢复 | 同左 | 同左 | 静止时约 530 ms 半周期；压力期可暂停 phase，但停止后恢复且不锁到 60/120 Hz |
| 远端 resize | SSH、Telnet NAWS | SSH、Telnet NAWS | SSH、Telnet NAWS | 快速连续 resize 后远端 TUI 与本地 snapshot geometry 一致 |

Windows 的 local PTY echo/working-directory/resize 集成测试在仓库中有平台跳过，因此真机验收优先级更高。Linux/macOS 虽有非 Windows local PTY 自动化覆盖，GPU、IME、compositor、DPI/Retina 和窗口生命周期仍必须手工验证。

### 三平台手工验收记录（待填写）

以下表格只是记录模板，不表示任何平台已经执行或通过。每次验收应填写实际 commit、环境、输入法、构建或安装包来源，并在结果中附失败现象或 issue 链接；不要用自动化测试结果代填。

| Commit | OS/架构 | 显示后端/缩放 | 输入法 | 构建/安装包 | 结果 |
| --- | --- | --- | --- | --- | --- |
| `<填写>` | Windows `<版本/架构>` | `<显示器/GPU/缩放>` | `<填写>` | `<本地构建或包名>` | `<通过/失败/未执行；证据>` |
| `<填写>` | macOS `<版本/架构>` | `<显示器/Retina/缩放>` | `<填写>` | `<本地构建或包名>` | `<通过/失败/未执行；证据>` |
| `<填写>` | Linux `<发行版/架构>` | `<X11/Wayland/GPU/缩放>` | `<填写>` | `<本地构建或包名>` | `<通过/失败/未执行；证据>` |

## 为什么不照搬浏览器 visibility/rAF

1. **生命周期对象不同。** NyaTerm 是原生 GPUI 应用，没有 DOM、`document.visibilityState` 或 `requestAnimationFrame`。当前可见性是 workspace pane/split 挂载状态，不是统一的 OS occlusion 状态。
2. **parser 与 paint 必须解耦。** 后台仍要维护跨 chunk 字符编码、ANSI/OSC、Kitty/Sixel、shell integration、alternate screen、title/cwd、clipboard 和 PTY replies。以“页面 hidden”暂停 parser 会破坏权威状态。
3. **数据面由 producer wake 驱动。** transport、bridge 和 frame worker 在数据到达时唤醒；GPUI 通过 entity notify 调度 native layout/prepaint/paint。再增加 rAF 会形成第二个帧调度器，并把数据延迟错误绑定到 60/120 Hz。
4. **低延迟要求不等待下一帧。** worker 只合并已经到达的字节，明确不等待未来 chunk；浏览器帧节拍会增加按键回显与洪峰尾块延迟。
5. **可借鉴策略，不复制 API。** “不可见时省 snapshot/paint”和“UI apply 使用预算”已经映射为 `TerminalPresentation`、snapshot priority、event wake 和 GPUI notify，应在这些原生边界上演进。

## 为什么不照搬 xterm write buffer

1. NyaTerm 没有 xterm.js/`@xterm` 依赖；不能假设其字符串、callback、时间片或浏览器 event-loop 语义适用于 Rust 原生线程模型。
2. 当前有四个不同责任层：用户输入 writer、transport 原始 output queue、frame command queue、worker→GPUI event queue。每层拥有不同的排序、背压、effects 和可丢派生数据契约，不能压成单一 write buffer。
3. 原始 output 是 bytes，可能包含非 UTF-8 编码和 graphics payload；必须经过流式 charset/graphics/parser 状态，不能先按 JavaScript string/chunk 语义截断。
4. batch key 包含 session、encoding 和 scrollback limit，禁止跨 session/encoding 合并；effects 和 protocol replies 也要求保序。
5. 当前 8 KiB enqueue merge、128 KiB worker burst、snapshot omission 和 wake/pacing 都是针对 NyaTerm owned snapshot 成本与输入 echo 调整的。可借鉴 xterm 的测量维度，但阈值或算法必须由本仓库的确定性断言和三平台手工数据证明。

变更上述契约时，应同时更新本文、相邻行为测试和平台验收记录；不得仅以 wall-clock benchmark 或单平台视觉观察替代完整性断言。