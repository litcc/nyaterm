use rust_i18n::t;

use gpui::{
    App, ClickEvent, Context, FontWeight, IntoElement, SharedString, Window, div, prelude::*, px,
    rgb, rgba, svg,
};
use nyaterm_ui::{NyaNumberInputOptions, NyaSelectOption};

use crate::features::{NyaTermApp, selects::FOLLOW_UI_THEME_VALUE, shell::appearance_font_stack};
use crate::theme::{APPEARANCE_THEME_IDS, ThemePalette, appearance_theme_label};
use nyaterm_ui::NyaTooltip;

use super::super::{settings_form_row, settings_form_section, settings_switch};

impl NyaTermApp {
    pub(in crate::features) fn appearance_settings_section(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(settings_form_section(
                palette,
                None,
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap_5()
                    .child(appearance_settings_field(
                        palette,
                        t!("settings.theme"),
                        Some(SharedString::from(t!("settings.themeDesc"))),
                        self.appearance_theme_select(false, cx),
                    ))
                    .child(appearance_settings_field(
                        palette,
                        t!("settings.terminalTheme"),
                        Some(SharedString::from(t!("settings.terminalThemeDesc"))),
                        self.appearance_theme_select(true, cx),
                    ))
                    .child(appearance_settings_field(
                        palette,
                        t!("settings.minimumContrastRatio"),
                        Some(SharedString::from(
                            t!("settings.minimumContrastRatioDesc"),
                        )),
                        self.appearance_contrast_select(cx),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("settings.panelMultiOpen"),
                        Some(SharedString::from(t!("settings.panelMultiOpenDesc"))),
                        settings_switch(
                            palette,
                            "appearance-panel-multi-open",
                            self.shell.panel_multi_open(),
                            cx.listener(|this, _, _, cx| {
                                this.toggle_panel_multi_open(cx);
                            }),
                        ),
                    )),
            ))
            .child(settings_form_section(
                palette,
                Some(t!("settings.backgroundImage")),
                Some(t!("settings.backgroundImageDesc")),
                {
                    let path_label = self
                        .settings
                        .summary()
                        .background_image_path
                        .as_deref()
                        .map(|p| {
                            if p.chars().count() > 56 {
                                format!(
                                    "…{}",
                                    p.chars()
                                        .rev()
                                        .take(52)
                                        .collect::<String>()
                                        .chars()
                                        .rev()
                                        .collect::<String>()
                                )
                            } else {
                                p.to_string()
                            }
                        })
                        .unwrap_or_else(|| t!("settings.backgroundImageEmpty").to_string());
                    let has_image = self.settings.summary().background_image_path.is_some();
                    div()
                        .flex()
                        .flex_col()
                        .gap_5()
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .min_h(px(36.))
                                        .min_w_0()
                                        .flex_1()
                                        .px_3()
                                        .py_2()
                                        .rounded_sm()
                                        .border_1()
                                        .border_color(rgb(palette.border))
                                        .bg(rgb(palette.input))
                                        .font_family(crate::features::shell::gpui_code_font_family())
                                        .text_size(px(11.))
                                        .text_color(rgb(if has_image {
                                            palette.text
                                        } else {
                                            palette.text_muted
                                        }))
                                        .child(path_label),
                                )
                                .child(appearance_icon_text_button(
                                    palette,
                                    "appearance-wallpaper-browse",
                                    "icons/conn/folder.svg",
                                    t!("settings.selectBackgroundImage"),
                                    cx.listener(|this, _, _, cx| {
                                        this.prompt_background_image(cx);
                                    }),
                                ))
                                .when(has_image, |this| {
                                    this.child(appearance_icon_text_button(
                                        palette,
                                        "appearance-wallpaper-clear",
                                        "icons/fe/delete.svg",
                                        t!("settings.removeBackgroundImage"),
                                        cx.listener(|this, _, _, cx| {
                                            this.clear_background_image(cx);
                                        }),
                                    ))
                                }),
                        )
                        .child(appearance_settings_field(
                            palette,
                            t!("settings.backgroundImageFit"),
                            Some(SharedString::from(
                                t!("settings.backgroundImageFitDesc"),
                            )),
                            self.appearance_background_fit_select(has_image, cx),
                        ))
                        .child(self.appearance_opacity_slider(false, has_image, cx))
                        .child(self.appearance_opacity_slider(true, has_image, cx))
                },
            ))
            .child(self.appearance_font_stack_settings_section(false, cx))
            .child(self.appearance_font_stack_settings_section(true, cx))
            .child(settings_form_section(
                palette,
                None,
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap_5()
                    .child(appearance_settings_field(
                        palette,
                        t!("settings.cursorStyle"),
                        None,
                        self.appearance_cursor_style_select(cx),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("settings.cursorBlink"),
                        None,
                        settings_switch(
                            palette,
                            "appearance-cursor-blink",
                            self.settings.summary().cursor_blink,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_cursor_blink(cx);
                            }),
                        ),
                    )),
            ))
    }

    fn appearance_font_stack_settings_section(
        &mut self,
        terminal: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let (title, desc, raw, fallback, mut options) = if terminal {
            (
                t!("settings.terminalFontFamily"),
                t!("settings.terminalFontFamilyDesc"),
                self.settings.summary().terminal_font_family.clone(),
                "JetBrains Mono",
                self.settings.terminal_font_options().to_vec(),
            )
        } else {
            (
                t!("settings.uiFontFamily"),
                t!("settings.uiFontFamilyDesc"),
                self.settings.summary().ui_font_family.clone(),
                "Inter",
                self.settings.ui_font_options().to_vec(),
            )
        };
        let fonts = appearance_font_stack(&raw, fallback);
        for family in &fonts {
            if !options
                .iter()
                .any(|option| option.eq_ignore_ascii_case(family))
            {
                options.insert(0, family.clone());
            }
        }
        let kind = if terminal { "terminal" } else { "ui" };
        let add_id = if terminal {
            "appearance-terminal-font-add"
        } else {
            "appearance-ui-font-add"
        };
        let add_handler: AppearanceClickHandler = Box::new(cx.listener(move |this, _, _, cx| {
            this.add_appearance_fallback_font(terminal, cx);
        }));
        let add_action = appearance_icon_text_button(
            palette,
            add_id,
            "icons/conn/add.svg",
            t!("settings.addFallbackFont"),
            add_handler,
        );
        let primary_label = t!("settings.fontPrimary");
        let fallback_label = t!("settings.fontFallback");
        let remove_label = t!("common.remove");

        let mut content =
            div()
                .flex()
                .flex_col()
                .gap_3()
                .children(fonts.into_iter().enumerate().map(|(index, family)| {
                    let menu_id = format!("appearance-{kind}-font-{index}");
                    let select_options = options
                        .iter()
                        .map(|option| {
                            NyaSelectOption::new(option.clone(), option.clone())
                                .font_family(option.clone())
                        })
                        .collect::<Vec<_>>();
                    let selected_family = family.clone();
                    let delete_id = format!("appearance-{kind}-font-delete-{index}");
                    let delete: AppearanceClickHandler =
                        Box::new(cx.listener(move |this, _, _, cx| {
                            this.remove_appearance_font_stack_entry(terminal, index, cx);
                        }));
                    let row_label = if index == 0 {
                        primary_label.to_string()
                    } else {
                        format!("{fallback_label} {index}")
                    };

                    div()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(palette.border))
                        .bg(rgb(palette.input))
                        .p_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap_3()
                                .child(
                                    div()
                                        .w(px(96.))
                                        .flex_none()
                                        .text_size(px(11.))
                                        .font_weight(FontWeight(500.))
                                        .text_color(rgb(palette.text_muted))
                                        .child(row_label),
                                )
                                .child(div().flex_1().min_w(px(220.)).child(self.select_control(
                                    menu_id,
                                    select_options,
                                    Some(selected_family),
                                    false,
                                    cx,
                                )))
                                .child(appearance_icon_button(
                                    palette,
                                    delete_id,
                                    "icons/fe/delete.svg",
                                    remove_label.clone(),
                                    delete,
                                )),
                        )
                }));

        if terminal {
            let font_size_label = self.settings.summary().terminal_font_size.to_string();
            content = content
                .child(appearance_settings_field(
                    palette,
                    t!("settings.fontSize"),
                    None,
                    self.number_input_box(
                        "appearance.number.terminal-font-size",
                        font_size_label.as_str(),
                        NyaNumberInputOptions::default().range(8.0, 72.0).step(1.0),
                        cx,
                    ),
                ))
                .child(appearance_settings_field(
                    palette,
                    t!("settings.terminalFontWeight"),
                    Some(SharedString::from(t!("settings.terminalFontWeightDesc"))),
                    self.appearance_font_weight_select(false, cx),
                ))
                .child(appearance_settings_field(
                    palette,
                    t!("settings.terminalFontWeightBold"),
                    Some(SharedString::from(t!(
                        "settings.terminalFontWeightBoldDesc"
                    ))),
                    self.appearance_font_weight_select(true, cx),
                ));
        } else {
            let font_size_label = self.settings.summary().ui_font_size.to_string();
            content = content.child(appearance_settings_field(
                palette,
                t!("settings.uiFontSize"),
                None,
                self.number_input_box(
                    "appearance.number.ui-font-size",
                    font_size_label.as_str(),
                    NyaNumberInputOptions::default().range(12.0, 24.0).step(1.0),
                    cx,
                ),
            ));
        }

        appearance_form_section_with_action(palette, title, desc, add_action, content)
    }

    fn appearance_opacity_slider(
        &mut self,
        content: bool,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let value = if content {
            self.settings.summary().background_content_opacity
        } else {
            self.settings.summary().background_image_opacity
        };
        let label = if content {
            t!("settings.backgroundContentOpacity")
        } else {
            t!("settings.backgroundImageOpacity")
        };
        let desc = if content {
            t!("settings.backgroundContentOpacityDesc", value = "82%").to_string()
        } else {
            t!("settings.backgroundImageOpacityDesc").to_string()
        };
        let kind = if content { "content" } else { "image" };

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight(500.))
                                    .text_color(rgb(palette.text))
                                    .child(label),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_size(px(11.))
                                    .text_color(rgb(palette.text_dimmed))
                                    .child(desc),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(palette.border))
                            .bg(rgb(palette.input))
                            .px_2()
                            .py_1()
                            .font_family(crate::features::shell::gpui_code_font_family())
                            .text_size(px(11.))
                            .text_color(rgb(palette.text_muted))
                            .child(format!("{value}%")),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from(format!(
                        "appearance-{kind}-opacity-track"
                    )))
                    .h(px(10.))
                    .w_full()
                    .rounded_full()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.input))
                    .overflow_hidden()
                    .flex()
                    .opacity(if enabled { 1.0 } else { 0.5 })
                    .children((0_u8..=100).map(|percent| {
                        div()
                            .id(SharedString::from(format!(
                                "appearance-{kind}-opacity-{percent}"
                            )))
                            .h_full()
                            .flex_1()
                            .bg(if percent < value {
                                rgb(palette.primary)
                            } else {
                                rgb(palette.input)
                            })
                            .when(enabled, |this| {
                                this.cursor_pointer().on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        if content {
                                            this.set_background_content_opacity(percent, cx);
                                        } else {
                                            this.set_background_image_opacity(percent, cx);
                                        }
                                    },
                                ))
                            })
                    })),
            )
    }

    fn appearance_theme_select(
        &mut self,
        terminal: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = if terminal {
            "appearance-terminal-theme"
        } else {
            "appearance-ui-theme"
        };
        let current = if terminal {
            self.settings
                .summary()
                .terminal_theme
                .as_deref()
                .filter(|theme| !theme.trim().is_empty())
        } else {
            Some(self.settings.summary().theme.as_str())
        };
        let mut options = Vec::new();
        if terminal {
            options.push(NyaSelectOption::new(
                FOLLOW_UI_THEME_VALUE,
                t!("settings.followUiTheme"),
            ));
        }
        for theme_id in APPEARANCE_THEME_IDS {
            options.push(NyaSelectOption::new(
                *theme_id,
                appearance_theme_label(theme_id),
            ));
        }
        let selected = current
            .map(|theme| {
                if theme == "catppuccin" {
                    "catppuccin-mocha"
                } else {
                    theme
                }
            })
            .unwrap_or(FOLLOW_UI_THEME_VALUE)
            .to_string();
        self.select_control(id, options, Some(selected), false, cx)
    }

    fn appearance_contrast_select(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let id = "appearance-minimum-contrast";
        let current = match self.settings.summary().minimum_contrast_ratio.as_str() {
            "3" => "3",
            "4.5" => "4.5",
            "7" => "7",
            "21" => "21",
            _ => "1",
        };
        let label_for = |ratio: &str| match ratio {
            "3" => t!("settings.minimumContrastRatio_3"),
            "4.5" => t!("settings.minimumContrastRatio_4_5"),
            "7" => t!("settings.minimumContrastRatio_7"),
            "21" => t!("settings.minimumContrastRatio_21"),
            _ => t!("settings.minimumContrastRatio_1"),
        };
        let options = ["1", "3", "4.5", "7", "21"]
            .into_iter()
            .map(|ratio| NyaSelectOption::new(ratio, label_for(ratio)))
            .collect();
        self.select_control(id, options, Some(current.to_string()), false, cx)
    }

    fn appearance_background_fit_select(
        &mut self,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = "appearance-background-fit";
        let current = match self.settings.summary().background_image_fit.as_str() {
            "contain" => "contain",
            "stretch" | "fill" => "stretch",
            "tile" => "tile",
            _ => "cover",
        };
        let label_for = |fit: &str| match fit {
            "contain" => t!("settings.backgroundImageFit_contain"),
            "stretch" => t!("settings.backgroundImageFit_stretch"),
            "tile" => t!("settings.backgroundImageFit_tile"),
            _ => t!("settings.backgroundImageFit_cover"),
        };
        let options = ["cover", "contain", "stretch", "tile"]
            .into_iter()
            .map(|fit| NyaSelectOption::new(fit, label_for(fit)))
            .collect();
        self.select_control(id, options, Some(current.to_string()), !enabled, cx)
    }

    fn appearance_font_weight_select(
        &mut self,
        bold: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = if bold {
            "appearance-terminal-font-weight-bold"
        } else {
            "appearance-terminal-font-weight"
        };
        let current = if bold {
            self.settings.summary().terminal_font_weight_bold
        } else {
            self.settings.summary().terminal_font_weight
        };
        let label_for = |weight| match weight {
            300 => t!("settings.fontWeight_300"),
            500 => t!("settings.fontWeight_500"),
            600 => t!("settings.fontWeight_600"),
            700 => t!("settings.fontWeight_700"),
            800 => t!("settings.fontWeight_800"),
            _ => t!("settings.fontWeight_400"),
        };
        let options = [300_u16, 400, 500, 600, 700, 800]
            .into_iter()
            .map(|weight| NyaSelectOption::new(weight.to_string(), label_for(weight)))
            .collect();
        let selected = match current {
            300 => 300,
            500 => 500,
            600 => 600,
            700 => 700,
            800 => 800,
            _ => 400,
        };
        self.select_control(id, options, Some(selected.to_string()), false, cx)
    }

    fn appearance_cursor_style_select(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let id = "appearance-cursor-style";
        let current = match self.settings.summary().cursor_style.as_str() {
            "underline" => "underline",
            "bar" => "bar",
            _ => "block",
        };
        let label_for = |style: &str| match style {
            "underline" => t!("settings.cursorUnderline"),
            "bar" => t!("settings.cursorBar"),
            _ => t!("settings.cursorBlock"),
        };
        let options = ["block", "underline", "bar"]
            .into_iter()
            .map(|style| NyaSelectOption::new(style, label_for(style)))
            .collect();
        self.select_control(id, options, Some(current.to_string()), false, cx)
    }
}

type AppearanceClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

fn appearance_form_section_with_action(
    palette: ThemePalette,
    title: impl Into<SharedString>,
    desc: impl Into<SharedString>,
    action: impl IntoElement,
    content: impl IntoElement,
) -> impl IntoElement {
    let title: SharedString = title.into();
    let desc: SharedString = desc.into();
    div()
        .rounded_lg()
        .border_1()
        .border_color(rgba((palette.border << 8) | 0xb3))
        .bg(rgba((palette.surface << 8) | 0x99))
        .overflow_hidden()
        .child(
            div()
                .px_4()
                .py_4()
                .border_b_1()
                .border_color(rgba((palette.surface_elevated << 8) | 0x99))
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight(600.))
                                .text_color(rgb(palette.text))
                                .child(title),
                        )
                        .child(
                            div()
                                .mt_1()
                                .text_size(px(12.))
                                .text_color(rgb(palette.text_dimmed))
                                .child(desc),
                        ),
                )
                .child(action),
        )
        .child(div().px_4().py_4().child(content))
}

fn appearance_settings_field(
    palette: ThemePalette,
    label: impl Into<SharedString>,
    desc: Option<SharedString>,
    control: impl IntoElement,
) -> impl IntoElement {
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
        .child(div().w_full().max_w(px(576.)).child(control))
}

