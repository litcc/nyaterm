use gpui::{
    AnyElement, App, ClickEvent, Context, FontWeight, IntoElement, KeyDownEvent, MouseButton,
    Point, Render, SharedString, Window, div, prelude::*, px, rgb, rgba, svg, uniform_list,
};

use super::super::{filtered_quick_commands, quick_command_category_options};
use crate::features::{NyaTermApp, text_inputs::TextInputSetup};
use crate::models::{QuickCommandSortMode, QuickCommandViewMode};
use crate::widgets::small_button;
use nyaterm_ui::{NyaDropdownMenu, NyaMenuItem, NyaScrollable, NyaSearchInput, NyaTooltip};

mod rows;
use rows::quick_command_tile_column_count;
mod sidebar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuickCommandDragKind {
    Command,
    Category,
}

#[derive(Clone, Debug)]
struct QuickCommandDragPayload {
    kind: QuickCommandDragKind,
    id: String,
    label: String,
}

struct QuickCommandDragPreview {
    payload: QuickCommandDragPayload,
    position: Point<gpui::Pixels>,
}

impl Render for QuickCommandDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let icon = match self.payload.kind {
            QuickCommandDragKind::Command => "icons/conn/terminal.svg",
            QuickCommandDragKind::Category => "icons/conn/folder.svg",
        };
        div()
            .absolute()
            .left(self.position.x - px(88.))
            .top(self.position.y - px(16.))
            .w(px(196.))
            .h(px(34.))
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x388bfd))
            .bg(rgba(0x0d1117ee))
            .shadow_lg()
            .child(svg().size(px(13.)).path(icon).text_color(rgb(0x58a6ff)))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .text_xs()
                    .font_weight(FontWeight(600.))
                    .text_color(rgb(0xe5edf7))
                    .overflow_hidden()
                    .child(nyaterm_core::truncate_preview(&self.payload.label, 24)),
            )
    }
}

#[derive(Clone, Copy)]
struct QuickCommandToolbarContext {
    palette: crate::theme::ThemePalette,
    popover_bg: gpui::Rgba,
}

struct QuickCommandSortMenuConfig {
    current: QuickCommandSortMode,
    sort_label: &'static str,
    created_label: &'static str,
    name_label: &'static str,
    usage_label: &'static str,
    custom_label: &'static str,
}

struct QuickCommandViewMenuConfig {
    current: QuickCommandViewMode,
    icon_path: &'static str,
    view_label: &'static str,
    list_label: &'static str,
    compact_label: &'static str,
    tile_label: &'static str,
}

struct QuickCommandAiPopoverConfig {
    open: bool,
    prompt: String,
    prompt_input: AnyElement,
    button_label: &'static str,
    generate_label: &'static str,
}

impl NyaTermApp {
    pub(in crate::features) fn quick_commands_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let filtered_commands = filtered_quick_commands(
            self.commands.quick_commands(),
            self.commands.quick_command_categories(),
            self.commands.quick_search_draft(),
            self.commands.quick_selected_category(),
            self.commands.quick_sort_mode(),
        );
        let total_commands = self.commands.quick_commands().len();
        let visible_commands = filtered_commands.len();
        let _pinned_commands = self
            .commands
            .quick_commands()
            .iter()
            .filter(|command| command.pinned.unwrap_or_default())
            .count();
        let categories = quick_command_category_options(
            self.commands.quick_commands(),
            self.commands.quick_command_categories(),
            self.tr("quickCommands.allCategories"),
            self.tr("quickCommands.uncategorized"),
        );
        let palette = self.theme_palette();
        let popover_bg = self.shell_surface_color(palette.surface);
        let toolbar_context = QuickCommandToolbarContext {
            palette,
            popover_bg,
        };
        let search_draft = self.commands.quick_search_draft().to_string();
        let ai_prompt_draft = self.commands.quick_ai_prompt_draft().to_string();
        let search_field = self.text_input(
            "quick-command.search",
            &search_draft,
            TextInputSetup::placeholder(self.tr("quickCommands.search")),
            cx,
        );
        let ai_prompt_input = self
            .text_input_box(
                "quick-command.ai-prompt",
                &ai_prompt_draft,
                TextInputSetup::placeholder(self.tr("ai.placeholder")),
                cx,
            )
            .into_any_element();
        let view_icon = match self.commands.quick_view_mode() {
            QuickCommandViewMode::List => "icons/view-list.svg",
            QuickCommandViewMode::Compact => "icons/view-compact.svg",
            QuickCommandViewMode::Tile => "icons/view-grid.svg",
        };

