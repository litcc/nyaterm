use std::cmp::Ordering;

use gpui::{
    ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, ParentElement as _, Pixels, Rgba, SharedString,
    StatefulInteractiveElement as _, Styled as _, div, prelude::FluentBuilder as _, px, rgb, rgba,
};
use nyaterm_transport::SftpFileEntry;

use crate::features::NyaTermApp;
use crate::models::{TransferBrowserSortColumn, TransferBrowserSortDirection};
use crate::theme::ThemePalette;

/// Height of the sort-header row.
///
/// The browser's vertical scrollbar overlay starts below the header, so the two
/// must agree or the bar's track is offset from the rows it scrolls.
pub(in crate::features::pages::transfers) const FILE_BROWSER_HEADER_HEIGHT_PX: f32 = 28.;

pub(in crate::features::pages::transfers) fn transfer_browser_search_status(
    query: &str,
    visible: usize,
    total: usize,
) -> String {
    if query.trim().is_empty() {
        format!("{total} item(s)")
    } else {
        format!("{visible} of {total} item(s) match search")
    }
}

pub(in crate::features::pages::transfers) fn sort_header_cell(
    palette: ThemePalette,
    column: TransferBrowserSortColumn,
    localized_label: &'static str,
    width: Pixels,
    state: TransferBrowserSortHeaderState,
    cx: &mut Context<NyaTermApp>,
) -> impl IntoElement {
    let is_active = column == state.active_column;
    let is_resizing = state.resizing_column == Some(column);
    let direction_icon = match state.direction {
        TransferBrowserSortDirection::Ascending => "icons/fe/sort-ascending.svg",
        TransferBrowserSortDirection::Descending => "icons/fe/sort-descending.svg",
    };

    div()
        .id(SharedString::from(format!(
            "transfer-browser-sort-{}",
            column.label().to_lowercase()
        )))
        .h(px(FILE_BROWSER_HEADER_HEIGHT_PX))
        .w(width)
        .flex_none()
        .relative()
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .border_r_1()
        .border_color(rgb(palette.surface_elevated))
        .cursor_pointer()
        .bg(if is_active {
            rgba((palette.primary << 8) | 0x14)
        } else {
            state.header_bg
        })
        .text_size(px(10.))
        .font_weight(FontWeight(800.))
        .text_color(if is_active {
            rgb(palette.link)
        } else {
            rgb(palette.text_muted)
        })
        .hover(|this| this.bg(rgb(palette.hover)).text_color(rgb(palette.text)))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.toggle_transfer_browser_sort(column, cx);
        }))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(localized_label.to_uppercase()),
        )
        .when(is_active, |this| {
            this.child(
                gpui::svg()
                    .size(px(14.))
                    .flex_none()
                    .path(direction_icon)
                    .text_color(rgb(palette.link)),
            )
        })
        .child(
            div()
                .id(SharedString::from(format!(
                    "transfer-browser-resize-{}",
                    column.label().to_lowercase()
                )))
                .absolute()
                .right(px(-3.))
                .top(px(4.))
                .bottom(px(4.))
                .w(px(7.))
                .rounded_sm()
                .cursor_col_resize()
                .bg(if is_resizing {
                    rgb(palette.success)
                } else {
                    rgb(0x1b2433)
                })
                .hover(|this| this.bg(rgb(palette.success)))
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.start_transfer_browser_column_resize(column, event, cx);
                    }),
                ),
        )
}

#[derive(Clone, Copy)]
pub(in crate::features::pages::transfers) struct TransferBrowserSortHeaderState {
    pub header_bg: Rgba,
    pub active_column: TransferBrowserSortColumn,
    pub direction: TransferBrowserSortDirection,
    pub resizing_column: Option<TransferBrowserSortColumn>,
}

pub(in crate::features::pages::transfers) fn compare_transfer_browser_entries(
    left: &SftpFileEntry,
    right: &SftpFileEntry,
    column: TransferBrowserSortColumn,
    direction: TransferBrowserSortDirection,
) -> Ordering {
    if left.file_type != right.file_type {
        let left_dir = left.is_directory();
        let right_dir = right.is_directory();
        if left_dir != right_dir {
            return if left_dir {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }
    }

    let result = match column {
        TransferBrowserSortColumn::Name => natural_compare_ascii(&left.name, &right.name),
        TransferBrowserSortColumn::Size => left.size.unwrap_or(0).cmp(&right.size.unwrap_or(0)),
        TransferBrowserSortColumn::Modified => left
            .modified_at
            .unwrap_or(0)
            .cmp(&right.modified_at.unwrap_or(0)),
        TransferBrowserSortColumn::Permissions => left
            .permissions
            .unwrap_or(0)
            .cmp(&right.permissions.unwrap_or(0)),
        TransferBrowserSortColumn::Owner => natural_compare_ascii(&left.owner, &right.owner),
        TransferBrowserSortColumn::Group => natural_compare_ascii(&left.group, &right.group),
    };

    let directed = match direction {
        TransferBrowserSortDirection::Ascending => result,
        TransferBrowserSortDirection::Descending => result.reverse(),
    };
    directed.then_with(|| natural_compare_ascii(&left.name, &right.name))
}

pub(in crate::features::pages::transfers) fn natural_compare_ascii(
    left: &str,
    right: &str,
) -> Ordering {
    let left = left.to_lowercase();
    let right = right.to_lowercase();
    let mut left_chars = left.chars().peekable();
    let mut right_chars = right.chars().peekable();

    loop {
        match (left_chars.peek().copied(), right_chars.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(left_char), Some(right_char))
                if left_char.is_ascii_digit() && right_char.is_ascii_digit() =>
            {
                let mut left_number = String::new();
                while let Some(value) = left_chars.peek().copied() {
                    if value.is_ascii_digit() {
                        left_number.push(value);
                        left_chars.next();
                    } else {
                        break;
                    }
                }
                let mut right_number = String::new();
                while let Some(value) = right_chars.peek().copied() {
                    if value.is_ascii_digit() {
                        right_number.push(value);
                        right_chars.next();
                    } else {
                        break;
                    }
                }
                let left_trimmed = left_number.trim_start_matches('0');
                let right_trimmed = right_number.trim_start_matches('0');
                let left_key = if left_trimmed.is_empty() {
                    "0"
                } else {
                    left_trimmed
                };
                let right_key = if right_trimmed.is_empty() {
                    "0"
                } else {
                    right_trimmed
                };
                let ordering = left_key
                    .len()
                    .cmp(&right_key.len())
                    .then_with(|| left_key.cmp(right_key))
                    .then_with(|| left_number.len().cmp(&right_number.len()));
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (Some(left_char), Some(right_char)) => {
                left_chars.next();
                right_chars.next();
                let ordering = left_char.cmp(&right_char);
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
        }
    }
}
