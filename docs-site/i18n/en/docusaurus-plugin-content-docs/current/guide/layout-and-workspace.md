# Layout & Workspace

NyaTerm is built around a composable workspace rather than a single terminal tab. You can open multiple sessions, split panes inside a tab, and keep common tools docked around the sides of the app.

## Workspace areas

A typical workspace is made up of these areas:

- **Center area** — tabs and terminal panes
- **Left activity bar / panels** — file explorer, network, Security/Auth, Cloud Sync
- **Right activity bar / panels** — saved connections, active sessions, command history, resource monitor
- **Bottom helper area** — quick commands, serial send, recording, lock actions

These areas are not isolated pages. They cooperate around the currently active session.

## Tabs

Each tab can hold a session, and each tab can also be split into multiple panes.

Common tab actions include:

- Creating a new session
- Closing the current tab
- Switching between tabs
- Renaming a tab
- Setting a tab color
- Duplicating the current session, optionally with a startup command
- Dragging a tab into another dock/split area
- Reconnecting a session
- Viewing session details

This makes NyaTerm a good fit for separating:

- Different environments
- Different projects
- Different task phases

## Command Palette

The **Command Palette** is a searchable overlay opened with `Ctrl / Cmd + Shift + S` (labeled **Open Command Palette** in settings). It searches three kinds of entries:

- **Active sessions** — tabs you already have open
- **Pending sessions** — sessions that were started but have not connected yet, including the failure reason
- **Saved connections** — entries from your connection list; selecting one starts the connection

Use the arrow keys to select and Enter to open. When you have many tabs and connections open, this is the fastest way to navigate.

The palette only searches sessions and connections. It does **not** contain app-level actions such as opening settings, toggling panels, or locking the screen — those live in the top menu and keyboard shortcuts.

## Split panes

Right-click a tab to split the current session into:

- **Horizontal Split**
- **Vertical Split**

The panes still belong to the same tab, but each pane can hold its own independent session content. Newer workspace builds also let you drag tabs into target docking areas, which is a more visual way to move sessions into horizontal or vertical splits. This is useful for:

- Watching logs in one pane and running commands in another
- Comparing two hosts side by side
- Keeping a local terminal next to a remote SSH session
- Watching serial output while running SSH troubleshooting commands

## Sessions and workspace structure

There are two concepts that are easy to mix up:

1. **Logical tabs / pane tree** — how a tab is split internally
2. **Runtime window layout** — where tabs are currently attached in the live workspace

For day-to-day usage, the simple mental model is:

- Tabs organize tasks
- Splits let you observe things side by side
- The active pane decides where input goes

## Layout restoration and workspace padding

NyaTerm saves the terminal window layout and tries to restore tabs, splits, and active positions when the workspace is reopened. This helps if you maintain the same set of hosts, logs, and local build tasks over time.

If you want clearer visual separation between terminal areas, adjust **Workspace Padding** in terminal-related settings. This only changes the workspace spacing; it does not change remote or local shell behavior.

## Left and right panels

### Left side

The left side is mainly for capability entry points:

- File explorer
- Network
- Security/Auth
- Cloud Sync

The **Cloud Sync** panel is a runtime entry point for cross-device config sync. It surfaces current status, recent sync activity, and direct conflict-handling actions when a conflict is detected.

The **File Explorer** can follow the current SSH terminal working directory and can send a directory or file path back to the terminal.

### Right side

The right side is mainly for live state and navigation:

- Saved connections
- Active sessions
- Command history
- Resource Monitor, NVIDIA GPU Monitor, Ascend NPU Monitor, Process Manager, and Docker Manager (SSH sessions)

If your workflow is "pick a connection, then inspect live state," this split feels natural.

The five monitoring panels only make sense for SSH sessions, and each is shown or hidden by its own toggle in **Settings → Terminal**; turning a toggle off also hides its activity-bar icon. Resource Monitor, Process Manager, and Docker Manager are on by default; the two accelerator panels are off. See [Remote Host Monitoring](./remote-monitoring).

The **AI Assistant** panel can use the active pane as context to generate commands, explain output or selected text, analyze errors, and create approved quick commands.

## Bottom helper area

The bottom area is used for features that do not need to permanently occupy a sidebar, such as:

- **Quick Commands** — reusable commands with variable prompts
- **Serial Send** — useful when repeatedly sending fixed text to a serial device
- **Recording** — start or stop session recording
- **Lock** — quickly lock the app

This is one of the differences between NyaTerm and a basic multi-tab terminal: it organizes the actions around sessions, not just the terminal surface itself.

## Child windows

Some flows open dedicated child windows instead of replacing the main workspace, such as:

- Settings
- New session / connection creation
- Quick command editing
- Auto-upload prompts

This helps because it:

- Avoids interrupting the main workspace
- Gives complex configuration its own focused space
- Makes screenshots and demos easier to structure

## Tray, child windows, and window state

If **Minimize to tray when closing** is enabled in **Settings → General**, closing the main window keeps the app running in the background instead of quitting immediately. The main window can also be hidden and restored later from the tray or system entry point.

NyaTerm also tries to remember child-window size, position, and modal relationships. Settings, new-session, remote-file editing, and auto-upload windows should reopen with more stable focus and window state.

This is useful when you want to:

- Keep active sessions alive
- Let sync or backup continue in the background
- Reopen the main workspace quickly from the tray

## Recommended workflow combinations

### Local + remote

- Tab 1: SSH session to the target host
- Tab 2: Local Terminal for builds or Git commands
- Right panel: Command History

### Dual-pane troubleshooting

- Left pane: live logs
- Right pane: diagnostic commands
- Resource Monitor open to watch CPU / memory changes

### File + terminal workflow

- SSH terminal enters the target directory
- File explorer syncs to the same path
- Open a remote file, edit it locally, then upload it back

### Multi-host synchronized input

- Open multiple SSH sessions for related hosts
- Add the sessions to a synchronized input group
- Input typed in one active session is broadcast to the other enabled sessions in the group

:::tip Screenshot suggestion
- Suggested image path: `/img/docs/layout/quick-start-split-workspace.png`
- Show a split workspace with one SSH session and one Local Terminal
- Another good image path: `/img/docs/readme/main-workspace.png`
- Include the activity bars, center terminal area, and bottom helper area together
:::
