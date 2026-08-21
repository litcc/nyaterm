use rust_i18n::t;

use std::borrow::Cow;

use gpui::{
    AnyElement, App, ClickEvent, Context, FontWeight, IntoElement, SharedString, Window, div,
    prelude::*, px, rgb, rgba,
};

use crate::models::{SettingsTab, SnapshotPasswordPromptKind, SnapshotPasswordPromptState};
use crate::theme::ThemePalette;
use nyaterm_ui::{
    NyaSelectOption, NyaSettingsLayout, NyaSettingsNavGroup, NyaSettingsNavItem, NyaSwitch,
};

use super::super::NyaTermApp;

mod ai;
mod security;
mod sync_backup;
mod terminal;
mod transfer;
mod translation;
mod workspace;

impl NyaTermApp {
    pub(in crate::features) fn settings_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.settings_surface(self.shell.viewport_size().0, false, cx)
    }

    pub(in crate::features) fn settings_window_view(
        &mut self,
        viewport_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_surface(viewport_width, true, cx)
            .into_any_element()
    }

    fn settings_surface(
        &mut self,
        viewport_width: f32,
        native_window: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let backup_snapshot_prompt = self.settings.snapshot_password_prompt().filter(|prompt| {
            matches!(
                prompt.kind,
                SnapshotPasswordPromptKind::Export | SnapshotPasswordPromptKind::Import
            )
        });
        self.settings_shell(backup_snapshot_prompt, viewport_width, native_window, cx)
    }

    pub(in crate::features) fn settings_shell(
        &mut self,
        backup_snapshot_prompt: Option<SnapshotPasswordPromptState>,
        viewport_width: f32,
        native_window: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Tauri SettingsPage shell: compact header + narrow nav + scroll content.
        let palette = self.theme_palette();
        let settings_title = t!("settings.title");
        let active_group = t!(self.shell.settings_active_tab().group_i18n_key());
        let active_label = t!(self.shell.settings_active_tab().i18n_key());
        let back_label = t!("common.close");
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(palette.bg))
            .when(!native_window, |this| {
                this.child(
                    div()
                        .h(px(36.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .border_b_1()
                        .border_color(rgb(palette.border))
                        .bg(rgb(palette.section_header))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .font_weight(FontWeight(700.))
                                        .text_color(rgb(palette.text))
                                        .child(settings_title),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(palette.text_dimmed))
                                        .child("·"),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(palette.text_muted))
                                        .child(active_group),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(palette.text_dimmed))
                                        .child("/"),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .font_weight(FontWeight(600.))
                                        .text_color(rgb(palette.text))
                                        .child(active_label),
                                ),
                        )
                        .child(
                            div()
                                .id(SharedString::from("settings-close"))
                                .h(px(26.))
                                .px_2()
                                .flex()
                                .items_center()
                                .rounded_md()
                                .text_size(px(11.))
                                .text_color(rgb(palette.text_muted))
                                .cursor_pointer()
                                .hover(move |this| {
                                    this.bg(rgb(palette.surface_elevated))
                                        .text_color(rgb(palette.text))
                                })
                                .child(back_label)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.close_settings(cx);
                                })),
                        ),
                )
            })
            .child(self.settings_layout(viewport_width, cx))
            .when_some(backup_snapshot_prompt, |this, prompt| {
                this.child(
                    div()
                        .flex_none()
                        .px_4()
                        .child(self.snapshot_password_prompt_banner(prompt, cx)),
                )
            })
            .child(self.settings_action_footer(cx))
    }

    fn settings_action_footer(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.theme_palette();
        let dirty = self.settings_draft_dirty();
        let validation_error = dirty.then(|| self.pending_settings_cloud_error()).flatten();
        let apply_disabled = !dirty || validation_error.is_some();
        let confirm_disabled = validation_error.is_some();
        let status = validation_error.clone().unwrap_or_else(|| {
            t!(if dirty {
                "fileEditor.unsavedDesc"
            } else {
                "updater.noUpdate"
            })
            .to_string()
        });
        let cancel_label = t!("common.cancel");
        let apply_label = t!("common.apply");
        let confirm_label = t!("common.confirm");

        div()
            .h(px(52.))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .px_5()
            .py_3()
            .border_t_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.section_header))
            .child(
                div()
                    .min_w_0()
                    .text_size(px(11.))
                    .text_color(if validation_error.is_some() {
                        rgb(palette.warning)
                    } else {
                        rgb(palette.text_muted)
                    })
                    .child(status),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("settings-cancel")
                            .h(px(28.))
                            .min_w(px(64.))
                            .px_4()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(palette.border))
                            .text_size(px(11.))
                            .text_color(rgb(palette.text))
                            .cursor_pointer()
                            .hover(move |this| this.bg(rgb(palette.hover)))
                            .child(cancel_label)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_settings(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("settings-apply")
                            .h(px(28.))
                            .min_w(px(64.))
                            .px_4()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(palette.border))
                            .text_size(px(11.))
                            .text_color(if apply_disabled {
                                rgb(palette.text_dimmed)
                            } else {
                                rgb(palette.text)
                            })
                            .when(!apply_disabled, |this| {
                                this.cursor_pointer()
                                    .hover(move |this| this.bg(rgb(palette.hover)))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.apply_settings_draft(false, cx);
                                    }))
                            })
                            .child(apply_label),
                    )
                    .child(
                        div()
                            .id("settings-confirm")
                            .h(px(28.))
                            .min_w(px(64.))
                            .px_4()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .bg(if confirm_disabled {
                                rgb(palette.surface_elevated)
                            } else {
                                rgb(palette.link)
                            })
                            .text_size(px(11.))
                            .font_weight(FontWeight(600.))
                            .text_color(if confirm_disabled {
                                rgb(palette.text_dimmed)
                            } else {
                                rgb(0xffffff)
                            })
                            .when(!confirm_disabled, |this| {
                                this.cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.confirm_settings_draft(cx);
                                    }))
                            })
                            .child(confirm_label),
                    ),
            )
    }

    fn settings_layout(&mut self, viewport_width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.theme_palette();
        let active_tab = self.shell.settings_active_tab();
        let active_item_id = settings_tab_nav_id(active_tab);
        let active_label = t!(active_tab.i18n_key());
        let content = self.settings_tab_content(active_tab, cx);
        let select_app = cx.entity();
        let toggle_app = select_app.clone();

        NyaSettingsLayout::new(
            "settings-layout",
            self.settings_nav_groups(),
            active_item_id,
            content,
        )
        .palette(palette)
        .active_title(active_label)
        .sidebar_title(t!("settings.title"))
        .viewport_width(viewport_width)
        .compact_breakpoint(640.)
        .wide_breakpoint(1024.)
        .on_select(move |item_id, _, cx| {
            if let Some(tab) = settings_tab_from_nav_id(item_id.as_ref()) {
                select_app.update(cx, |this, cx| {
                    if tab == SettingsTab::Appearance {
                        this.ensure_appearance_font_options(cx);
                    }
                    this.shell.set_settings_active_tab(tab);
                    cx.notify();
                });
            }
        })
        .on_toggle_group(move |group_id, _, cx| {
            let _ = toggle_app.update(cx, |this, cx| {
                this.shell.toggle_settings_group(group_id.to_string());
                cx.notify();
            });
        })
    }

    fn settings_nav_groups(&self) -> Vec<NyaSettingsNavGroup> {
        let palette = self.theme_palette();
        vec![
            NyaSettingsNavGroup::new(
                "workspace",
                t!("settings.groupWorkspace"),
                "icons/workspace.svg",
            )
            .accent(palette.link)
            .expanded(self.shell.settings_group_is_expanded("workspace"))
            .items([
                self.settings_nav_item(SettingsTab::General),
                self.settings_nav_item(SettingsTab::Appearance),
                self.settings_nav_item(SettingsTab::Interaction),
                self.settings_nav_item(SettingsTab::Keybindings),
            ]),
            NyaSettingsNavGroup::new(
                "terminal_session",
                t!("settings.groupTerminalSession"),
                "icons/conn/terminal.svg",
            )
            .accent(palette.success)
            .expanded(self.shell.settings_group_is_expanded("terminal_session"))
            .items([
                self.settings_nav_item(SettingsTab::TerminalGeneral),
                self.settings_nav_item(SettingsTab::Search),
                self.settings_nav_item(SettingsTab::Translation),
            ]),
            NyaSettingsNavGroup::new("ai_group", t!("ai.title"), "icons/ai.svg")
                .accent(0xbc8cff)
                .expanded(self.shell.settings_group_is_expanded("ai_group"))
                .items([
                    self.settings_nav_item(SettingsTab::AiGeneral),
                    self.settings_nav_item(SettingsTab::AiModels),
                    self.settings_nav_item(SettingsTab::AiRules),
                ]),
            NyaSettingsNavGroup::standalone([
                self.settings_nav_item(SettingsTab::Transfer),
                self.settings_nav_item(SettingsTab::Security),
                self.settings_nav_item(SettingsTab::SyncBackup),
            ]),
        ]
    }

    fn settings_nav_item(&self, tab: SettingsTab) -> NyaSettingsNavItem {
        NyaSettingsNavItem::new(
            settings_tab_nav_id(tab),
            t!(tab.i18n_key()),
            tab.icon_path(),
        )
    }

    fn settings_tab_content(
        &mut self,
        active_tab: SettingsTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match active_tab {
            SettingsTab::General => self.general_settings_section(cx).into_any_element(),
            SettingsTab::Appearance => self.appearance_settings_section(cx).into_any_element(),
            SettingsTab::Interaction => self.interaction_settings_section(cx).into_any_element(),
            SettingsTab::Keybindings => self.keybindings_settings_section(cx).into_any_element(),
            SettingsTab::TerminalGeneral => self
                .terminal_general_settings_section(cx)
                .into_any_element(),
            SettingsTab::Search => self.terminal_search_settings_section(cx).into_any_element(),
            SettingsTab::Translation => self.translation_settings_section(cx).into_any_element(),
            SettingsTab::AiGeneral => self.ai_settings_section(cx).into_any_element(),
            SettingsTab::AiModels => self.ai_models_settings_section(cx).into_any_element(),
            SettingsTab::AiRules => self.ai_rules_settings_section(cx).into_any_element(),
            SettingsTab::Transfer => self.transfer_settings_section(cx).into_any_element(),
            SettingsTab::Security => self.security_settings_section(cx).into_any_element(),
            SettingsTab::SyncBackup => self.cloud_sync_settings_section(cx).into_any_element(),
        }
    }

    pub(in crate::features::pages::settings) fn settings_select_control<I, S>(
        &mut self,
        id: I,
        options: Vec<NyaSelectOption>,
        selected_value: S,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<I, S>
    where
        I: Into<SharedString>,
        S: Into<String>,
    {
        div()
            .w(px(260.))
            .max_w_full()
            .child(self.form_select_control(id, options, Some(selected_value.into()), disabled, cx))
    }

    pub(in crate::features::pages::settings) fn settings_select_field<I, L, S>(
        &mut self,
        id: I,
        label: L,
        desc: Option<SharedString>,
        options: Vec<NyaSelectOption>,
        selected_value: S,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<I, L, S>
    where
        I: Into<SharedString>,
        L: Into<SharedString>,
        S: Into<String>,
    {
        let palette = self.theme_palette();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight(500.))
                    .text_color(rgb(palette.text))
                    .child(label.into()),
            )
            .when_some(desc, |this, desc| {
                this.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(palette.text_dimmed))
                        .child(desc),
                )
            })
            .child(
                div()
                    .w_full()
                    .max_w(px(576.))
                    .child(self.form_select_control(
                        id,
                        options,
                        Some(selected_value.into()),
                        disabled,
                        cx,
                    )),
            )
    }
}