        let category_sidebar = self.quick_command_category_sidebar(categories, palette, cx);
        let view_mode = self.commands.quick_view_mode();
        let tile_columns = quick_command_tile_column_count(
            self.shell.viewport_size().0,
            self.shell.left_panel_width(),
            self.shell.right_panel_width(),
            !self.shell.left_panel_collapsed(),
            !self.shell.right_panel_collapsed(),
        );
        let row_scroll = self.commands.quick_row_scroll().clone();
        let logical_row_height = match view_mode {
            QuickCommandViewMode::Tile => 32.,
            QuickCommandViewMode::Compact => 38.,
            QuickCommandViewMode::List => 50.,
        };
        let rows = if filtered_commands.is_empty() {
            div()
                .id(SharedString::from("quick-command-rows-scroll"))
                .flex_1()
                .min_h_0()
                .overflow_scrollbar()
                .child(
                    div()
                        .mt_8()
                        .mx_auto()
                        .w_full()
                        .max_w(px(384.))
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(palette.border))
                        .p_4()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .text_xs()
                        .text_color(rgb(palette.text_muted))
                        .line_height(px(18.))
                        .opacity(0.72)
                        .child(
                            svg()
                                .size(px(24.))
                                .flex_none()
                                .text_color(rgb(palette.text_muted))
                                .path("icons/conn/terminal.svg"),
                        )
                        .child(self.tr("quickCommands.noCommandsFound"))
                        .when(total_commands == 0, |this| {
                            this.child(small_button(
                                palette,
                                "quick-command-empty-add",
                                self.tr("quickCommands.addCommand"),
                                cx.listener(|this, _, window, cx| {
                                    this.open_new_quick_command_editor(window, cx);
                                }),
                            ))
                        }),
                )
                .into_any_element()
        } else {
            let logical_row_count = match view_mode {
                QuickCommandViewMode::Tile => filtered_commands.len().div_ceil(tile_columns),
                QuickCommandViewMode::Compact | QuickCommandViewMode::List => {
                    filtered_commands.len()
                }
            };
            uniform_list(
                "quick-command-rows-scroll",
                logical_row_count,
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    let mut rows = Vec::with_capacity(range.len());
                    for logical_index in range {
                        let (start, end) = match view_mode {
                            QuickCommandViewMode::Tile => {
                                let start = logical_index.saturating_mul(tile_columns);
                                (
                                    start,
                                    start
                                        .saturating_add(tile_columns)
                                        .min(filtered_commands.len()),
                                )
                            }
                            QuickCommandViewMode::Compact | QuickCommandViewMode::List => {
                                (logical_index, logical_index.saturating_add(1))
                            }
                        };
                        if start >= end || end > filtered_commands.len() {
                            continue;
                        }
                        let command_items =
                            this.quick_command_items(&filtered_commands[start..end], palette, cx);
                        let row = div()
                            .h(px(logical_row_height))
                            .w_full()
                            .flex_none()
                            .when(view_mode == QuickCommandViewMode::Tile, |this| {
                                this.grid().grid_cols(tile_columns as u16).gap_1()
                            })
                            .when(view_mode != QuickCommandViewMode::Tile, |this| {
                                this.flex().items_start()
                            })
                            .children(command_items);
                        rows.push(row);
                    }
                    rows
                }),
            )
            .flex_1()
            .min_h_0()
            .track_scroll(&row_scroll)
            .into_any_element()
        };

        // Tauri QuickCommands: PanelHeader-like strip with search + compact actions,
        // then category sidebar + command list (no page metrics cards).
        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(self.shell_transparent_color(palette.surface))
            .child(
                div()
                    .h(px(36.))
                    .flex_none()
                    .px_2()
                    .border_b_1()
                    .border_color(rgb(palette.border))
                    .bg(self.shell_transparent_color(palette.section_header))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(FontWeight(700.))
                            .text_color(rgb(palette.text_muted))
                            .child(self.tr("panel.quickCommands")),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(palette.text_dimmed))
                            .child(if visible_commands == total_commands {
                                total_commands.to_string()
                            } else {
                                format!("{visible_commands}/{total_commands}")
                            }),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .w(px(144.))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.close_quick_command_toolbar_popovers();
                                    cx.notify();
                                }),
                            )
                            .child(
                                NyaSearchInput::new("quick-command-search-input", &search_field)
                                    .on_key_down(cx.listener(
                                        |this, event: &KeyDownEvent, _, cx| {
                                            if event.keystroke.key == "escape" {
                                                cx.stop_propagation();
                                                this.commands.clear_quick_filters();
                                                this.reset_text_input(
                                                    "quick-command.search",
                                                    "",
                                                    cx,
                                                );
                                                this.shell.set_status(
                                                    "quick command filters cleared".to_string(),
                                                );
                                                cx.notify();
                                            }
                                        },
                                    )),
                            ),
                    )
                    .child(quick_command_toolbar_divider(palette))
                    .child(quick_command_sort_menu_button(
                        QuickCommandSortMenuConfig {
                            current: self.commands.quick_sort_mode(),
                            sort_label: self.tr("quickCommands.sort"),
                            created_label: self.tr("quickCommands.sortByCreated"),
                            name_label: self.tr("quickCommands.sortByName"),
                            usage_label: self.tr("quickCommands.sortByUseCount"),
                            custom_label: self.tr("quickCommands.sortCustom"),
                        },
                        cx,
                    ))
                    .child(quick_command_view_menu_button(
                        QuickCommandViewMenuConfig {
                            current: self.commands.quick_view_mode(),
                            icon_path: view_icon,
                            view_label: self.tr("quickCommands.viewMode"),
                            list_label: self.tr("quickCommands.listMode"),
                            compact_label: self.tr("quickCommands.compactListMode"),
                            tile_label: self.tr("quickCommands.tileMode"),
                        },
                        cx,
                    ))
                    .child(quick_command_toolbar_divider(palette))
                    .child(quick_command_toolbar_icon_button(
                        palette,
                        "quick-command-add",
                        "icons/conn/add.svg",
                        false,
                        self.tr("quickCommands.addCommand"),
                        cx.listener(|this, _, window, cx| {
                            this.close_quick_command_toolbar_popovers();
                            this.open_new_quick_command_editor(window, cx);
                        }),
                    ))
                    .child(quick_command_toolbar_icon_button(
                        palette,
                        "quick-command-import",
                        "icons/import.svg",
                        self.commands.quick_import_path_prompt().is_some(),
                        self.tr("quickCommands.import"),
                        cx.listener(|this, _, window, cx| {
                            this.close_quick_command_toolbar_popovers();
                            this.open_quick_command_import_dialog(window, cx);
                        }),
                    ))
                    .child(quick_command_toolbar_icon_button(
                        palette,
                        "quick-command-export",
                        "icons/menu/export.svg",
                        false,
                        self.tr("quickCommands.export"),
                        cx.listener(|this, _, _, cx| {
                            this.close_quick_command_toolbar_popovers();
                            this.prompt_quick_command_export(cx);
                        }),
                    ))
                    .child(quick_command_toolbar_divider(palette))
                    .child(quick_command_ai_popover_button(
                        toolbar_context,
                        QuickCommandAiPopoverConfig {
                            open: self.commands.quick_ai_popover_is_open(),
                            prompt: self.commands.quick_ai_prompt_draft().to_string(),
                            prompt_input: ai_prompt_input,
                            button_label: self.tr("ai.generateCommand"),
                            generate_label: self.tr("ai.generate"),
                        },
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            let changed = this.commands.close_quick_toolbar_popovers();
                            if changed {
                                cx.notify();
                            }
                        }),
                    )
                    .child(category_sidebar)
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .min_h_0()
                            .h_full()
                            .p(px(6.))
                            .relative()
                            .flex()
                            .flex_col()
                            .child(rows)
                            .vertical_scrollbar(&row_scroll),
                    ),
            )
    }
}

