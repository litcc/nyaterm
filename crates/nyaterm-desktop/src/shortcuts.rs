#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShortcutCategory {
    Terminal,
    Tab,
    View,
    FileExplorer,
    SavedConnections,
    Special,
}

impl ShortcutCategory {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Terminal => "Terminal",
            Self::Tab => "Tab / Session",
            Self::View => "View / Layout",
            Self::FileExplorer => "File Explorer",
            Self::SavedConnections => "Saved Connections",
            Self::Special => "Special",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShortcutNativeStatus {
    Supported,
    Partial,
    Contextual,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ShortcutDefinition {
    pub(crate) id: &'static str,
    pub(crate) category: ShortcutCategory,
    pub(crate) label: &'static str,
    pub(crate) default_keys: &'static str,
    pub(crate) native_status: ShortcutNativeStatus,
    pub(crate) note: &'static str,
}

pub(crate) const SHORTCUT_CATEGORIES: [ShortcutCategory; 6] = [
    ShortcutCategory::Terminal,
    ShortcutCategory::Tab,
    ShortcutCategory::View,
    ShortcutCategory::FileExplorer,
    ShortcutCategory::SavedConnections,
    ShortcutCategory::Special,
];

pub(crate) const SHORTCUT_REGISTRY: [ShortcutDefinition; 32] = [
    ShortcutDefinition {
        id: "terminal.copy",
        category: ShortcutCategory::Terminal,
        label: "Copy",
        default_keys: "Ctrl+Shift+C / Cmd+Shift+C",
        native_status: ShortcutNativeStatus::Supported,
        note: "Copies the current selection, otherwise the visible terminal text.",
    },
    ShortcutDefinition {
        id: "terminal.paste",
        category: ShortcutCategory::Terminal,
        label: "Paste",
        default_keys: "Ctrl+Shift+V / Cmd+Shift+V / Shift+Insert",
        native_status: ShortcutNativeStatus::Partial,
        note: "Native terminal accepts Ctrl/Cmd+V and toolbar paste.",
    },
    ShortcutDefinition {
        id: "terminal.pasteSelected",
        category: ShortcutCategory::Terminal,
        label: "Paste Selected Text",
        default_keys: "Ctrl+Shift+X / Cmd+Shift+X",
        native_status: ShortcutNativeStatus::Supported,
        note: "Pastes the current terminal text selection into the active session.",
    },
    ShortcutDefinition {
        id: "terminal.find",
        category: ShortcutCategory::Terminal,
        label: "Find",
        default_keys: "Ctrl+Shift+F / Cmd+Shift+F",
        native_status: ShortcutNativeStatus::Supported,
        note: "Opens native buffer/history search.",
    },
    ShortcutDefinition {
        id: "terminal.clear",
        category: ShortcutCategory::Terminal,
        label: "Clear Screen",
        default_keys: "Ctrl+L / Cmd+L",
        native_status: ShortcutNativeStatus::Supported,
        note: "Clears the active terminal screen.",
    },
    ShortcutDefinition {
        id: "terminal.selectAll",
        category: ShortcutCategory::Terminal,
        label: "Select All",
        default_keys: "Ctrl+Shift+A / Cmd+Shift+A",
        native_status: ShortcutNativeStatus::Supported,
        note: "Selects the visible terminal grid; copy uses the selection when present.",
    },
    ShortcutDefinition {
        id: "terminal.manageSyncGroups",
        category: ShortcutCategory::Terminal,
        label: "Manage Sync Groups",
        default_keys: "Ctrl+Shift+G / Cmd+Shift+G",
        native_status: ShortcutNativeStatus::Supported,
        note: "Opens native synchronized input groups and broadcasts terminal input to peers.",
    },
    ShortcutDefinition {
        id: "terminal.showCommandSuggestions",
        category: ShortcutCategory::Terminal,
        label: "Show Command Suggestions",
        default_keys: "Alt+R",
        native_status: ShortcutNativeStatus::Supported,
        note: "Shows recent history and pinned quick commands when the command line is empty.",
    },
    ShortcutDefinition {
        id: "terminal.recording.toggle",
        category: ShortcutCategory::Terminal,
        label: "Toggle Session Recording",
        default_keys: "Ctrl+Shift+R / Cmd+Shift+R",
        native_status: ShortcutNativeStatus::Supported,
        note: "Starts or stops transcript recording for the active terminal session.",
    },
    ShortcutDefinition {
        id: "tab.newSession",
        category: ShortcutCategory::Tab,
        label: "New Session",
        default_keys: "Ctrl+Shift+N / Cmd+Shift+N",
        native_status: ShortcutNativeStatus::Supported,
        note: "Opens the native saved connections page.",
    },
    ShortcutDefinition {
        id: "tab.temporarySshLink",
        category: ShortcutCategory::Tab,
        label: "Temporary SSH Link",
        default_keys: "Ctrl+Alt+N / Cmd+Alt+N",
        native_status: ShortcutNativeStatus::Supported,
        note: "Opens the native temporary SSH link dialog and starts a transient SSH session.",
    },
    ShortcutDefinition {
        id: "tab.quickSwitch",
        category: ShortcutCategory::Tab,
        label: "Quick Switch",
        default_keys: "Ctrl+Shift+S / Cmd+Shift+S",
        native_status: ShortcutNativeStatus::Supported,
        note: "Opens the native session/connection switcher.",
    },
    ShortcutDefinition {
        id: "tab.newLocalTerminal",
        category: ShortcutCategory::Tab,
        label: "New Local Terminal",
        default_keys: "Ctrl+` / Cmd+`",
        native_status: ShortcutNativeStatus::Supported,
        note: "Starts a local PTY session.",
    },
    ShortcutDefinition {
        id: "tab.close",
        category: ShortcutCategory::Tab,
        label: "Close Tab",
        default_keys: "Ctrl+Shift+W / Cmd+Shift+W",
        native_status: ShortcutNativeStatus::Supported,
        note: "Closes the active native session.",
    },
    ShortcutDefinition {
        id: "tab.next",
        category: ShortcutCategory::Tab,
        label: "Next Tab",
        default_keys: "Ctrl+Tab",
        native_status: ShortcutNativeStatus::Supported,
        note: "Cycles forward through ordered sessions.",
    },
    ShortcutDefinition {
        id: "tab.prev",
        category: ShortcutCategory::Tab,
        label: "Previous Tab",
        default_keys: "Ctrl+Shift+Tab",
        native_status: ShortcutNativeStatus::Supported,
        note: "Cycles backward through ordered sessions.",
    },
    ShortcutDefinition {
        id: "tab.switchTo",
        category: ShortcutCategory::Tab,
        label: "Switch Tab",
        default_keys: "Ctrl+1-9 / Cmd+1-9",
        native_status: ShortcutNativeStatus::Supported,
        note: "Switches to tabs 1-8, or the last tab with 9.",
    },
    ShortcutDefinition {
        id: "tab.duplicateSession",
        category: ShortcutCategory::Tab,
        label: "Duplicate Session",
        default_keys: "Ctrl+Shift+D / Cmd+Shift+D",
        native_status: ShortcutNativeStatus::Supported,
        note: "Duplicates the active session.",
    },
    ShortcutDefinition {
        id: "tab.multiplexSsh",
        category: ShortcutCategory::Tab,
        label: "Multiplex SSH",
        default_keys: "Ctrl+Shift+M / Cmd+Shift+M",
        native_status: ShortcutNativeStatus::Supported,
        note: "Creates a native SSH session through the multiplex handle pool.",
    },
    ShortcutDefinition {
        id: "tab.duplicateSessionWithCommand",
        category: ShortcutCategory::Tab,
        label: "Duplicate Session With Command",
        default_keys: "Ctrl+Alt+D / Cmd+Alt+D",
        native_status: ShortcutNativeStatus::Supported,
        note: "Opens the duplicate-and-run command dialog.",
    },
    ShortcutDefinition {
        id: "tab.multiplexSshWithCommand",
        category: ShortcutCategory::Tab,
        label: "Multiplex SSH With Command",
        default_keys: "Ctrl+Alt+M / Cmd+Alt+M",
        native_status: ShortcutNativeStatus::Supported,
        note: "Opens the multiplex-and-run command dialog.",
    },
    ShortcutDefinition {
        id: "view.toggleLeftSidebar",
        category: ShortcutCategory::View,
        label: "Toggle Left Sidebar",
        default_keys: "Ctrl+Shift+E / Cmd+Shift+E",
        native_status: ShortcutNativeStatus::Supported,
        note: "Collapses or expands the native Explorer panel.",
    },
    ShortcutDefinition {
        id: "view.toggleRightSidebar",
        category: ShortcutCategory::View,
        label: "Toggle Right Sidebar",
        default_keys: "Ctrl+Shift+B / Cmd+Shift+B",
        native_status: ShortcutNativeStatus::Supported,
        note: "Collapses or expands the native Inspector panel.",
    },
    ShortcutDefinition {
        id: "view.zoomIn",
        category: ShortcutCategory::View,
        label: "Zoom In",
        default_keys: "Ctrl+= / Cmd+=",
        native_status: ShortcutNativeStatus::Supported,
        note: "Increases the native terminal font size and saves the appearance setting.",
    },
    ShortcutDefinition {
        id: "view.zoomOut",
        category: ShortcutCategory::View,
        label: "Zoom Out",
        default_keys: "Ctrl+- / Cmd+-",
        native_status: ShortcutNativeStatus::Supported,
        note: "Decreases the native terminal font size and saves the appearance setting.",
    },
    ShortcutDefinition {
        id: "view.resetZoom",
        category: ShortcutCategory::View,
        label: "Reset Zoom",
        default_keys: "Ctrl+0 / Cmd+0",
        native_status: ShortcutNativeStatus::Supported,
        note: "Resets the native terminal font size to the default appearance value.",
    },
    ShortcutDefinition {
        id: "view.openSettings",
        category: ShortcutCategory::View,
        label: "Open Settings",
        default_keys: "Ctrl+, / Cmd+,",
        native_status: ShortcutNativeStatus::Supported,
        note: "Opens the native Settings page.",
    },
    ShortcutDefinition {
        id: "view.openChat",
        category: ShortcutCategory::View,
        label: "Open Chat",
        default_keys: "Ctrl+Alt+I / Cmd+Alt+I",
        native_status: ShortcutNativeStatus::Supported,
        note: "Focuses the native AI command panel.",
    },
    ShortcutDefinition {
        id: "view.showAllCommands",
        category: ShortcutCategory::View,
        label: "Show All Commands",
        default_keys: "Ctrl+Shift+P / Cmd+Shift+P",
        native_status: ShortcutNativeStatus::Supported,
        note: "Focuses the command center in the right inspector.",
    },
    ShortcutDefinition {
        id: "fileExplorer.rename",
        category: ShortcutCategory::FileExplorer,
        label: "Rename File",
        default_keys: "F2",
        native_status: ShortcutNativeStatus::Contextual,
        note: "Renames the selected entry in the native SFTP transfers listing.",
    },
    ShortcutDefinition {
        id: "savedConnections.copySelected",
        category: ShortcutCategory::SavedConnections,
        label: "Copy Selected Saved Connections",
        default_keys: "Ctrl+Alt+C / Cmd+Alt+C",
        native_status: ShortcutNativeStatus::Supported,
        note: "Duplicates the selected native saved connections without copying stored secrets.",
    },
    ShortcutDefinition {
        id: "special.lockScreen",
        category: ShortcutCategory::Special,
        label: "Lock Screen",
        default_keys: "Ctrl+Shift+L / Cmd+Shift+L",
        native_status: ShortcutNativeStatus::Supported,
        note: "Locks the native workspace.",
    },
];

pub(crate) fn shortcut_keys_for(id: &str, overrides: &HashMap<String, String>) -> Option<String> {
    overrides
        .get(id)
        .filter(|keys| !keys.trim().is_empty())
        .cloned()
        .or_else(|| default_chords_for(id).map(ToString::to_string))
}

pub(crate) fn shortcut_matches(
    event: &KeyDownEvent,
    id: &str,
    overrides: &HashMap<String, String>,
) -> bool {
    shortcut_keys_for(id, overrides)
        .as_deref()
        .is_some_and(|keys| hotkey_matches(event, keys))
}

pub(crate) fn event_to_hotkey_string(event: &KeyDownEvent) -> Option<String> {
    let key = normalized_event_key(event)?;
    if is_modifier_key(&key) {
        return None;
    }

    let mut parts = Vec::new();
    if event.keystroke.modifiers.control {
        parts.push("ctrl");
    }
    if event.keystroke.modifiers.platform {
        parts.push("meta");
    }
    if event.keystroke.modifiers.alt {
        parts.push("alt");
    }
    if event.keystroke.modifiers.shift {
        parts.push("shift");
    }
    parts.push(key.as_str());
    Some(parts.join("+"))
}

pub(crate) fn format_hotkey_for_display(keys: &str) -> String {
    keys.split(',')
        .map(str::trim)
        .filter(|combo| !combo.is_empty())
        .map(|combo| {
            combo
                .split('+')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(format_hotkey_part)
                .collect::<Vec<_>>()
                .join("+")
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

pub(crate) fn hotkey_keystrokes_for_display(keys: &str) -> Option<Vec<Keystroke>> {
    let keystrokes = keys
        .split(',')
        .map(str::trim)
        .filter(|combo| !combo.is_empty())
        .map(|combo| {
            let parts = combo
                .split('+')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(|part| match part.to_ascii_lowercase().as_str() {
                    "control" => "ctrl".to_string(),
                    "meta" | "command" => "cmd".to_string(),
                    "option" => "alt".to_string(),
                    _ => normalize_key_name(part),
                })
                .collect::<Vec<_>>();

            Keystroke::parse(&parts.join("-")).ok()
        })
        .collect::<Option<Vec<_>>>()?;

    (!keystrokes.is_empty()).then_some(keystrokes)
}

pub(crate) fn is_indexed_shortcut_template(keys: &str) -> bool {
    keys.split(',')
        .map(str::trim)
        .filter(|combo| !combo.is_empty())
        .all(|combo| {
            let parts = combo
                .split('+')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            parts.len() > 1
                && parts
                    .last()
                    .is_some_and(|key| normalize_key_name(key) == "1")
        })
}

fn default_chords_for(id: &str) -> Option<&'static str> {
    match id {
        "terminal.copy" => Some("ctrl+shift+c,meta+shift+c"),
        "terminal.paste" => Some("ctrl+shift+v,meta+shift+v,shift+insert"),
        "terminal.pasteSelected" => Some("ctrl+shift+x,meta+shift+x"),
        "terminal.find" => Some("ctrl+shift+f,meta+shift+f"),
        "terminal.clear" => Some("ctrl+l,meta+l"),
        "terminal.selectAll" => Some("ctrl+shift+a,meta+shift+a"),
        "terminal.manageSyncGroups" => Some("ctrl+shift+g,meta+shift+g"),
        "terminal.recording.toggle" => Some("ctrl+shift+r,meta+shift+r"),
        "tab.newSession" => Some("ctrl+shift+n,meta+shift+n"),
        "tab.temporarySshLink" => Some("ctrl+alt+n,meta+alt+n"),
        "tab.quickSwitch" => Some("ctrl+shift+s,meta+shift+s"),
        "tab.newLocalTerminal" => Some("ctrl+`,meta+`"),
        "tab.close" => Some("ctrl+shift+w,meta+shift+w"),
        "tab.next" => Some("ctrl+tab"),
        "tab.prev" => Some("ctrl+shift+tab"),
        "tab.switchTo" => Some(
            "ctrl+1,ctrl+2,ctrl+3,ctrl+4,ctrl+5,ctrl+6,ctrl+7,ctrl+8,ctrl+9,meta+1,meta+2,meta+3,meta+4,meta+5,meta+6,meta+7,meta+8,meta+9",
        ),
        "tab.duplicateSession" => Some("ctrl+shift+d,meta+shift+d"),
        "tab.multiplexSsh" => Some("ctrl+shift+m,meta+shift+m"),
        "tab.duplicateSessionWithCommand" => Some("ctrl+alt+d,meta+alt+d"),
        "tab.multiplexSshWithCommand" => Some("ctrl+alt+m,meta+alt+m"),
        "view.toggleLeftSidebar" => Some("ctrl+shift+e,meta+shift+e"),
        "view.toggleRightSidebar" => Some("ctrl+shift+b,meta+shift+b"),
        "view.zoomIn" => Some("ctrl+=,meta+=,ctrl+shift+=,meta+shift+="),
        "view.zoomOut" => Some("ctrl+-,meta+-"),
        "view.resetZoom" => Some("ctrl+0,meta+0"),
        "view.openSettings" => Some("ctrl+comma,meta+comma"),
        "view.openChat" => Some("ctrl+alt+i,meta+alt+i"),
        "view.showAllCommands" => Some("ctrl+shift+p,meta+shift+p"),
        "fileExplorer.rename" => Some("f2"),
        "savedConnections.copySelected" => Some("ctrl+alt+c,meta+alt+c"),
        "special.lockScreen" => Some("ctrl+shift+l,meta+shift+l"),
        _ => None,
    }
}

fn hotkey_matches(event: &KeyDownEvent, keys: &str) -> bool {
    keys.split(',')
        .map(str::trim)
        .filter(|combo| !combo.is_empty())
        .any(|combo| combo_matches(event, combo))
}

fn combo_matches(event: &KeyDownEvent, combo: &str) -> bool {
    let mut expect_ctrl = false;
    let mut expect_meta = false;
    let mut expect_alt = false;
    let mut expect_shift = false;
    let mut expected_key = None;

    for part in combo
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => expect_ctrl = true,
            "cmd" | "command" | "meta" | "super" => expect_meta = true,
            "alt" | "option" => expect_alt = true,
            "shift" => expect_shift = true,
            key => expected_key = Some(normalize_key_name(key)),
        }
    }

    let Some(expected_key) = expected_key else {
        return false;
    };
    let Some(actual_key) = normalized_event_key(event) else {
        return false;
    };

    event.keystroke.modifiers.control == expect_ctrl
        && event.keystroke.modifiers.platform == expect_meta
        && event.keystroke.modifiers.alt == expect_alt
        && event.keystroke.modifiers.shift == expect_shift
        && actual_key == expected_key
}

fn normalized_event_key(event: &KeyDownEvent) -> Option<String> {
    let key = event.keystroke.key.as_str();
    if key.len() == 1 {
        return Some(normalize_key_name(key));
    }
    if let Some(key_char) = event
        .keystroke
        .key_char
        .as_deref()
        .filter(|key_char| key_char.chars().count() == 1)
    {
        return Some(normalize_key_name(key_char));
    }
    Some(normalize_key_name(key))
}

fn normalize_key_name(key: &str) -> String {
    match key.trim().to_ascii_lowercase().as_str() {
        "," => "comma".to_string(),
        "." => "period".to_string(),
        "`" | "grave" | "backtick" => "`".to_string(),
        "esc" => "escape".to_string(),
        "return" => "enter".to_string(),
        "plus" => "+".to_string(),
        "equals" | "equal" => "=".to_string(),
        value => value.to_string(),
    }
}

fn format_hotkey_part(part: &str) -> String {
    match normalize_key_name(part).as_str() {
        "ctrl" | "control" => "Ctrl".to_string(),
        "meta" | "cmd" | "command" | "super" => "Cmd".to_string(),
        "alt" | "option" => "Alt".to_string(),
        "shift" => "Shift".to_string(),
        "comma" => ",".to_string(),
        "period" => ".".to_string(),
        "`" => "`".to_string(),
        "escape" => "Esc".to_string(),
        "enter" => "Enter".to_string(),
        "tab" => "Tab".to_string(),
        "insert" => "Insert".to_string(),
        key if key.len() == 1 => key.to_ascii_uppercase(),
        key => key
            .split('_')
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn is_modifier_key(key: &str) -> bool {
    matches!(
        key,
        "ctrl" | "control" | "meta" | "cmd" | "command" | "super" | "alt" | "option" | "shift"
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use gpui::{KeyDownEvent, Keystroke, Modifiers};

    use super::{hotkey_keystrokes_for_display, shortcut_matches};

    #[test]
    fn display_hotkeys_parse_chords_and_key_aliases_for_kbd() {
        let keystrokes =
            hotkey_keystrokes_for_display("ctrl+shift+c,meta+comma,control+-,option+return")
                .expect("valid hotkeys should parse");

        assert_eq!(keystrokes.len(), 4);
        assert!(keystrokes[0].modifiers.control);
        assert!(keystrokes[0].modifiers.shift);
        assert_eq!(keystrokes[0].key, "c");
        assert!(keystrokes[1].modifiers.platform);
        assert_eq!(keystrokes[1].key, "comma");
        assert!(keystrokes[2].modifiers.control);
        assert_eq!(keystrokes[2].key, "-");
        assert!(keystrokes[3].modifiers.alt);
        assert_eq!(keystrokes[3].key, "enter");
    }

    #[test]
    fn display_hotkeys_reject_invalid_or_empty_chord_sets() {
        assert!(hotkey_keystrokes_for_display("").is_none());
        assert!(hotkey_keystrokes_for_display("ctrl+shift+c,not-a-valid-key").is_none());
    }

    fn key_event(key: &str, modifiers: Modifiers) -> KeyDownEvent {
        KeyDownEvent {
            keystroke: Keystroke {
                modifiers,
                key: key.to_string(),
                key_char: Some(key.to_string()),
            },
            is_held: false,
            prefer_character_input: false,
        }
    }

    #[test]
    fn terminal_clipboard_shortcuts_require_shift() {
        let overrides = HashMap::new();

        assert!(!shortcut_matches(
            &key_event(
                "c",
                Modifiers {
                    control: true,
                    ..Modifiers::default()
                }
            ),
            "terminal.copy",
            &overrides,
        ));
        assert!(shortcut_matches(
            &key_event(
                "c",
                Modifiers {
                    control: true,
                    shift: true,
                    ..Modifiers::default()
                }
            ),
            "terminal.copy",
            &overrides,
        ));
        assert!(!shortcut_matches(
            &key_event(
                "v",
                Modifiers {
                    control: true,
                    ..Modifiers::default()
                }
            ),
            "terminal.paste",
            &overrides,
        ));
        assert!(shortcut_matches(
            &key_event(
                "v",
                Modifiers {
                    control: true,
                    shift: true,
                    ..Modifiers::default()
                }
            ),
            "terminal.paste",
            &overrides,
        ));
    }
}
use std::collections::HashMap;

use gpui::{KeyDownEvent, Keystroke};
