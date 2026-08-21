use gpui::{
    Context, FontWeight, IntoElement, SharedString, div,
    prelude::{FluentBuilder, ParentElement, Styled},
    px, rgb,
};
use nyaterm_ui::{NyaSwitch, NyaTabItem, NyaTabs};

use crate::features::{NyaTermApp, connections::ConnectionEditorToggle};
use crate::models::{
    ConnectionEditorField, ConnectionEditorPasswordSource, ConnectionEditorSelect,
};

use super::super::super::list::{
    ConnectionEditorRenderContext, connection_editor_select, editor_field, editor_stepper_field,
    required,
};
use super::ConnectionEditorSectionContext;

fn vnc_switch_row(
    palette: crate::theme::ThemePalette,
    id: &'static str,
    label: impl Into<SharedString>,
    description: impl Into<SharedString>,
    checked: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let label: SharedString = label.into();
    let description: SharedString = description.into();
    div()
        .rounded_md()
        .border_1()
        .border_color(rgb(palette.border))
        .px_3()
        .py_2()
        .flex()
        .items_start()
        .justify_between()
        .gap_3()
        .child(
            div()
                .min_w_0()
                .flex_1()
                .child(div().text_xs().font_weight(FontWeight(500.)).child(label))
                .child(
                    div()
                        .mt_1()
                        .text_size(px(10.))
                        .text_color(rgb(palette.text_muted))
                        .child(description),
                ),
        )
        .child(
            NyaSwitch::new(id)
                .checked(checked)
                .on_click(move |_, window, cx| {
                    on_click(&gpui::ClickEvent::default(), window, cx);
                }),
        )
}

pub(super) fn connection_editor_vnc_section(
    section: ConnectionEditorSectionContext<'_>,
    cx: &mut Context<NyaTermApp>,
) -> gpui::Div {
    let ConnectionEditorSectionContext {
        palette,
        editor,
        language,
        fields,
    } = section;
    let tr = |key: &'static str| crate::i18n::text(language, key);
    let auth_values = ["none".to_string(), "password".to_string()];
    let auth_tabs = NyaTabs::new("connection-vnc-auth-tabs")
        .items([
            NyaTabItem::new(tr("dialog.noAuthentication")),
            NyaTabItem::new(tr("dialog.password")),
        ])
        .selected_index(if editor.auth_mode == "none" { 0 } else { 1 })
        .on_select(cx.listener(move |this, index: &usize, _, cx| {
            let Some(value) = auth_values.get(*index) else {
                return;
            };
            this.set_connection_editor_select_value(
                ConnectionEditorSelect::Authentication,
                Some(value.as_str()),
                cx,
            );
        }));
    let password_source_tabs = NyaTabs::new("connection-vnc-password-source-tabs")
        .items([
            NyaTabItem::new(tr("dialog.askWhenConnecting")),
            NyaTabItem::new(tr("dialog.directPassword")),
            NyaTabItem::new(tr("dialog.savedPassword")),
        ])
        .selected_index(match editor.password_source {
            ConnectionEditorPasswordSource::Ask => 0,
            ConnectionEditorPasswordSource::Direct => 1,
            ConnectionEditorPasswordSource::Saved => 2,
        })
        .on_select(cx.listener(|this, index, _, cx| {
            let source = match *index {
                0 => ConnectionEditorPasswordSource::Ask,
                1 => ConnectionEditorPasswordSource::Direct,
                _ => ConnectionEditorPasswordSource::Saved,
            };
            this.set_connection_editor_password_source(source, cx);
        }));
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .gap_3()
                .child(div().min_w_0().flex_1().child(editor_field(
                    palette,
                    required(tr("dialog.host")),
                    ConnectionEditorField::Host,
                    fields,
                    cx,
                )))
                .child(div().w(px(150.)).flex_none().child(editor_stepper_field(
                    palette,
                    required(tr("dialog.port")),
                    ConnectionEditorField::Port,
                    fields,
                    cx,
                ))),
        )
        .child(div().flex().flex_col().gap_1().child(auth_tabs))
        .when(editor.auth_mode == "password", |this| {
            this.child(div().flex().flex_col().gap_1().child(password_source_tabs))
                .when(
                    editor.password_source == ConnectionEditorPasswordSource::Direct,
                    |this| {
                        this.child(editor_field(
                            palette,
                            tr("dialog.password"),
                            ConnectionEditorField::Password,
                            fields,
                            cx,
                        ))
                    },
                )
                .when(
                    editor.password_source == ConnectionEditorPasswordSource::Saved,
                    |this| {
                        this.child(connection_editor_select(
                            ConnectionEditorRenderContext {
                                palette,
                                fields,
                                cx,
                            },
                            "connection-editor-vnc-saved-password",
                            tr("dialog.savedPassword"),
                            ConnectionEditorSelect::SavedPassword,
                        ))
                    },
                )
        })
        .child(connection_editor_select(
            ConnectionEditorRenderContext {
                palette,
                fields,
                cx,
            },
            "connection-editor-vnc-security-mode",
            tr("dialog.vncSecurityMode"),
            ConnectionEditorSelect::VncSecurityMode,
        ))
        .child(connection_editor_select(
            ConnectionEditorRenderContext {
                palette,
                fields,
                cx,
            },
            "connection-editor-vnc-scale-mode",
            tr("dialog.vncScaleMode"),
            ConnectionEditorSelect::VncScaleMode,
        ))
        .child(vnc_switch_row(
            palette,
            "connection-vnc-clipboard",
            tr("dialog.vncClipboard"),
            tr("dialog.vncClipboardDesc"),
            editor.vnc_clipboard.enabled,
            cx.listener(|this, _, _, cx| {
                this.toggle_connection_editor_flag(ConnectionEditorToggle::VncClipboard, cx);
            }),
        ))
        .child(vnc_switch_row(
            palette,
            "connection-vnc-shared",
            tr("dialog.vncSharedSession"),
            tr("dialog.vncSharedSessionDesc"),
            editor.vnc_shared,
            cx.listener(|this, _, _, cx| {
                this.toggle_connection_editor_flag(ConnectionEditorToggle::VncShared, cx);
            }),
        ))
        .child(vnc_switch_row(
            palette,
            "connection-vnc-view-only",
            tr("dialog.vncViewOnly"),
            tr("dialog.vncViewOnlyDesc"),
            editor.vnc_view_only,
            cx.listener(|this, _, _, cx| {
                this.toggle_connection_editor_flag(ConnectionEditorToggle::VncViewOnly, cx);
            }),
        ))
        .child(vnc_switch_row(
            palette,
            "connection-vnc-reconnect",
            tr("dialog.vncReconnect"),
            tr("dialog.vncReconnectDesc"),
            editor.vnc_reconnect.enabled,
            cx.listener(|this, _, _, cx| {
                this.toggle_connection_editor_flag(ConnectionEditorToggle::VncReconnect, cx);
            }),
        ))
        .when(editor.vnc_reconnect.enabled, |this| {
            this.child(editor_stepper_field(
                palette,
                tr("dialog.vncReconnectAttempts"),
                ConnectionEditorField::VncReconnectAttempts,
                fields,
                cx,
            ))
        })
}