fn quick_command_sort_menu_button(
    config: QuickCommandSortMenuConfig,
    cx: &mut Context<NyaTermApp>,
) -> impl IntoElement {
    let QuickCommandSortMenuConfig {
        current,
        sort_label,
        created_label,
        name_label,
        usage_label,
        custom_label,
    } = config;
    NyaDropdownMenu::new("quick-command-sort")
        .icon("icons/conn/sort.svg")
        .icon_size(px(14.))
        .selected(current != QuickCommandSortMode::Created)
        .tooltip(format!(
            "{} · {}",
            sort_label,
            quick_command_sort_mode_label(
                current,
                created_label,
                name_label,
                usage_label,
                custom_label,
            )
        ))
        .min_width(px(154.))
        .items([
            NyaMenuItem::action(created_label)
                .checked(current == QuickCommandSortMode::Created)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_quick_command_sort_mode(QuickCommandSortMode::Created, cx);
                })),
            NyaMenuItem::action(name_label)
                .checked(current == QuickCommandSortMode::Name)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_quick_command_sort_mode(QuickCommandSortMode::Name, cx);
                })),
            NyaMenuItem::action(usage_label)
                .checked(current == QuickCommandSortMode::Usage)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_quick_command_sort_mode(QuickCommandSortMode::Usage, cx);
                })),
            NyaMenuItem::action(custom_label)
                .checked(current == QuickCommandSortMode::Custom)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_quick_command_sort_mode(QuickCommandSortMode::Custom, cx);
                })),
        ])
}