fn settings_tab_nav_id(tab: SettingsTab) -> &'static str {
    match tab {
        SettingsTab::General => "settings-tab-general",
        SettingsTab::Appearance => "settings-tab-appearance",
        SettingsTab::Interaction => "settings-tab-interaction",
        SettingsTab::Keybindings => "settings-tab-keybindings",
        SettingsTab::TerminalGeneral => "settings-tab-terminal-general",
        SettingsTab::Search => "settings-tab-search",
        SettingsTab::Translation => "settings-tab-translation",
        SettingsTab::AiGeneral => "settings-tab-ai-general",
        SettingsTab::AiModels => "settings-tab-ai-models",
        SettingsTab::AiRules => "settings-tab-ai-rules",
        SettingsTab::Transfer => "settings-tab-transfer",
        SettingsTab::Security => "settings-tab-security",
        SettingsTab::SyncBackup => "settings-tab-sync-backup",
    }
}

fn settings_tab_from_nav_id(id: &str) -> Option<SettingsTab> {
    match id {
        "settings-tab-general" => Some(SettingsTab::General),
        "settings-tab-appearance" => Some(SettingsTab::Appearance),
        "settings-tab-interaction" => Some(SettingsTab::Interaction),
        "settings-tab-keybindings" => Some(SettingsTab::Keybindings),
        "settings-tab-terminal-general" => Some(SettingsTab::TerminalGeneral),
        "settings-tab-search" => Some(SettingsTab::Search),
        "settings-tab-translation" => Some(SettingsTab::Translation),
        "settings-tab-ai-general" => Some(SettingsTab::AiGeneral),
        "settings-tab-ai-models" => Some(SettingsTab::AiModels),
        "settings-tab-ai-rules" => Some(SettingsTab::AiRules),
        "settings-tab-transfer" => Some(SettingsTab::Transfer),
        "settings-tab-security" => Some(SettingsTab::Security),
        "settings-tab-sync-backup" => Some(SettingsTab::SyncBackup),
        _ => None,
    }
}

