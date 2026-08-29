use rust_i18n::t;

use std::borrow::Cow;

use gpui::{
    Bounds, Context, ElementInputHandler, FontWeight, IntoElement, KeyDownEvent, KeyUpEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollDelta,
    ScrollWheelEvent, SharedString, Size, canvas, div, prelude::*, px, rgb,
};
use nyaterm_remote_desktop::{
    CertificatePromptReason, RdpCertificateResponse, RdpPointerButton, RdpSessionState,
    VncScaleMode,
};

use crate::features::NyaTermApp;
use crate::widgets::small_button;

use super::runtime::{format_rdp_error, secure_attention_available};
use super::state::RdpCertificatePrompt;

impl NyaTermApp {
    pub(in crate::features) fn remote_desktop_view(
        &mut self,
        session_id: String,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let palette = self.theme_palette();
        let Some(session) = self.remote_desktop.sessions.get(&session_id) else {
            return self
                .rdp_status_view(
                    "Remote Desktop",
                    "The RDP runtime is no longer available for this tab.",
                )
                .into_any_element();
        };
        let is_rdp = self.session.metadata(&session_id).is_some_and(|metadata| {
            matches!(
                metadata.launch_config,
                crate::models::SessionLaunchConfig::Rdp(_)
            )
        });
        let secure_attention_is_available =
            secure_attention_available(is_rdp, &session.state, session.server_capabilities);
        if let Some(request) = session.certificate_request.clone() {
            return self
                .rdp_certificate_view(session_id, request, cx)
                .into_any_element();
        }
        if let Some(error) = session.error.clone()
            && matches!(session.state, RdpSessionState::Failed(_))
        {
            let retry_id = session_id.clone();
            let close_id = session_id.clone();
            return div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .bg(rgb(0x000000))
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight(600.))
                        .text_color(rgb(palette.danger))
                        .child(t!("remoteDesktop.connectionFailed")),
                )
                .child(
                    div()
                        .max_w(px(520.))
                        .px_6()
                        .text_center()
                        .text_size(px(12.))
                        .text_color(rgb(palette.text_dimmed))
                        .child(format_rdp_error(&error)),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(small_button(
                            palette,
                            format!("rdp-retry-{session_id}"),
                            t!("remoteDesktop.retry"),
                            cx.listener(move |this, _, _, cx| {
                                this.retry_rdp_runtime(&retry_id, cx);
                                cx.notify();
                            }),
                        ))
                        .child(small_button(
                            palette,
                            format!("rdp-close-{session_id}"),
                            t!("remoteDesktop.close"),
                            cx.listener(move |this, _, _, cx| {
                                this.close_session(close_id.clone(), cx);
                            }),
                        )),
                )
                .into_any_element();
        }
        if matches!(session.state, RdpSessionState::Disconnected) {
            let retry_id = session_id.clone();
            let close_id = session_id.clone();
            return div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .bg(rgb(0x000000))
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight(600.))
                        .text_color(rgb(palette.text))
                        .child(t!("remoteDesktop.disconnected")),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(small_button(
                            palette,
                            format!("rdp-disconnected-retry-{session_id}"),
                            t!("remoteDesktop.retry"),
                            cx.listener(move |this, _, _, cx| {
                                this.retry_rdp_runtime(&retry_id, cx);
                                cx.notify();
                            }),
                        ))
                        .child(small_button(
                            palette,
                            format!("rdp-disconnected-close-{session_id}"),
                            t!("remoteDesktop.close"),
                            cx.listener(move |this, _, _, cx| {
                                this.close_session(close_id.clone(), cx);
                            }),
                        )),
                )
                .into_any_element();
        }
        let (Some(framebuffer), Some(texture)) = (session.framebuffer.as_ref(), session.texture)
        else {
            let detail = match session.state {
                RdpSessionState::Connecting => t!("remoteDesktop.connecting"),
                RdpSessionState::Disconnecting => {
                    Cow::Borrowed("Disconnecting Remote Desktop session...")
                }
                RdpSessionState::Disconnected => {
                    Cow::Borrowed("Remote Desktop session is disconnected.")
                }
                _ => t!("remoteDesktop.waitingFrame"),
            };
            return self
                .rdp_status_view("Remote Desktop", detail)
                .into_any_element();
        };
        let remote_size = (framebuffer.width(), framebuffer.height());
        let scale_mode = self
            .session
            .metadata(&session_id)
            .and_then(|metadata| match &metadata.launch_config {
                crate::models::SessionLaunchConfig::Vnc(config) => Some(config.display.scale_mode),
                _ => None,
            })
            .unwrap_or(VncScaleMode::Fit);
        let cursor = session.cursor.clone();
        let cursor_texture = session.cursor_texture;
        let app = cx.entity();
        let input_entity = app.clone();
        let surface_focus = self.remote_desktop.focus().clone();
        let input_focus = surface_focus.clone();
        let input_is_active = self.session.active_id() == Some(session_id.as_str());
        let viewport_session_id = session_id.clone();
        let canvas = canvas(
            move |bounds, window, cx| {
                if input_is_active {
                    let visible_bounds = window.content_mask().bounds.intersect(&bounds);
                    if visible_bounds.size.width > px(0.) && visible_bounds.size.height > px(0.) {
                        window.handle_input(
                            &input_focus,
                            ElementInputHandler::new(visible_bounds, input_entity.clone()),
                            cx,
                        );
                    }
                }
                let app = app.clone();
                let session_id = viewport_session_id.clone();
                window.defer(cx, move |_, cx| {
                    app.update(cx, |this, _| {
                        this.update_rdp_viewport(&session_id, bounds);
                    });
                });
                remote_desktop_image_bounds(bounds, remote_size.0, remote_size.1, scale_mode)
            },
            move |_, image_bounds, window, _| {
                let _ = window.paint_dynamic_texture(image_bounds, texture);
                if let (Some(cursor), Some(cursor_texture)) = (&cursor, cursor_texture)
                    && cursor.visible
                    && remote_size.0 > 0
                    && remote_size.1 > 0
                {
                    let scale = f32::from(image_bounds.size.width) / remote_size.0 as f32;
                    let cursor_bounds = Bounds::new(
                        Point::new(
                            image_bounds.origin.x
                                + px((cursor.x as f32 - cursor.hotspot_x as f32) * scale),
                            image_bounds.origin.y
                                + px((cursor.y as f32 - cursor.hotspot_y as f32) * scale),
                        ),
                        Size::new(
                            px(cursor.width as f32 * scale),
                            px(cursor.height as f32 * scale),
                        ),
                    );
                    let _ = window.paint_dynamic_texture(cursor_bounds, cursor_texture);
                }
            },
        )
        .size_full();

        let move_id = session_id.clone();
        let left_down_id = session_id.clone();
        let right_down_id = session_id.clone();
        let middle_down_id = session_id.clone();
        let left_up_id = session_id.clone();
        let right_up_id = session_id.clone();
        let middle_up_id = session_id.clone();
        let scroll_id = session_id.clone();
        let key_down_id = session_id.clone();
        let key_up_id = session_id.clone();
        let secure_attention_id = session_id.clone();
        let focus = surface_focus;
        div()
            .id(format!("rdp-surface-{session_id}"))
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(rgb(0x000000))
            .track_focus(&focus)
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if this.send_rdp_key_down(
                    &key_down_id,
                    &event.keystroke.key,
                    event.keystroke.key_char.as_deref(),
                    event.is_held,
                    (
                        event.keystroke.modifiers.control,
                        event.keystroke.modifiers.alt,
                        event.keystroke.modifiers.platform,
                    ),
                ) {
                    cx.stop_propagation();
                    this.mark_user_activity();
                }
            }))
            .on_key_up(cx.listener(move |this, event: &KeyUpEvent, _, cx| {
                if this.send_rdp_key_up(&key_up_id, &event.keystroke.key) {
                    cx.stop_propagation();
                }
            }))
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                if this.send_rdp_pointer(&move_id, event.position, None, false) {
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.focus(this.remote_desktop.focus(), cx);
                    this.activate_workspace_pane(left_down_id.clone(), cx);
                    let _ = this.send_rdp_pointer(
                        &left_down_id,
                        event.position,
                        Some(RdpPointerButton::Left),
                        true,
                    );
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.focus(this.remote_desktop.focus(), cx);
                    this.activate_workspace_pane(right_down_id.clone(), cx);
                    let _ = this.send_rdp_pointer(
                        &right_down_id,
                        event.position,
                        Some(RdpPointerButton::Right),
                        true,
                    );
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.focus(this.remote_desktop.focus(), cx);
                    this.activate_workspace_pane(middle_down_id.clone(), cx);
                    let _ = this.send_rdp_pointer(
                        &middle_down_id,
                        event.position,
                        Some(RdpPointerButton::Middle),
                        true,
                    );
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseUpEvent, _, cx| {
                    let _ = this.send_rdp_pointer(
                        &left_up_id,
                        event.position,
                        Some(RdpPointerButton::Left),
                        false,
                    );
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseUpEvent, _, cx| {
                    let _ = this.send_rdp_pointer(
                        &right_up_id,
                        event.position,
                        Some(RdpPointerButton::Right),
                        false,
                    );
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(move |this, event: &MouseUpEvent, _, cx| {
                    let _ = this.send_rdp_pointer(
                        &middle_up_id,
                        event.position,
                        Some(RdpPointerButton::Middle),
                        false,
                    );
                    cx.stop_propagation();
                }),
            )
            .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
                let delta_y = match event.delta {
                    ScrollDelta::Pixels(delta) => f32::from(delta.y),
                    ScrollDelta::Lines(delta) => delta.y,
                };
                let button = if delta_y < 0.0 {
                    RdpPointerButton::WheelUp
                } else {
                    RdpPointerButton::WheelDown
                };
                if delta_y != 0.0 {
                    let _ = this.send_rdp_pointer(&scroll_id, event.position, Some(button), true);
                    cx.stop_propagation();
                }
            }))
            .child(canvas)
            .when(secure_attention_is_available, |surface| {
                surface.child(
                    div()
                        .absolute()
                        .top(px(8.))
                        .right(px(8.))
                        .child(small_button(
                            palette,
                            format!("rdp-secure-attention-{session_id}"),
                            "Secure Attention (Ctrl+Alt+Delete)",
                            cx.listener(move |this, _, _, cx| {
                                if this.send_rdp_secure_attention(&secure_attention_id) {
                                    this.mark_user_activity();
                                }
                                cx.notify();
                            }),
                        )),
                )
            })
            .into_any_element()
    }

    fn rdp_certificate_view(
        &mut self,
        session_id: String,
        prompt: RdpCertificatePrompt,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let reject_id = session_id.clone();
        let once_id = session_id.clone();
        let remember_id = session_id.clone();
        let changed = matches!(prompt.reason, CertificatePromptReason::Changed { .. });
        let title = if changed {
            t!("remoteDesktop.certificateChangedTitle")
        } else {
            t!("remoteDesktop.certificateTitle")
        };
        let remember_label = if changed {
            t!("remoteDesktop.replaceCertificate")
        } else {
            t!("remoteDesktop.trustAndRemember")
        };
        let risk_detail = match &prompt.reason {
            CertificatePromptReason::FirstUse => String::new(),
            CertificatePromptReason::Changed {
                previous_fingerprint,
                presented_fingerprint,
            } => format!(
                "{}\n{}: {}\n{}: {}",
                t!("remoteDesktop.certificateChangedWarning"),
                t!("remoteDesktop.previousFingerprint"),
                previous_fingerprint,
                t!("remoteDesktop.presentedFingerprint"),
                presented_fingerprint,
            ),
        };
        let request = prompt.request;
        let certificate_detail = format!(
            "{}:{}\nSHA-256 {}\nSubject: {}\nIssuer: {}\nValid: {} to {}",
            request.host,
            request.port,
            request.sha256_fingerprint,
            request.subject.as_deref().unwrap_or("unknown"),
            request.issuer.as_deref().unwrap_or("unknown"),
            request.valid_from.as_deref().unwrap_or("unknown"),
            request.valid_to.as_deref().unwrap_or("unknown")
        );
        let detail = if risk_detail.is_empty() {
            certificate_detail
        } else {
            format!("{risk_detail}\n\n{certificate_detail}")
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .px_6()
            .bg(rgb(0x000000))
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight(600.))
                    .text_color(rgb(palette.warning))
                    .child(title),
            )
            .child(
                div()
                    .max_w(px(620.))
                    .text_size(px(12.))
                    .text_color(rgb(palette.text_dimmed))
                    .child(detail),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(small_button(
                        palette,
                        format!("rdp-cert-reject-{session_id}"),
                        t!("remoteDesktop.reject"),
                        cx.listener(move |this, _, _, cx| {
                            this.resolve_rdp_certificate(
                                &reject_id,
                                RdpCertificateResponse::Reject,
                                cx,
                            );
                            cx.notify();
                        }),
                    ))
                    .when(!changed, |this| {
                        this.child(small_button(
                            palette,
                            format!("rdp-cert-once-{session_id}"),
                            t!("remoteDesktop.trustOnce"),
                            cx.listener(move |this, _, _, cx| {
                                this.resolve_rdp_certificate(
                                    &once_id,
                                    RdpCertificateResponse::TrustOnce,
                                    cx,
                                );
                                cx.notify();
                            }),
                        ))
                    })
                    .child(small_button(
                        palette,
                        format!("rdp-cert-remember-{session_id}"),
                        remember_label,
                        cx.listener(move |this, _, _, cx| {
                            this.resolve_rdp_certificate(
                                &remember_id,
                                RdpCertificateResponse::TrustAndRemember,
                                cx,
                            );
                            cx.notify();
                        }),
                    )),
            )
    }

    fn rdp_status_view(
        &self,
        title: &'static str,
        detail: impl Into<SharedString>,
    ) -> impl IntoElement {
        let detail: SharedString = detail.into();
        let palette = self.theme_palette();
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .bg(rgb(0x000000))
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight(600.))
                    .text_color(rgb(palette.text))
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(palette.text_dimmed))
                    .child(detail),
            )
    }
}

fn remote_desktop_image_bounds(
    viewport: Bounds<Pixels>,
    remote_width: u32,
    remote_height: u32,
    scale_mode: VncScaleMode,
) -> Bounds<Pixels> {
    if matches!(scale_mode, VncScaleMode::Stretch) {
        return viewport;
    }
    let viewport_width = f32::from(viewport.size.width);
    let viewport_height = f32::from(viewport.size.height);
    let scale = if matches!(scale_mode, VncScaleMode::Actual) {
        1.0
    } else {
        (viewport_width / remote_width as f32).min(viewport_height / remote_height as f32)
    };
    let width = remote_width as f32 * scale;
    let height = remote_height as f32 * scale;
    Bounds::new(
        Point::new(
            viewport.origin.x + px((viewport_width - width) * 0.5),
            viewport.origin.y + px((viewport_height - height) * 0.5),
        ),
        Size::new(px(width), px(height)),
    )
}
