use std::collections::HashMap;

use gpui::{Context, KeyDownEvent, Window};

use crate::features::NyaTermApp;
use crate::shortcuts::event_to_hotkey_string;

impl NyaTermApp {
    /// First display chord for empty-workspace / UI labels (Tauri-style chips).
    pub(in crate::features) fn display_shortcut_for(&self, id: &str, fallback: &str) -> String {
        use crate::shortcuts::{format_hotkey_for_display, shortcut_keys_for};
        let raw = shortcut_keys_for(id, &self.settings.summary().keybindings)
            .unwrap_or_else(|| fallback.to_string());
        let display = format_hotkey_for_display(&raw);
        display
            .split(" / ")
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(fallback)
            .to_string()
    }
    pub(in crate::features) fn start_keybinding_recording(
        &mut self,
        shortcut_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.begin_keybinding_recording(shortcut_id);
        self.shell.set_status("recording shortcut".to_string());
        window.focus(self.settings.keybinding_focus(), cx);
        cx.notify();
    }

    pub(in crate::features) fn cancel_keybinding_recording(&mut self, cx: &mut Context<Self>) {
        self.settings.cancel_keybinding_recording();
        self.shell
            .set_status("shortcut recording cancelled".to_string());
        cx.notify();
    }

    pub(in crate::features) fn confirm_keybinding_recording(&mut self, cx: &mut Context<Self>) {
        let Some(shortcut_id) = self.settings.keybinding_recording_id().map(str::to_owned) else {
            self.shell
                .set_status("no shortcut recording is active".to_string());
            cx.notify();
            return;
        };
        let Some(keys) = self.settings.pending_keybinding().map(str::to_owned) else {
            self.shell
                .set_status("press a shortcut before saving".to_string());
            cx.notify();
            return;
        };

        self.settings.finish_keybinding_recording();
        if let Some(conflict) = self.keybinding_conflict_label(&keys, &shortcut_id) {
            self.settings.begin_keybinding_recording(shortcut_id);
            self.settings.set_pending_keybinding(Some(keys));
            self.shell
                .set_status(format!("shortcut conflicts with {conflict}"));
            cx.notify();
            return;
        }
        let mut keybindings = self.settings.summary().keybindings.clone();
        let is_default = crate::shortcuts::SHORTCUT_REGISTRY
            .iter()
            .find(|s| s.id == shortcut_id)
            .is_some_and(|def| keys == def.default_keys);
        if is_default {
            keybindings.remove(&shortcut_id);
        } else {
            keybindings.insert(shortcut_id.clone(), keys);
        }
        self.save_keybindings(keybindings, format!("shortcut {shortcut_id} saved"), cx);
    }

    pub(in crate::features) fn reset_keybinding(
        &mut self,
        shortcut_id: String,
        cx: &mut Context<Self>,
    ) {
        let mut keybindings = self.settings.summary().keybindings.clone();
        keybindings.remove(&shortcut_id);
        self.save_keybindings(keybindings, format!("shortcut {shortcut_id} reset"), cx);
    }

    pub(in crate::features) fn reset_all_keybindings(&mut self, cx: &mut Context<Self>) {
        self.save_keybindings(HashMap::new(), "all shortcuts reset".to_string(), cx);
    }

    fn save_keybindings(
        &mut self,
        keybindings: HashMap<String, String>,
        success_message: String,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_keybindings(keybindings.clone());
        if self.defer_settings_persistence(cx) {
            self.settings.finish_keybinding_recording();
            self.shell
                .set_status(success_message.replace("saved", "staged"));
            return;
        }
        self.settings.finish_keybinding_recording();
        self.shell.set_status(success_message);
        self.queue_settings_save(crate::features::settings::SettingsSaveKind::Keybindings, cx);
        cx.notify();
    }

    pub(in crate::features) fn handle_keybinding_key_down(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        let Some(recording_id) = self.settings.keybinding_recording_id().map(str::to_owned) else {
            return;
        };
        match event.keystroke.key.as_str() {
            "escape" => {
                self.cancel_keybinding_recording(cx);
                return;
            }
            "enter" if self.settings.pending_keybinding().is_some() => {
                self.confirm_keybinding_recording(cx);
                return;
            }
            _ => {}
        }

        let Some(keys) = event_to_hotkey_string(event) else {
            return;
        };
        if recording_id == "tab.switchTo" && !crate::shortcuts::is_indexed_shortcut_template(&keys)
        {
            self.settings.set_pending_keybinding(None);
            self.shell
                .set_status("tab switch shortcut must end with number 1".to_string());
            cx.notify();
            return;
        }
        self.settings.set_pending_keybinding(Some(keys));
        self.shell
            .set_status("shortcut captured; press Enter or Save".to_string());
        cx.notify();
    }

    pub(in crate::features) fn keybinding_conflict_label(
        &self,
        pending_keys: &str,
        exclude_id: &str,
    ) -> Option<String> {
        let normalized_new = pending_keys
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase())
            .collect::<Vec<_>>();
        if normalized_new.is_empty() {
            return None;
        }
        for shortcut in crate::shortcuts::SHORTCUT_REGISTRY.iter() {
            if shortcut.id == exclude_id {
                continue;
            }
            let existing = crate::shortcuts::shortcut_keys_for(
                shortcut.id,
                &self.settings.summary().keybindings,
            )
            .unwrap_or_else(|| shortcut.default_keys.to_string());
            let normalized_existing = existing
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_ascii_lowercase())
                .collect::<Vec<_>>();
            if normalized_new
                .iter()
                .any(|n| normalized_existing.iter().any(|e| e == n))
            {
                return Some(rust_i18n::t!(shortcut.label_key).into_owned());
            }
        }
        None
    }

    pub(in crate::features) fn apply_keybinding_search(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_keybinding_search(text);
        cx.notify();
    }
}
