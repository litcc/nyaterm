# 开发环境搭建

NyaTerm 应用本身是 Cargo workspace。Node.js 和 pnpm 只用于构建本仓库中的 Docusaurus 文档站点。

## 应用开发前置要求

### Rust 与 Git

安装最新稳定版 Rust：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Windows 用户可从 [rustup.rs](https://rustup.rs/) 安装。所有平台还需要 Git 和目标平台的原生编译工具链。

### 平台依赖

#### Windows

安装 [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)，并选择“使用 C++ 的桌面开发”工作负载。

#### macOS

安装 Xcode 和命令行工具：

```bash
xcode-select --install
```

GPUI 在 macOS 上使用 Metal，因此需要可用的 macOS SDK。

#### Linux（Ubuntu / Debian）

安装 Rust crate 编译工具、字体/窗口系统开发库和 Vulkan loader：

```bash
sudo apt update
sudo apt install build-essential clang pkg-config cmake \
  libfontconfig1-dev libfreetype6-dev libssl-dev libudev-dev \
  libwayland-dev libx11-dev libx11-xcb-dev \
  libxcb-cursor-dev libxcb-icccm4-dev libxcb-image0-dev \
  libxcb-keysyms1-dev libxcb-randr0-dev libxcb-render0-dev \
  libxcb-shape0-dev libxcb-xfixes0-dev libxcb-xinerama0-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libzstd-dev \
  libvulkan1 mesa-vulkan-drivers
```

运行桌面应用还需要可用的 Vulkan 驱动，以及 X11 或 Wayland 会话。

## 获取源码

```bash
git clone https://github.com/nyakang/nyaterm.git
cd nyaterm
```

应用依赖全部由 Cargo 管理。仓库里的 Node.js 依赖只属于 `docs-site`。

## 启动应用

```bash
cargo run -p nyaterm-app --bin nyaterm
```

首次编译会构建 GPUI 和全部依赖，耗时明显长于后续增量构建。

### RDP / VNC 需要先构建 helper

RDP 和 VNC 各自运行在独立的 helper 进程里，应用在自己的可执行文件旁边查找它们。上面的 `cargo run -p nyaterm-app` **只构建应用**，因此两个协议都会以 `HelperMissing` 失败。先构建 helper：

```bash
cargo build -p nyaterm-rdp-helper -p nyaterm-vnc-helper
```

不带 `-p` 的 `cargo build` 或 `cargo check` 会覆盖三者，它们是 workspace 的 `default-members`。

`NYATERM_RDP_HELPER` 和 `NYATERM_VNC_HELPER` 可以用显式路径覆盖查找结果，便于指向另一个 target 目录里的构建产物。

## 常用检查

迭代时优先运行受影响 crate 的检查：

```bash
cargo check -p nyaterm-app
cargo test -p <crate-name>
```

提交评审前运行相关 workspace 检查：

```bash
cargo check --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
```

`cargo fmt --all` 会写回格式化结果，仅在准备应用格式变更时运行。

## Release profile 构建

```bash
cargo build -p nyaterm-app --bin nyaterm --release
```

原生二进制位于 `target/release/nyaterm`，Windows 下为 `target/release/nyaterm.exe`。该命令只构建应用二进制，既不构建 helper，也不生成安装包。

发布包由 `scripts/release/package_native.py` 生成，它负责把 helper 放到应用旁边，并按平台产出 `-setup.exe` / `_portable.zip`、`.dmg` / `.app.tar.gz`、`.AppImage` / `.deb` / `.rpm`。新增 helper 时必须同时更新该脚本的 `HELPER_BINS` 列表。

## 文档站开发

编辑 `docs-site` 时另外安装 Node.js 18+ 和 [pnpm](https://pnpm.io/)：

```bash
pnpm --dir docs-site install
pnpm --dir docs-site start:zh
```

英文文档开发服务器：

```bash
pnpm --dir docs-site start:en
```

构建全部 locale：

```bash
pnpm --dir docs-site build
```

构建会检查页面和 sidebar，Markdown 链接问题按站点配置报告。

构建不会发现的问题由一个单独的脚本检查：

```bash
python3 scripts/ci/check_docs_translations.py
```

它校验每个页面都有中英两份、两份的标题数一致（用来发现整节漏译）、每个页面都被 `sidebars.ts` 引用，以及没有把撰稿备注留在正文里。CI 的 `Documentation site` job 会跑这个脚本再构建全部 locale。

注意脚本比对的是标题数量，不校验译文措辞。

## 修改第三方依赖

NyaTerm 打了补丁的第三方依赖**没有 vendor 到仓库里**。每个都是 [github.com/nyakang](https://github.com/nyakang) 下 fork 的 `nyaterm` 分支上的一条补丁序列，由根 `Cargo.toml` 固定 revision 消费：`alacritty`、`gpui-component`、`IronRDP`、`russh`、`russh-sftp`、`sspi-rs`、`vnc-rs`、`zed`（`gpui`）和 `zmodem2`。

改动流程是：提交到 fork 分支 → 推送 → 在根 `Cargo.toml` 里 bump revision。补丁按关注点拆分而不是压成一个提交，并在提交信息和该分支的 `NYATERM.md` 里记录原因与验证方式。已有序列优先 rebase 到更新的上游 revision，而不是不断累积快照。

`temp/vendor/` 下是这些源码的只读本地副本，仅供阅读。**它不参与编译，改动那里既不会生效也不会报错。**

## 开发约定

- 先阅读根目录 `AGENTS.md` 和 `CONTRIBUTING.md`。
- UI 状态与视图放在 `nyaterm-desktop`，共享控件放在 `nyaterm-ui`。
- transport、terminal 和 core crate 保持独立于 GPUI。
- 新增 UI 文本时同步更新 `crates/nyaterm-desktop/src/i18n/locales/` 下的中英文文件。
- 不要在测试、日志或诊断数据中使用真实凭据。
