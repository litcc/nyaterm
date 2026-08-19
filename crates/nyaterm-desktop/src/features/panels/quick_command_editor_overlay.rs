use gpui::{
    AnyElement, App, ClickEvent, Context, FontWeight, IntoElement, KeyDownEvent, SharedString,
    Window, div, prelude::*, px, rgb, rgba, svg,
};
use nyaterm_core::truncate_preview;
use nyaterm_ui::{NyaPopover, NyaScrollArea, NyaScrollable, NyaSwitch};

use super::{
    quick_command_color, quick_command_editor_field, quick_command_editor_script_field,
    quick_command_icon_mark,
};
use crate::features::{
    NyaTermApp, commands::QUICK_COMMAND_COLOR_OPTIONS, icons::QUICK_COMMAND_ICON_OPTIONS,
    text_inputs::TextInputSetup,
};
use crate::models::{QuickCommandEditorField, QuickCommandEditorState};
use crate::widgets::{mode_button, small_button};

impl NyaTermApp {
    pub(in crate::features) fn quick_command_editor_overlay(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.quick_command_editor_surface(self.shell.viewport_size().0, false, cx)
    }

    pub(in crate::features) fn quick_command_editor_window_view(
        &mut self,
        viewport_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.quick_command_editor_surface(viewport_width, true, cx)
    }