fn quick_command_view_menu_button(
    config: QuickCommandViewMenuConfig,
    cx: &mut Context<NyaTermApp>,
) -> impl IntoElement {
    let QuickCommandViewMenuConfig {
        current,
        icon_path,
        view_label,
        list_label,
        compact_label,
        tile_label,
    } = config;
    NyaDropdownMenu::new("quick-command-view")
        .icon(icon_path)
        .icon_size(px(14.))
        .selected(true)
        .tooltip(format!(
            "{} · {}",
            view_label,
            quick_command_view_mode_label(current, list_label, compact_label, tile_label)
        ))
        .min_width(px(154.))
        .items([
            NyaMenuItem::action(list_label)
                .checked(current == QuickCommandViewMode::List)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_quick_command_view_mode(QuickCommandViewMode::List, cx);
                })),
            NyaMenuItem::action(compact_label)
                .checked(current == QuickCommandViewMode::Compact)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_quick_command_view_mode(QuickCommandViewMode::Compact, cx);
                })),
            NyaMenuItem::action(tile_label)
                .checked(current == QuickCommandViewMode::Tile)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_quick_command_view_mode(QuickCommandViewMode::Tile, cx);
                })),
        ])
}

fn quick_command_ai_popover_button(
    context: QuickCommandToolbarContext,
    config: QuickCommandAiPopoverConfig,
    cx: &mut Context<NyaTermApp>,
) -> impl IntoElement {
    let QuickCommandToolbarContext {
        palette,
        popover_bg,
    } = context;
    let QuickCommandAiPopoverConfig {
        open,
        prompt,
        prompt_input,
        button_label,
        generate_label,
    } = config;
    let can_generate = !prompt.trim().is_empty();
    div()
        .relative()
        .child(quick_command_toolbar_icon_button(
            palette,
            "quick-command-ai",
            "icons/ai.svg",
            open,
            button_label,
            cx.listener(|this, _, window, cx| {
                this.toggle_quick_command_ai_popover(window, cx);
            }),
        ))
        .when(open, |this| {
            this.child(
                div()
                    .id(SharedString::from("quick-command-ai-popover"))
                    .absolute()
                    .top(px(28.))
                    .right_0()
                    .w(px(320.))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(popover_bg)
                    .shadow_lg()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight(600.))
                            .text_color(rgb(palette.text))
                            .child(button_label),
                    )
                    .child(
                        div()
                            .id(SharedString::from("quick-command-ai-input"))
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                cx.stop_propagation();
                                this.handle_quick_command_ai_prompt_key_down(event, window, cx);
                            }))
                            .child(prompt_input),
                    )
                    .child(
                        div().flex().justify_end().child(
                            div()
                                .id(SharedString::from("quick-command-ai-submit"))
                                .h(px(24.))
                                .px_2()
                                .rounded_md()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_size(px(11.))
                                .font_weight(FontWeight(600.))
                                .text_color(if prompt.trim().is_empty() {
                                    rgb(palette.text_dimmed)
                                } else {
                                    rgb(palette.text)
                                })
                                .bg(if prompt.trim().is_empty() {
                                    rgb(palette.input)
                                } else {
                                    rgb(palette.hover)
                                })
                                .when(can_generate, |this| {
                                    this.cursor_pointer().hover(|this| {
                                        this.bg(rgb(palette.surface_elevated))
                                            .text_color(rgb(palette.text))
                                    })
                                })
                                .child(
                                    svg()
                                        .size(px(13.))
                                        .flex_none()
                                        .path("icons/ai.svg")
                                        .text_color(if prompt.trim().is_empty() {
                                            rgb(palette.text_dimmed)
                                        } else {
                                            rgb(palette.text)
                                        }),
                                )
                                .child(generate_label)
                                .when(can_generate, |this| {
                                    this.on_click(cx.listener(|this, _, window, cx| {
                                        this.submit_quick_command_ai_prompt(window, cx);
                                    }))
                                }),
                        ),
                    ),
            )
        })
}

