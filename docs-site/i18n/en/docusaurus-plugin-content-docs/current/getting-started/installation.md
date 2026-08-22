---
sidebar_position: 1
---

# Installation

## System Requirements

NyaTerm supports the following operating systems:

- **Windows** 10/11 (x64 / ARM64)
- **macOS** 12+ (Intel / Apple Silicon)
- **Linux** (x64 / ARM64; Ubuntu 20.04+, Fedora 36+, Arch Linux, and similar distributions)

NyaTerm renders natively on the GPU through GPUI, so it has graphics requirements:

- **Linux**: a working Vulkan driver (for example `libvulkan1` and `mesa-vulkan-drivers`) plus an X11 or Wayland session. The application will not start without a Vulkan driver
- **macOS**: uses Metal, which ships with the system
- **Windows**: uses the system graphics driver; usually nothing extra is needed

NyaTerm is a desktop client, not a terminal multiplexer, so it is not applicable on a headless SSH-only server.

## Download and install

### From releases

Visit the [Releases](https://github.com/nyakang/nyaterm/releases) page and download the installer for your OS:

| Platform | Format |
|----------|--------|
| Windows | installer `-setup.exe` / portable `.zip` |
| macOS | `.dmg` / `.app.tar.gz` |
| Linux | `.deb` / `.AppImage` / `.rpm` |

For the Windows portable edition, extract the zip and run `NyaTerm.exe`. **Help → Check Updates** checks GitHub Releases and provides a Releases-page entry; it currently does not download or replace program files automatically. To update a portable build, close NyaTerm, replace the program files manually, and keep the adjacent `data/` directory.

Choose the latest Windows portable asset (x64 or ARM64) from [Releases](https://github.com/nyakang/nyaterm/releases). This page intentionally avoids fixed-version asset links so they do not become stale.

### macOS

macOS users can install NyaTerm with Homebrew:

```bash
brew install nyakang/nyaterm/nyaterm
```

This uses the [`nyakang/homebrew-nyaterm`](https://github.com/nyakang/homebrew-nyaterm) tap and installs the `nyaterm` cask. You can also download the `.dmg` installer from [nyaterm.app](https://nyaterm.app) or [Releases](https://github.com/nyakang/nyaterm/releases), then drag NyaTerm into `/Applications`.

NyaTerm is currently not signed with an Apple Developer certificate. If macOS reports that the app is damaged or cannot be opened after installation, remove the quarantine attribute and open it again:

```bash
sudo xattr -cr /Applications/NyaTerm.app
```

### Build from source

If you prefer to build NyaTerm yourself, see [Development Setup](../development/setup).

## What you see on first launch

After installation, the main window is typically organized into these areas:

- **Top menu and window bar** — File / View / Help and window controls
- **Central workspace** — terminal tabs and split panes inside the active tab
- **Left activity bar and panels** — file explorer, network, Security/Auth, Cloud Sync, settings, and related capability entry points
- **Right activity bar and panels** — saved connections, AI Assistant, active sessions, command history, and resource monitor
- **Bottom helper area** — quick commands, serial send, recording, and lock actions

Some workflows open dedicated child windows instead of interrupting the main workspace, such as:

- Settings
- New session / connection creation
- Quick command editing
- Remote-file editing
- Auto-upload prompts

## Settings worth checking after install

Before using NyaTerm long term, quickly review:

- **Settings → General**: startup restore, minimize to tray when closing, close confirmation
- **Settings → General**: log level, log retention, open log directory, export diagnostics bundle
- **Settings → Interaction**: command suggestions, history-command length filters, copy, right-click paste, macOS IME compatibility
- **Settings → Terminal**: scrollback, Keep-Alive, action links, line numbers / timestamps, keyword highlighting, resource monitor, workspace padding, font weight, image path paste behavior
- **Settings → Transfer**: default download directory, default editor, recording path, concurrency, retry, duplicate-target strategy
- **Settings → Security**: master password, screen lock, idle auto-lock, host key policy
- **Settings → AI**: providers, models, risk controls, history, and context limits

If you often keep sessions or sync tasks running in the background, check **Minimize to tray when closing** early.

## Migrating an existing environment

NyaTerm can import sessions from:

- **Xshell** (`.xts`)
- **MobaXterm** (`.mxtsessions`)
- **WindTerm** (`.sessions`)
- **SecureCRT** (`.xml`)
- **FinalShell** (`conn` directory)
- **Termius** (local IndexedDB)
- **NyaTerm / Electerm JSON** (`.json`)
- NyaTerm encrypted backup files (`.nya`)

Use `.nya` when you need to restore the broader NyaTerm configuration, not only a connection list. `.nya` import and export require a master password.

## Suggested first run

For a first pass through the app, try this order:

1. Open [Quick Start](./quick-start)
2. Create one **SSH** connection
3. Create one **Local Terminal** to experience the mixed workspace model
4. Open the file explorer and transfer queue in the SSH session
5. Try command history, quick commands, AI Assistant, recording, and terminal search
6. On Windows, also try dragging local files or folders into the file explorer for upload
