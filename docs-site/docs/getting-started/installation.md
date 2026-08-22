---
sidebar_position: 1
---

# 安装指南

## 系统要求

NyaTerm 支持以下操作系统：

- **Windows** 10/11（x64 / ARM64）
- **macOS** 12+（Intel / Apple Silicon）
- **Linux**（x64 / ARM64；Ubuntu 20.04+、Fedora 36+、Arch Linux 等）

NyaTerm 用 GPUI 做原生 GPU 渲染，因此对图形环境有要求：

- **Linux**：需要可用的 Vulkan 驱动（例如 `libvulkan1` 和 `mesa-vulkan-drivers`），以及 X11 或 Wayland 会话。缺少 Vulkan 驱动时应用无法启动
- **macOS**：使用 Metal，系统自带
- **Windows**：使用系统图形驱动，通常无需额外安装

如果你在纯 SSH 的无头服务器上，NyaTerm 不适用——它是桌面客户端，不是终端复用器。

## 下载安装

### 从发布页面下载

前往 [Releases](https://github.com/nyakang/nyaterm/releases) 页面，下载适合你操作系统的安装包：

| 平台 | 安装包格式 |
|------|-----------|
| Windows | 安装版 `-setup.exe` / 便携版 `.zip` |
| macOS | `.dmg` / `.app.tar.gz` |
| Linux | `.deb` / `.AppImage` / `.rpm` |

Windows 便携版解压后运行 `NyaTerm.exe` 即可。**Help → 检查更新** 会检查 GitHub Releases，并提供 Releases 页面入口；当前不会自动下载或替换程序文件。更新便携版时，请关闭 NyaTerm 后手动替换程序文件，并保留同目录下的 `data/`。

请从 [Releases](https://github.com/nyakang/nyaterm/releases) 选择最新的 Windows 便携版（x64 或 ARM64）。文档不维护固定版本的直链，以免链接和版本过期。

### macOS

macOS 用户可以通过 Homebrew 安装 NyaTerm：

```bash
brew install nyakang/nyaterm/nyaterm
```

该命令会使用 [`nyakang/homebrew-nyaterm`](https://github.com/nyakang/homebrew-nyaterm) tap，并安装 `nyaterm` cask。也可以从 [nyaterm.app](https://nyaterm.app) 或 [Releases](https://github.com/nyakang/nyaterm/releases) 下载 `.dmg` 安装包，然后将 NyaTerm 拖入 `/Applications`。

NyaTerm 目前还没有使用 Apple Developer 证书签名。安装后如果 macOS 提示应用已损坏或无法打开，可以移除 quarantine 属性后再打开：

```bash
sudo xattr -cr /Applications/NyaTerm.app
```

### 从源码构建

如果你想从源码构建，请参考 [开发环境搭建](../development/setup) 章节。

## 首次启动后会看到什么

安装完成后启动 NyaTerm，主窗口通常会由这些区域组成：

- **顶部菜单与窗口栏** — File / View / Help、窗口控制与应用级入口
- **中央工作区** — 终端标签页，以及标签内横向 / 纵向分屏
- **左侧活动栏与面板** — 文件浏览器、网络、Security/Auth、云同步、设置等能力入口
- **右侧活动栏与面板** — 已保存连接、AI Assistant、活动会话、命令历史、资源监控等运行态信息
- **底部辅助区** — 快捷命令、串口发送、录制、锁屏等辅助操作

某些流程会打开独立子窗口，而不是打断主工作区，例如：

- 设置
- 新建连接
- 快捷命令编辑
- 自动上传提示

## 安装后建议先检查的设置

建议在正式投入使用前，先快速浏览以下几项：

- **设置 → 常规**：启动恢复、关闭时最小化到托盘、关闭确认
- **设置 → 常规**：日志级别、日志保留时间、打开日志目录、导出诊断包
- **设置 → 交互**：命令建议开关、历史命令长度范围、复制与右键粘贴、macOS IME 兼容
- **设置 → 终端**：回滚缓冲区、Keep-Alive、动作链接、行号 / 时间戳、关键词高亮、资源监控、工作区间距、字体粗细、图片路径粘贴
- **设置 → 传输**：默认下载目录、默认编辑器、录制保存路径、并发、重试与重复目标策略
- **设置 → 安全**：主密码、锁屏、空闲自动锁定、主机密钥策略
- **设置 → AI**：provider、模型、风险控制、历史记录与上下文限制

如果你经常在后台保留会话或同步任务，**关闭时最小化到托盘** 值得优先确认。

## 如果你要迁移旧环境

如果你之前已经在其他工具中维护会话，安装后可以直接导入：

- **Xshell**（`.xts`）
- **MobaXterm**（`.mxtsessions`）
- **WindTerm**（`.sessions`）
- **SecureCRT**（`.xml`）
- **FinalShell**（`conn` 目录）
- **Termius**（本机 IndexedDB）
- **NyaTerm / Electerm JSON**（`.json`）
- **NyaTerm** 的 `.nya` 加密配置备份

如果你的目标是完整恢复 NyaTerm 配置，优先使用 `.nya`；它会恢复的不只是连接，还包括更多本地配置数据。

需要注意：

- `.nya` 导入 / 导出需要先设置主密码
- 导入 `.nya` 后通常需要重启应用
- 普通会话导入更适合迁移连接列表，不会替代完整配置恢复

## 第一次体验建议

如果你是第一次使用，建议按这个顺序体验：

1. 打开 [快速开始](./quick-start)
2. 创建一个 **SSH** 连接，或先导入已有会话
3. 再创建一个 **本地终端**，体验混合工作区
4. 在 SSH 会话里打开文件浏览器和传输面板
5. 试试命令历史、快捷命令、AI 助手、录制与终端搜索
6. 如果你在 Windows 上，可额外尝试把本地文件或文件夹直接拖到文件浏览器中上传