/// Tauri SettingSection: rounded card with optional title/desc and body.
pub(super) fn settings_form_section(
    palette: ThemePalette,
    title: Option<Cow<'static, str>>,
    desc: Option<Cow<'static, str>>,
    content: impl IntoElement,
) -> impl IntoElement {
    div()
        .rounded_lg()
        .border_1()
        .border_color(rgba((palette.border << 8) | 0xb3))
        .bg(rgba((palette.surface << 8) | 0x99))
        .overflow_hidden()
        .when(title.is_some() || desc.is_some(), |this| {
            this.child(
                div()
                    .px_4()
                    .py_4()
                    .border_b_1()
                    .border_color(rgba((palette.surface_elevated << 8) | 0x99))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .when_some(title, |this, title| {
                        this.child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight(600.))
                                .text_color(rgb(palette.text))
                                .child(title),
                        )
                    })
                    .when_some(desc, |this, desc| {
                        this.child(
                            div()
                                .text_size(px(12.))
                                .text_color(rgb(palette.text_dimmed))
                                .child(desc),
                        )
                    }),
            )
        })
        .child(div().px_4().py_4().flex().flex_col().gap_4().child(content))
}

/// Tauri SettingRow: label/desc left, control right.
pub(super) fn settings_form_row(
    palette: ThemePalette,
    label: impl Into<SharedString>,
    desc: Option<SharedString>,
    control: impl IntoElement,
) -> impl IntoElement {
    let label = label.into();
    div()
        .flex()
        .flex_wrap()
        .items_start()
        .justify_between()
        .gap_4()
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight(500.))
                        .text_color(rgb(palette.text))
                        .child(label),
                )
                .when_some(desc, |this, desc| {
                    this.child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(palette.text_dimmed))
                            .child(desc),
                    )
                }),
        )
        .child(
            div()
                .flex_none()
                .min_w_0()
                .max_w_full()
                .flex()
                .items_center()
                .justify_end()
                .gap_2()
                .child(control),
        )
}