fn quick_command_toolbar_icon_button(
    palette: crate::theme::ThemePalette,
    id: &'static str,
    icon_path: &'static str,
    active: bool,
    tooltip: impl Into<String>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let tooltip = tooltip.into();
    div()
        .id(SharedString::from(id))
        .size(px(24.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .text_color(if active {
            rgb(palette.link)
        } else {
            rgb(palette.text_muted)
        })
        .cursor_pointer()
        .hover(|this| this.bg(rgb(palette.hover)).text_color(rgb(palette.text)))
        .child(
            svg()
                .size(px(15.))
                .flex_none()
                .path(icon_path)
                .text_color(if active {
                    rgb(palette.link)
                } else {
                    rgb(palette.text_muted)
                }),
        )
        .tooltip(move |window, cx| NyaTooltip::new(tooltip.clone()).build(window, cx))
        .on_click(on_click)
}

fn quick_command_toolbar_divider(palette: crate::theme::ThemePalette) -> impl IntoElement {
    div()
        .mx_1()
        .h(px(16.))
        .w(px(1.))
        .flex_none()
        .bg(rgb(palette.border))
}

fn quick_command_view_mode_label(
    mode: QuickCommandViewMode,
    list_label: &'static str,
    compact_label: &'static str,
    tile_label: &'static str,
) -> &'static str {
    match mode {
        QuickCommandViewMode::List => list_label,
        QuickCommandViewMode::Compact => compact_label,
        QuickCommandViewMode::Tile => tile_label,
    }
}

fn quick_command_sort_mode_label(
    mode: QuickCommandSortMode,
    created_label: &'static str,
    name_label: &'static str,
    usage_label: &'static str,
    custom_label: &'static str,
) -> &'static str {
    match mode {
        QuickCommandSortMode::Created => created_label,
        QuickCommandSortMode::Name => name_label,
        QuickCommandSortMode::Usage => usage_label,
        QuickCommandSortMode::Custom => custom_label,
    }
}