    fn quick_command_editor_surface(
        &mut self,
        viewport_width: f32,
        native_window: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.theme_palette();
        let editor = self
            .commands
            .quick_editor_snapshot()
            .unwrap_or_else(QuickCommandEditorState::blank);
        let title = self.quick_command_editor_title();
        let uncategorized_label = self.tr("quickCommands.uncategorized");
        let category_label_text = self.tr("quickCommands.category");
        let category_search_label = self.tr("quickCommands.searchOrCreateCategory");
        let new_category_placeholder = self.tr("quickCommands.newCategoryPlaceholder");
        let new_category_root_hint = self.tr("quickCommands.newCategoryRootHint");
        let description_label = self.tr("quickCommands.description");
        let description_placeholder = self.tr("quickCommands.descriptionPlaceholder");
        let label_name = self.tr("quickCommands.labelName");
        let label_placeholder = self.tr("quickCommands.labelPlaceholder");
        let color_tag_label = self.tr("quickCommands.colorTag");
        let pin_label = self.tr("quickCommands.pin");
        let execution_mode_label = self.tr("quickCommands.executionMode");
        let execute_label = self.tr("quickCommands.executeImmediately");
        let append_label = self.tr("quickCommands.appendOnly");
        let execute_hint = self.tr("quickCommands.executeHint");
        let append_hint = self.tr("quickCommands.appendHint");
        let command_script_label = self.tr("quickCommands.commandScript");
        let command_placeholder = self.tr("quickCommands.commandPlaceholder");
        let cancel_label = self.tr("common.cancel");
        let save_label = self.tr("common.save");
        let wide_fields = viewport_width >= 768.;
        // Built before the card, which reads `self` all the way down: creating
        // an input needs it mutably.
        let label_input = quick_command_editor_field(
            self,
            QuickCommandEditorField::Label,
            label_name,
            label_placeholder,
            editor.label.clone(),
            cx,
        );
        let description_input = quick_command_editor_field(
            self,
            QuickCommandEditorField::Description,
            description_label,
            description_placeholder,
            editor.description.clone(),
            cx,
        );
        let command_input = quick_command_editor_script_field(
            self,
            QuickCommandEditorField::Command,
            command_script_label,
            command_placeholder,
            editor.command.clone(),
            cx,
        );
        let category_search_draft = self
            .commands
            .quick_editor_category_search_draft()
            .to_string();
        let new_category_draft = self.commands.quick_editor_new_category_draft().to_string();
        let category_search_input = self.text_input_box(
            "quick-command.editor.category",
            &category_search_draft,
            TextInputSetup::placeholder(category_search_label),
            cx,
        );
        let new_category_input = self.text_input_box(
            "quick-command.editor.new-category",
            &new_category_draft,
            TextInputSetup::placeholder(new_category_placeholder),
            cx,
        );
        let category_label = editor
            .category_id
            .as_deref()
            .and_then(|id| {
                self.commands
                    .quick_command_categories()
                    .iter()
                    .find(|category| category.id == id)
            })
            .map(|category| category.name.clone())
            .unwrap_or_else(|| uncategorized_label.to_string());
        let category_draft = self
            .commands
            .quick_editor_category_search_draft()
            .trim()
            .to_string();
        let category_query = category_draft.to_lowercase();
        let category_display = if editor.category_draft.trim().is_empty() {
            category_label.clone()
        } else {
            self.tr("quickCommands.createCategory")
                .replace("{{name}}", editor.category_draft.trim())
        };
        let mut color_swatches = div().flex().items_center().gap_2().flex_wrap();
        for option in QUICK_COMMAND_COLOR_OPTIONS {
            let selected = editor.color_tag.as_deref() == option && editor.icon_tag.is_none();
            color_swatches = color_swatches.child(quick_command_color_swatch(
                palette,
                option,
                selected,
                cx.listener(move |this, _, _, cx| {
                    this.set_quick_command_editor_color(option, cx);
                }),
            ));
        }
        let icon_options = QUICK_COMMAND_ICON_OPTIONS
            .iter()
            .copied()
            .flatten()
            .collect::<Vec<_>>();
        let mut icon_grid = div().grid().grid_cols(6).gap_1();
        for option in icon_options {
            let selected = editor.icon_tag.as_deref() == Some(option);
            icon_grid = icon_grid.child(quick_command_icon_option(
                palette,
                option,
                editor.color_tag.as_deref(),
                selected,
                cx.listener(move |this, _, _, cx| {
                    this.set_quick_command_editor_icon(Some(option), cx);
                }),
            ));
        }
        let mut category_choices = div().w_full().flex().flex_col();
        if category_draft.is_empty() {
            let uncategorized_selected =
                editor.category_id.as_deref().unwrap_or_default().is_empty();
            category_choices = category_choices.child(quick_command_category_choice(
                palette,
                "quick-command-editor-category-none".to_string(),
                uncategorized_label.to_string(),
                uncategorized_selected,
                cx.listener(|this, _, _, cx| {
                    this.set_quick_command_editor_category(None, cx);
                }),
            ));
        }
        for category in self
            .commands
            .quick_command_categories()
            .to_vec()
            .into_iter()
            .filter(|category| {
                category_query.is_empty() || category.name.to_lowercase().contains(&category_query)
            })
        {
            let category_id = category.id.clone();
            let selected = editor.category_draft.trim().is_empty()
                && editor.category_id.as_deref() == Some(category_id.as_str());
            category_choices = category_choices.child(quick_command_category_choice(
                palette,
                format!("quick-command-editor-category-{}", category.id),
                truncate_preview(&category.name, 22),
                selected,
                cx.listener(move |this, _, _, cx| {
                    this.set_quick_command_editor_category(Some(category_id.clone()), cx);
                }),
            ));
        }
        let new_category_name = self
            .commands
            .quick_editor_new_category_draft()
            .trim()
            .to_string();
        let new_category_duplicate = !new_category_name.is_empty()
            && self
                .commands
                .quick_command_categories()
                .iter()
                .any(|category| {
                    category
                        .name
                        .trim()
                        .eq_ignore_ascii_case(&new_category_name)
                });
        let category_list = NyaScrollArea::new("quick-command-editor-category-list")
            .max_h(px(258.))
            .child(category_choices);
        let new_category_add = div()
            .id("quick-command-editor-new-category-add")
            .size(px(28.))
            .rounded_sm()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(if new_category_name.is_empty() || new_category_duplicate {
                rgb(palette.text_dimmed)
            } else {
                rgb(palette.text)
            })
            .hover(|style| style.bg(rgb(palette.hover)))
            .child(svg().size(px(14.)).path("icons/plus.svg"))
            .when(
                !(new_category_name.is_empty() || new_category_duplicate),
                |this| {
                    this.on_click(cx.listener(|this, _, _, cx| {
                        this.commit_quick_command_editor_new_category(cx);
                    }))
                },
            );
        let category_picker_open = self.commands.quick_editor_category_picker_is_open();
        let category_picker_trigger = div()
            .id("quick-command-editor-category-trigger")
            .h(px(36.))
            .w_full()
            .px_3()
            .rounded_sm()
            .border_1()
            .border_color(if category_picker_open {
                rgb(palette.focus_ring)
            } else {
                rgb(palette.border)
            })
            .bg(rgb(palette.input))
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .hover(|style| style.border_color(rgb(palette.link)))
            .child(
                div()
                    .min_w_0()
                    .text_sm()
                    .text_color(if editor.category_id.is_some() {
                        rgb(palette.text)
                    } else {
                        rgb(palette.text_muted)
                    })
                    .child(truncate_preview(&category_display, 48)),
            )
            .child(
                svg()
                    .size(px(14.))
                    .text_color(rgb(palette.text_muted))
                    .path("icons/chevron-down.svg"),
            );
        let category_picker_content = div()
            .w(px((viewport_width - 38.).clamp(320., 560.)))
            .rounded_md()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(if native_window {
                rgb(palette.surface)
            } else {
                self.shell_surface_color(palette.surface)
            })
            .shadow_lg()
            .child(div().p_1().child(category_search_input))
            .child(category_list)
            .child(
                div()
                    .border_t_1()
                    .border_color(rgb(palette.border))
                    .p_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .on_key_down(cx.listener(
                                        |this, event: &KeyDownEvent, _, cx| {
                                            if event.keystroke.key == "enter" {
                                                this.commit_quick_command_editor_new_category(cx);
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                                    .child(new_category_input),
                            )
                            .child(new_category_add),
                    )
                    .child(
                        div()
                            .px_1()
                            .pt_1()
                            .text_size(px(10.))
                            .text_color(rgb(palette.text_dimmed))
                            .child(new_category_root_hint),
                    ),
            );
        let category_picker = NyaPopover::new(
            "quick-command-editor-category-popover",
            category_picker_trigger,
            category_picker_content,
        )
        .appearance(false)
        .open(category_picker_open)
        .on_open_change(cx.listener(|this, open, window, cx| {
            this.set_quick_command_editor_category_picker_open(*open, cx);
            if *open {
                this.focus_text_input_if_present("quick-command.editor.category", window, cx);
            }
        }));

        let icon_picker_open = self.commands.quick_editor_icon_picker_is_open();
        let icon_picker_trigger = div()
            .id("quick-command-editor-icon-trigger")
            .size(px(24.))
            .rounded_full()
            .border_2()
            .border_dashed()
            .border_color(if icon_picker_open {
                rgb(palette.focus_ring)
            } else {
                rgb(palette.text_muted)
            })
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_center()
            .hover(|style| style.border_color(rgb(palette.link)))
            .child(
                svg()
                    .size(px(12.))
                    .text_color(rgb(palette.text_muted))
                    .path("icons/plus.svg"),
            );
        let icon_picker_content = div()
            .w(px(192.))
            .max_h(px(220.))
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(if native_window {
                rgb(palette.surface)
            } else {
                self.shell_surface_color(palette.surface)
            })
            .shadow_lg()
            .child(icon_grid);
        let icon_picker = NyaPopover::new(
            "quick-command-editor-icon-popover",
            icon_picker_trigger,
            icon_picker_content,
        )
        .appearance(false)
        .open(icon_picker_open)
        .on_open_change(cx.listener(|this, open, _, cx| {
            this.set_quick_command_editor_icon_picker_open(*open, cx);
        }));
        let can_save = !editor.label.trim().is_empty() && !editor.command.trim().is_empty();
        let dialog_bg = if native_window {
            rgb(palette.bg)
        } else {
            self.shell_surface_color(palette.bg)
        };

        div()
            .id(SharedString::from("quick-command-editor-overlay"))
            .when(!native_window, |this| {
                this.absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .bg(rgba(0x00000080))
                    .p_4()
            })
            .when(native_window, |this| this.size_full().bg(rgb(palette.bg)))
            .flex()
            .flex_col()
            .items_center()
            .when(!native_window, |this| this.justify_center())
            .when(native_window, |this| this.justify_start())
            .overflow_hidden()
            .track_focus(self.commands.quick_editor_focus())
            // No blanket focus grab: it kept the surface "focused" for the old
            // label-div fields, and would now steal focus back from whichever
            // box the pointer just landed on, since click follows mouse-down.
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                this.handle_quick_command_editor_key_down(event, cx);
            }))
            .child(
                div()
                    .id(SharedString::from("quick-command-editor-dialog"))
                    .w_full()
                    .when(native_window, |this| this.size_full())
                    .when(!native_window, |this| this.max_w(px(560.)).max_h(px(640.)))
                    .when(!native_window, |this| {
                        this.rounded_md()
                            .border_1()
                            .border_color(rgb(palette.border))
                            .shadow_lg()
                    })
                    .bg(dialog_bg)
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .when(!native_window, |this| {
                        this.child(
                            div().flex().items_center().gap_3().px_4().pt_4().child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight(800.))
                                    .text_color(rgb(palette.text))
                                    .child(title),
                            ),
                        )
                    })
                    .child(
                        div()
                            .id("quick-command-editor-body")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .p_4()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .when(editor.error.is_some(), |this| {
                                this.child(
                                    div()
                                        .rounded_sm()
                                        .border_1()
                                        .border_color(rgb(palette.danger))
                                        .bg(rgb(0x1f0b0b))
                                        .p_2()
                                        .text_xs()
                                        .text_color(rgb(palette.danger))
                                        .child(editor.error.clone().unwrap_or_default()),
                                )
                            })
                            .child(
                                div()
                                    .flex()
                                    .when(wide_fields, |this| this.flex_row())
                                    .when(!wide_fields, |this| this.flex_col())
                                    .gap_3()
                                    .child(div().min_w_0().flex_1().child(label_input))
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .text_size(px(10.))
                                                    .text_color(rgb(palette.text_muted))
                                                    .child(category_label_text),
                                            )
                                            .child(div().mt_1().child(category_picker)),
                                    ),
                            )
                            .child(description_input)
                            .child(
                                div()
                                    .rounded_sm()
                                    .border_1()
                                    .border_color(rgb(palette.border))
                                    .bg(rgb(palette.input))
                                    .p_2()
                                    .flex()
                                    .items_start()
                                    .gap_4()
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .child(
                                                div().flex().items_center().gap_2().child(
                                                    div()
                                                        .text_size(px(10.))
                                                        .text_color(rgb(palette.text_muted))
                                                        .child(color_tag_label),
                                                ),
                                            )
                                            .child(
                                                div()
                                                    .mt_2()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(color_swatches)
                                                    .when(editor.icon_tag.is_some(), |this| {
                                                        this.child(
                                                            div()
                                                                .size(px(24.))
                                                                .rounded_full()
                                                                .border_2()
                                                                .border_color(rgb(palette.text))
                                                                .bg(rgb(palette.input))
                                                                .flex()
                                                                .items_center()
                                                                .justify_center()
                                                                .child(quick_command_icon_mark(
                                                                    palette,
                                                                    editor.icon_tag.as_deref(),
                                                                    editor.color_tag.as_deref(),
                                                                )),
                                                        )
                                                    })
                                                    .child(icon_picker),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .pl_4()
                                            .border_l_1()
                                            .border_color(rgb(palette.border))
                                            .flex()
                                            .flex_col()
                                            .items_end()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_size(px(10.))
                                                    .text_color(rgb(palette.text_muted))
                                                    .child(pin_label),
                                            )
                                            .child(
                                                NyaSwitch::new("quick-command-editor-pinned")
                                                    .checked(editor.pinned)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.toggle_quick_command_editor_pinned(cx);
                                                    })),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .child(
                                        div()
                                            .text_size(px(10.))
                                            .text_color(rgb(palette.text_muted))
                                            .child(execution_mode_label),
                                    )
                                    .child(
                                        div()
                                            .mt_2()
                                            .grid()
                                            .grid_cols(2)
                                            .gap_2()
                                            .child(mode_button(
                                                "quick-command-editor-execute",
                                                execute_label,
                                                editor.execution_mode != "append",
                                                self.theme_palette(),
                                                cx.listener(|this, _, _, cx| {
                                                    this.set_quick_command_editor_execution_mode(
                                                        "execute", cx,
                                                    );
                                                }),
                                            ))
                                            .child(mode_button(
                                                "quick-command-editor-append",
                                                append_label,
                                                editor.execution_mode == "append",
                                                self.theme_palette(),
                                                cx.listener(|this, _, _, cx| {
                                                    this.set_quick_command_editor_execution_mode(
                                                        "append", cx,
                                                    );
                                                }),
                                            )),
                                    )
                                    .child(
                                        div()
                                            .mt_1()
                                            .text_size(px(10.))
                                            .text_color(rgb(palette.text_muted))
                                            .child(if editor.execution_mode == "append" {
                                                append_hint
                                            } else {
                                                execute_hint
                                            }),
                                    ),
                            )
                            .child(div().min_h(px(128.)).flex().flex_col().child(command_input)),
                    )
                    .child(
                        div()
                            .h(px(52.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .px_5()
                            .py_3()
                            .border_t_1()
                            .border_color(rgb(palette.border))
                            .bg(rgb(palette.section_header))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(small_button(
                                        palette,
                                        "quick-command-editor-cancel",
                                        cancel_label,
                                        cx.listener(|this, _, _, cx| {
                                            this.close_quick_command_editor(cx);
                                        }),
                                    ))
                                    .child(
                                        div()
                                            .when(can_save, |this| {
                                                this.child(small_button(
                                                    palette,
                                                    "quick-command-editor-save",
                                                    save_label,
                                                    cx.listener(|this, _, _, cx| {
                                                        this.save_quick_command_editor(cx);
                                                    }),
                                                ))
                                            })
                                            .when(!can_save, |this| {
                                                this.child(
                                                    div()
                                                        .id("quick-command-editor-save-disabled")
                                                        .h(px(28.))
                                                        .px_3()
                                                        .flex()
                                                        .items_center()
                                                        .rounded_sm()
                                                        .border_1()
                                                        .border_color(rgb(palette.border))
                                                        .bg(rgb(palette.surface_elevated))
                                                        .text_color(rgb(palette.text_dimmed))
                                                        .text_xs()
                                                        .child(save_label),
                                                )
                                            }),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }
}

fn quick_command_category_choice(
    palette: crate::theme::ThemePalette,
    id: String,
    label: String,
    selected: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(id))
        .h(px(36.))
        .w_full()
        .px_3()
        .bg(if selected {
            rgb(0x1d3357)
        } else {
            rgb(palette.surface)
        })
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .text_size(px(13.))
        .text_color(if selected {
            rgb(palette.link)
        } else {
            rgb(palette.text)
        })
        .hover(|style| style.bg(rgb(palette.hover)))
        .child(div().min_w_0().text_ellipsis().child(label))
        .when(selected, |this| {
            this.child(
                svg()
                    .size(px(14.))
                    .text_color(rgb(palette.link))
                    .path("icons/check.svg"),
            )
        })
        .on_click(on_click)
}

fn quick_command_color_swatch(
    palette: crate::theme::ThemePalette,
    color_tag: Option<&'static str>,
    selected: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let id = format!(
        "quick-command-color-swatch-{}",
        color_tag.unwrap_or("default")
    );
    div()
        .id(SharedString::from(id))
        .size(px(22.))
        .rounded_full()
        .border_2()
        .border_color(if selected {
            rgb(palette.text)
        } else {
            rgb(palette.border)
        })
        .bg(quick_command_color(palette, color_tag))
        .cursor_pointer()
        .on_click(on_click)
        .hover(|style| style.border_color(rgb(palette.link)))
}

fn quick_command_icon_option(
    palette: crate::theme::ThemePalette,
    icon_tag: &'static str,
    color_tag: Option<&str>,
    selected: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!(
            "quick-command-icon-option-{icon_tag}"
        )))
        .size(px(24.))
        .rounded_sm()
        .border_1()
        .border_color(if selected {
            rgb(palette.text)
        } else {
            rgb(palette.border)
        })
        .bg(if selected {
            rgb(palette.hover)
        } else {
            rgb(palette.input)
        })
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_center()
        .on_click(on_click)
        .hover(|style| style.border_color(rgb(palette.link)))
        .child(quick_command_icon_mark(palette, Some(icon_tag), color_tag))
}