/// Gives an input a definite width inside a content-sized settings row control slot.
pub(super) fn settings_input_control(width: f32, input: impl IntoElement) -> impl IntoElement {
    div()
        .w(px(width))
        .max_w_full()
        .min_w_0()
        .flex()
        .child(div().min_w_0().flex_1().child(input))
}

/// Keeps an input and its trailing action on one line in a settings row.
pub(super) fn settings_input_action_control(
    width: f32,
    input: impl IntoElement,
    action: impl IntoElement,
) -> impl IntoElement {
    div()
        .w(px(width))
        .max_w_full()
        .min_w_0()
        .flex()
        .items_center()
        .gap_2()
        .child(div().min_w_0().flex_1().child(input))
        .child(div().flex_none().child(action))
}

/// Compact on/off switch control (Tauri SettingSwitch look).
pub(in crate::features::pages) fn settings_switch(
    palette: ThemePalette,
    id: impl Into<String>,
    checked: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    settings_switch_with_enabled(palette, id, checked, true, on_click)
}

pub(super) fn settings_switch_with_enabled(
    _palette: ThemePalette,
    id: impl Into<String>,
    checked: bool,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    NyaSwitch::new(id.into())
        .checked(checked)
        .disabled(!enabled)
        .on_click(move |_, window, cx| {
            if enabled {
                on_click(&ClickEvent::default(), window, cx);
            }
        })
}