fn appearance_icon_text_button(
    palette: ThemePalette,
    id: &'static str,
    icon_path: &'static str,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label: SharedString = label.into();
    let destructive = icon_path == "icons/fe/delete.svg";
    let hover = if destructive {
        rgba((palette.danger << 8) | 0x18)
    } else {
        rgb(palette.hover)
    };
    div()
        .id(id)
        .h(px(28.))
        .px_2()
        .rounded_sm()
        .flex()
        .items_center()
        .gap_1()
        .text_size(px(11.))
        .text_color(rgb(if destructive {
            palette.danger
        } else {
            palette.primary
        }))
        .cursor_pointer()
        .hover(move |this| this.bg(hover))
        .child(
            svg()
                .size(px(14.))
                .path(icon_path)
                .text_color(rgb(if destructive {
                    palette.danger
                } else {
                    palette.primary
                })),
        )
        .child(label)
        .on_click(on_click)
}

fn appearance_icon_button(
    palette: ThemePalette,
    id: String,
    icon_path: &'static str,
    tooltip: impl Into<SharedString>,
    on_click: AppearanceClickHandler,
) -> impl IntoElement {
    let tooltip: SharedString = tooltip.into();
    let hover = rgba((palette.danger << 8) | 0x18);
    div()
        .id(SharedString::from(id))
        .size(px(28.))
        .flex_none()
        .rounded_sm()
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(palette.danger))
        .cursor_pointer()
        .hover(move |this| this.bg(hover))
        .child(
            svg()
                .size(px(15.))
                .path(icon_path)
                .text_color(rgb(palette.danger)),
        )
        .tooltip(move |window, cx| NyaTooltip::new(tooltip.clone()).build(window, cx))
        .on_click(on_click)
}
