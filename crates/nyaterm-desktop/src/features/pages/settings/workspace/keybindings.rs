use rust_i18n::t;

use std::borrow::Cow;

use gpui::{Context, FontWeight, IntoElement, KeyDownEvent, div, prelude::*, px, rgb};
use nyaterm_ui::NyaSearchInput;

use crate::features::{pages::settings::panel::SettingsPanel, shell::gpui_code_font_family};
use crate::shortcuts::{
    SHORTCUT_CATEGORIES, SHORTCUT_REGISTRY, ShortcutCategory, ShortcutDefinition,
    ShortcutNativeStatus, format_hotkey_for_display, shortcut_keys_for,
};
use crate::widgets::small_button;

use super::super::settings_form_section;

impl SettingsPanel {
    pub(in crate::features) fn keybindings_settings_section(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let palette = self.theme_palette();
        let overrides = self.settings.summary().keybindings.len();
        let search = self.settings.keybinding_presentation().search_draft;
        let Some(search_field) = self.existing_text_input("settings.keybindings.search") else {
            debug_assert!(false, "the keybindings search input was never built");
            return div().into_any_element();
        };
        let mut groups = div().flex().flex_col().gap_3();
        for category in SHORTCUT_CATEGORIES {
            groups = groups.child(self.shortcut_category_group(category, &search, cx));
        }

        div()
            .id("settings-keybindings-panel")
            .flex()
            .flex_col()
            .gap_3()
            .track_focus(self.settings.keybinding_focus())
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                this.handle_keybinding_key_down(event, cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div().min_w_0().flex_1().child(
                            NyaSearchInput::new("settings-keybindings-search", &search_field)
                                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                                    if event.keystroke.key == "escape" {
                                        cx.stop_propagation();
                                        this.clear_keybinding_search(cx);
                                    }
                                })),
                        ),
                    )
                    .when(overrides > 0, |this| {
                        this.child(small_button(
                            palette,
                            "keybindings-reset-all",
                            t!("settings.keybindingsResetAll"),
                            cx.listener(|this, _, _, cx| {
                                this.reset_all_keybindings(cx);
                            }),
                        ))
                    }),
            )
            .child(groups)
            .into_any_element()
    }

    pub(in crate::features) fn shortcut_category_group(
        &mut self,
        category: ShortcutCategory,
        search: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let needle = search.trim().to_ascii_lowercase();
        let shortcuts = SHORTCUT_REGISTRY
            .iter()
            .filter(|shortcut| shortcut.category == category)
            .filter(|shortcut| {
                if needle.is_empty() {
                    return true;
                }
                let keys = shortcut_keys_for(shortcut.id, &self.settings.summary().keybindings)
                    .unwrap_or_else(|| shortcut.default_keys.to_string());
                let display = format_hotkey_for_display(&keys).to_ascii_lowercase();
                shortcut.label.to_ascii_lowercase().contains(&needle)
                    || shortcut.id.to_ascii_lowercase().contains(&needle)
                    || display.contains(&needle)
                    || keys.to_ascii_lowercase().contains(&needle)
            })
            .collect::<Vec<_>>();
        if !needle.is_empty() && shortcuts.is_empty() {
            return div().into_any_element();
        }
        let mut rows = div().flex().flex_col().gap_1();
        for shortcut in shortcuts {
            rows = rows.child(self.shortcut_registry_row(shortcut, cx));
        }

        settings_form_section(palette, Some(Cow::Borrowed(category.label())), None, rows)
            .into_any_element()
    }

    pub(in crate::features) fn shortcut_registry_row(
        &mut self,
        shortcut: &'static ShortcutDefinition,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let _registry_metadata = (
            shortcut.note,
            match shortcut.native_status {
                ShortcutNativeStatus::Supported => "supported",
                ShortcutNativeStatus::Partial => "partial",
                ShortcutNativeStatus::Contextual => "contextual",
            },
        );
        let is_custom = self
            .settings
            .summary()
            .keybindings
            .contains_key(shortcut.id);
        let interaction = self.settings.keybinding_presentation();
        let is_recording = interaction.recording_id.as_deref() == Some(shortcut.id);
        let effective_keys = shortcut_keys_for(shortcut.id, &self.settings.summary().keybindings)
            .unwrap_or_else(|| shortcut.default_keys.to_string());
        let conflict = if is_recording {
            interaction
                .pending_keys
                .as_deref()
                .and_then(|keys| self.keybinding_conflict_label(keys, shortcut.id))
        } else {
            None
        };
        let custom_label = t!("settings.keybindingsCustom");
        let recording_label = t!("settings.keybindingsRecording");
        let reset_label = t!("settings.keybindingsReset");
        let indexed_hint = t!("settings.keybindingsIndexedHint");
        let key_display = if is_recording {
            interaction
                .pending_keys
                .as_deref()
                .map(format_hotkey_for_display)
                .unwrap_or_else(|| recording_label.to_string())
        } else {
            format_hotkey_for_display(&effective_keys)
        };
        let shortcut_id = shortcut.id.to_string();
        let reset_shortcut_id = shortcut.id.to_string();
        let is_switch_to = shortcut.id == "tab.switchTo";

        div()
            .rounded_md()
            .px_2()
            .py_1()
            .border_1()
            .border_color(if is_recording {
                rgb(0x1f6feb)
            } else {
                rgb(palette.surface_elevated)
            })
            .bg(if is_recording {
                rgb(palette.hover)
            } else {
                rgb(palette.bg)
            })
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                div().min_w_0().flex_1().flex().flex_col().child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(12.))
                                .font_weight(FontWeight(600.))
                                .text_color(rgb(palette.text))
                                .overflow_hidden()
                                .child(shortcut.label),
                        )
                        .when(is_custom, |this| {
                            this.child(
                                div()
                                    .text_size(px(10.))
                                    .font_weight(FontWeight(600.))
                                    .text_color(rgb(0xbc8cff))
                                    .child(custom_label),
                            )
                        }),
                ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .rounded_md()
                            .border_1()
                            .border_color(if conflict.is_some() {
                                rgb(palette.danger)
                            } else if is_recording {
                                rgb(0x388bfd)
                            } else {
                                rgb(palette.border)
                            })
                            .bg(rgb(palette.surface))
                            .px_2()
                            .py_0()
                            .h(px(24.))
                            .flex()
                            .items_center()
                            .font_family(gpui_code_font_family())
                            .text_size(px(10.))
                            .font_weight(FontWeight(700.))
                            .text_color(if conflict.is_some() {
                                rgb(palette.danger)
                            } else if is_recording {
                                rgb(palette.link)
                            } else {
                                rgb(palette.text)
                            })
                            .child(key_display),
                    )
                    .when_some(conflict.clone(), |this, name| {
                        this.child(
                            div()
                                .text_size(px(10.))
                                .text_color(rgb(palette.danger))
                                .child(format!("conflicts: {name}")),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .when(is_recording, |this| {
                        this.child(small_button(
                            palette,
                            format!("keybinding-save-{}", shortcut.id),
                            t!("common.confirm"),
                            cx.listener(|this, _, _, cx| {
                                this.confirm_keybinding_recording(cx);
                            }),
                        ))
                        .child(small_button(
                            palette,
                            format!("keybinding-cancel-{}", shortcut.id),
                            t!("common.cancel"),
                            cx.listener(|this, _, _, cx| {
                                this.cancel_keybinding_recording(cx);
                            }),
                        ))
                    })
                    .when(!is_recording, |this| {
                        this.child(small_button(
                            palette,
                            format!("keybinding-record-{}", shortcut.id),
                            t!("common.edit"),
                            cx.listener(move |this, _, window, cx| {
                                this.start_keybinding_recording(shortcut_id.clone(), window, cx);
                            }),
                        ))
                    })
                    .when(is_custom && !is_recording, |this| {
                        this.child(small_button(
                            palette,
                            format!("keybinding-reset-{}", shortcut.id),
                            reset_label,
                            cx.listener(move |this, _, _, cx| {
                                this.reset_keybinding(reset_shortcut_id.clone(), cx);
                            }),
                        ))
                    }),
            )
            .when(is_recording && is_switch_to, |this| {
                this.child(
                    div()
                        .w_full()
                        .text_size(px(10.))
                        .text_color(rgb(palette.text_muted))
                        .child(indexed_hint),
                )
            })
    }
}
