use std::collections::HashMap;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, Focusable, IntoElement, ListSizingBehavior,
    MouseButton, ParentElement as _, Render, ScrollStrategy, SharedString, Styled as _,
    Subscription, UniformListScrollHandle, WeakEntity, Window, div, prelude::*, px, rgb,
    uniform_list,
};
use nyaterm_core::{NoteFolder, NoteNodeKind};
use nyaterm_ui::{
    NyaButton, NyaButtonVariant, NyaContextMenu, NyaDropdownMenu, NyaInputEvent, NyaInputShell,
    NyaInputState, NyaMenuItem, NyaScrollable as _, NyaSearchInput,
};
use rust_i18n::t;

use crate::features::{NyaTermApp, view_widgets::mono_icon};

use super::NoteTreeRow;

#[derive(Clone)]
struct NotesDragPayload {
    id: String,
    label: String,
}

struct NotesDragPreview {
    payload: NotesDragPayload,
    position: gpui::Point<gpui::Pixels>,
}

impl Render for NotesDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .pl(self.position.x - px(70.))
            .pt(self.position.y - px(16.))
            .child(
                div()
                    .w(px(140.))
                    .h(px(32.))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded_sm()
                    .bg(rgb(0x1f2937))
                    .text_color(rgb(0xf1f5f9))
                    .child(self.payload.label.clone()),
            )
    }
}

pub(in crate::features) struct NotesPanel {
    app: WeakEntity<NyaTermApp>,
    search: Entity<NyaInputState>,
    rename: Entity<NyaInputState>,
    renaming_id: Option<String>,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    _app_subscription: Subscription,
    _search_subscription: Subscription,
    _rename_subscription: Subscription,
}

impl NotesPanel {
    pub(in crate::features) fn new(app: WeakEntity<NyaTermApp>, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| {
            NyaInputState::new(cx, String::new()).placeholder(t!("notes.searchPlaceholder"))
        });
        let rename = cx.new(|cx| NyaInputState::new(cx, String::new()).max_chars(Some(120)));
        let app_subscription = cx.observe(
            &app.upgrade().expect("Notes panel is created with its app"),
            |_, _, cx| cx.notify(),
        );
        let search_subscription = cx.subscribe(&search, |_, _, _: &NyaInputEvent, cx| cx.notify());
        let rename_subscription = cx.subscribe(&rename, |this, _, event: &NyaInputEvent, cx| {
            if let NyaInputEvent::Submitted(value) | NyaInputEvent::Blurred(value) = event {
                let value = value.clone();
                let Some(id) = this.renaming_id.take() else {
                    return;
                };
                this.with_app(cx, |app, cx| app.rename_note_node(id, value, cx));
                cx.notify();
            }
        });
        Self {
            app,
            search,
            rename,
            renaming_id: None,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            _app_subscription: app_subscription,
            _search_subscription: search_subscription,
            _rename_subscription: rename_subscription,
        }
    }

    fn with_app(
        &self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut NyaTermApp, &mut Context<NyaTermApp>),
    ) {
        if let Some(app) = self.app.upgrade() {
            app.update(cx, f);
        }
    }

    fn move_selection(&self, delta: isize, cx: &mut Context<Self>) {
        let search = self.search.read(cx).value(cx);
        let scroll = self.scroll.clone();
        self.with_app(cx, move |app, cx| {
            let rows = app.notes.visible_rows(&search);
            if rows.is_empty() {
                return;
            }
            let current = app
                .notes
                .selected_node_id()
                .and_then(|id| rows.iter().position(|row| row.id == id));
            let next = match current {
                Some(index) => (index as isize + delta).clamp(0, rows.len() as isize - 1) as usize,
                None if delta < 0 => rows.len() - 1,
                None => 0,
            };
            app.select_note_node(Some(rows[next].id.clone()), cx);
            scroll.scroll_to_item(next, ScrollStrategy::Nearest);
        });
    }

    fn activate_selected(&self, cx: &mut Context<Self>) {
        self.with_app(cx, |app, cx| {
            let Some(id) = app.notes.selected_node_id().map(str::to_string) else {
                return;
            };
            match app.notes.node(&id).map(|node| node.0) {
                Some(NoteNodeKind::Folder) => app.toggle_note_folder(&id, cx),
                Some(NoteNodeKind::Note) => app.open_note_editor(id, cx),
                None => {}
            }
        });
    }

    fn begin_rename(
        &mut self,
        id: String,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.renaming_id = Some(id);
        self.rename.update(cx, |field, cx| {
            field.set_content(&name, cx);
            field.select_all(window, cx);
        });
        cx.notify();
    }

    fn rename_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.app.upgrade().and_then(|app| {
            app.read_with(cx, |app, _| {
                let id = app.notes.selected_node_id()?.to_string();
                let (_, _, name) = app.notes.node(&id)?;
                Some((id, name))
            })
        });
        if let Some((id, name)) = selected {
            self.begin_rename(id, name, window, cx);
        }
    }
}

impl Focusable for NotesPanel {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for NotesPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(app) = self.app.upgrade() else {
            return div().size_full().into_any_element();
        };
        let search = self.search.read(cx).value(cx);
        let (palette, rows, move_targets, loading, error, selected, catalog_empty) =
            app.read_with(cx, |app, _| {
                let rows = app.notes.visible_rows(&search);
                let move_targets = rows
                    .iter()
                    .map(|row| {
                        let folders = app
                            .notes
                            .folders()
                            .iter()
                            .filter(|folder| app.notes.can_move_to(&row.id, Some(&folder.id)))
                            .cloned()
                            .collect::<Vec<_>>();
                        (row.id.clone(), folders)
                    })
                    .collect::<HashMap<_, _>>();
                (
                    app.theme_palette(),
                    rows,
                    move_targets,
                    app.notes.loading(),
                    app.notes.error().map(str::to_string),
                    app.notes.selected_node_id().map(str::to_string),
                    app.notes.folders().is_empty() && app.notes.notes().is_empty(),
                )
            });
        let row_count = rows.len();
        let app_for_rows = self.app.clone();
        let selected_for_rows = selected.clone();
        let list = uniform_list(
            "notes-tree-rows",
            row_count,
            cx.processor(move |panel, range: std::ops::Range<usize>, _, cx| {
                range
                    .filter_map(|index| rows.get(index).cloned())
                    .map(|row| {
                        let folders = move_targets.get(&row.id).cloned().unwrap_or_default();
                        note_row(
                            panel,
                            row,
                            folders,
                            selected_for_rows.as_deref(),
                            palette,
                            app_for_rows.clone(),
                            cx,
                        )
                    })
                    .collect()
            }),
        )
        .flex_grow(1.)
        .with_sizing_behavior(ListSizingBehavior::Auto)
        .track_scroll(&self.scroll);

        let search_state = self.search.clone();
        let mut search_input = NyaSearchInput::new("notes-search", &search_state);
        if !search.is_empty() {
            let clear = self.search.clone();
            search_input = search_input.trailing(
                div()
                    .id("notes-clear-search")
                    .px_1()
                    .cursor_pointer()
                    .on_click(move |_, _, cx| clear.update(cx, |field, cx| field.clear(cx)))
                    .child("×"),
            );
        }
        let mut root = div()
            .id("notes-panel")
            .key_context("NotesPanel")
            .track_focus(&self.focus)
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(palette.bg))
            .on_key_down(
                cx.listener(|panel, event: &gpui::KeyDownEvent, window, cx| {
                    if panel.search.read(cx).has_focus() || panel.rename.read(cx).has_focus() {
                        return;
                    }
                    match event.keystroke.key.as_str() {
                        "up" => panel.move_selection(-1, cx),
                        "down" => panel.move_selection(1, cx),
                        "enter" => panel.activate_selected(cx),
                        "f2" => panel.rename_selected(window, cx),
                        "delete" => panel.with_app(cx, |app, cx| {
                            app.request_delete_selected_note_node(window, cx)
                        }),
                        _ => (),
                    }
                }),
            )
            .child(
                div()
                    .px_2()
                    .py(px(6.))
                    .border_b_1()
                    .border_color(rgb(palette.border))
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(div().min_w_0().flex_1().h(px(28.)).child(search_input))
                    .child(toolbar_button(
                        "notes-new-folder",
                        "icons/fe/new-folder.svg",
                        t!("notes.newFolder"),
                        palette,
                        self.app.clone(),
                        |app, cx| app.create_note_folder_in_selected_folder(cx),
                    ))
                    .child(toolbar_button(
                        "notes-new-note",
                        "icons/conn/add.svg",
                        t!("notes.newNote"),
                        palette,
                        self.app.clone(),
                        |app, cx| app.create_note_in_selected_folder(cx),
                    ))
                    .child(notes_more_menu(self.app.clone())),
            );
        if loading && catalog_empty {
            root = root.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(palette.text_muted))
                    .child(t!("common.loading").to_string()),
            );
        } else if let Some(error) = error {
            root = root.child(
                div()
                    .p_3()
                    .text_size(px(11.))
                    .text_color(rgb(palette.danger))
                    .child(error),
            );
        } else if catalog_empty {
            let new_note_app = self.app.clone();
            let new_folder_app = self.app.clone();
            root = root.child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .px_4()
                    .text_center()
                    .text_color(rgb(palette.text_muted))
                    .child(mono_icon(
                        "icons/file/description.svg",
                        rgb(palette.text_dimmed).into(),
                        24.,
                    ))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(palette.text))
                            .child(t!("notes.emptyTitle").to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .child(t!("notes.emptyDescription").to_string()),
                    )
                    .child(
                        div()
                            .pt_1()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                NyaButton::new(
                                    "notes-empty-new-note",
                                    t!("notes.newNote").to_string(),
                                )
                                .small()
                                .variant(NyaButtonVariant::Primary)
                                .on_click(move |_, _, cx| {
                                    if let Some(app) = new_note_app.upgrade() {
                                        app.update(cx, |app, cx| app.create_root_note(cx));
                                    }
                                }),
                            )
                            .child(
                                NyaButton::new(
                                    "notes-empty-new-folder",
                                    t!("notes.newFolder").to_string(),
                                )
                                .small()
                                .on_click(move |_, _, cx| {
                                    if let Some(app) = new_folder_app.upgrade() {
                                        app.update(cx, |app, cx| app.create_root_note_folder(cx));
                                    }
                                }),
                            ),
                    ),
            );
        } else {
            root = root.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .on_drop(cx.listener(|panel, payload: &NotesDragPayload, _, cx| {
                        cx.stop_propagation();
                        let id = payload.id.clone();
                        panel.with_app(cx, |app, cx| app.move_note_node_to(id, None, cx));
                    }))
                    .child(list)
                    .vertical_scrollbar(&self.scroll),
            );
        }
        if !self.focus.is_focused(window)
            && !self.search.read(cx).has_focus()
            && !self.rename.read(cx).has_focus()
        {
            window.focus(&self.focus, cx);
        }
        root.into_any_element()
    }
}

fn notes_more_menu(app: WeakEntity<NyaTermApp>) -> NyaDropdownMenu {
    let expand_app = app.clone();
    let collapse_app = app.clone();
    NyaDropdownMenu::new("notes-more")
        .icon("icons/session/more.svg")
        .icon_size(px(16.))
        .tooltip(t!("common.more"))
        .items([
            NyaMenuItem::action(t!("notes.expandAll").to_string()).on_click(move |_, _, cx| {
                if let Some(app) = expand_app.upgrade() {
                    app.update(cx, |app, cx| app.set_all_notes_expanded(true, cx));
                }
            }),
            NyaMenuItem::action(t!("notes.collapseAll").to_string()).on_click(move |_, _, cx| {
                if let Some(app) = collapse_app.upgrade() {
                    app.update(cx, |app, cx| app.set_all_notes_expanded(false, cx));
                }
            }),
            NyaMenuItem::action(t!("common.refresh").to_string())
                .icon("icons/fe/refresh.svg")
                .on_click(move |_, _, cx| {
                    if let Some(app) = app.upgrade() {
                        app.update(cx, |app, cx| app.refresh_notes(cx));
                    }
                }),
        ])
}

fn toolbar_button(
    id: &'static str,
    icon: &'static str,
    tooltip: impl Into<SharedString>,
    palette: crate::theme::ThemePalette,
    app: WeakEntity<NyaTermApp>,
    action: impl Fn(&mut NyaTermApp, &mut Context<NyaTermApp>) + 'static,
) -> gpui::AnyElement {
    let tooltip = tooltip.into();
    div()
        .id(id)
        .size(px(26.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .cursor_pointer()
        .hover(move |this| this.bg(rgb(palette.hover)))
        .tooltip(move |window, cx| nyaterm_ui::NyaTooltip::new(tooltip.clone()).build(window, cx))
        .on_click(move |_, _, cx| {
            if let Some(app) = app.upgrade() {
                app.update(cx, |app, cx| action(app, cx));
            }
        })
        .child(mono_icon(icon, rgb(palette.text_muted).into(), 14.))
        .into_any_element()
}

fn note_row(
    panel: &mut NotesPanel,
    row: NoteTreeRow,
    move_folders: Vec<NoteFolder>,
    selected_id: Option<&str>,
    palette: crate::theme::ThemePalette,
    app: WeakEntity<NyaTermApp>,
    cx: &mut Context<NotesPanel>,
) -> gpui::AnyElement {
    let selected = selected_id == Some(row.id.as_str());
    let row_id = row.id.clone();
    let click_id = row.id.clone();
    let kind = row.kind;
    let row_name = row.name.clone();
    let app_for_click = app.clone();
    let app_for_context = app.clone();
    let drag_payload = NotesDragPayload {
        id: row.id.clone(),
        label: row.name.clone(),
    };
    let drop_parent_id = match row.kind {
        NoteNodeKind::Folder => Some(row.id.clone()),
        NoteNodeKind::Note => row.parent_id.clone(),
    };
    if panel.renaming_id.as_deref() == Some(row.id.as_str()) {
        return div()
            .h(px(30.))
            .w_full()
            .flex_none()
            .pl(px(34. + row.depth as f32 * 16.))
            .pr_2()
            .flex()
            .items_center()
            .child(
                div()
                    .w_full()
                    .h(px(28.))
                    .child(NyaInputShell::new("notes-inline-rename", &panel.rename)),
            )
            .into_any_element();
    }
    let base = div()
        .id(SharedString::from(format!("notes-row-{}", row.id)))
        .h(px(30.))
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .gap_1()
        .pr_2()
        .pl(px(6. + row.depth as f32 * 16.))
        .bg(if selected {
            rgb(palette.surface_elevated)
        } else {
            rgb(palette.bg)
        })
        .hover(move |this| this.bg(rgb(palette.hover)))
        .cursor_pointer()
        .cursor_move()
        .on_drag(drag_payload, |payload, position, _, cx| {
            cx.new(|_| NotesDragPreview {
                payload: payload.clone(),
                position,
            })
        })
        .on_drop(
            cx.listener(move |panel, payload: &NotesDragPayload, _, cx| {
                cx.stop_propagation();
                let id = payload.id.clone();
                let parent_id = drop_parent_id.clone();
                panel.with_app(cx, |app, cx| app.move_note_node_to(id, parent_id, cx));
            }),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _, _, cx| cx.stop_propagation()),
        )
        .on_click(move |event, _, cx| {
            if let Some(app) = app_for_click.upgrade() {
                app.update(cx, |app, cx| {
                    app.select_note_node(Some(click_id.clone()), cx);
                    if event.click_count() >= 2 {
                        match kind {
                            NoteNodeKind::Folder => app.toggle_note_folder(&click_id, cx),
                            NoteNodeKind::Note => app.open_note_editor(click_id.clone(), cx),
                        }
                    }
                });
            }
        })
        .child(
            div()
                .w(px(14.))
                .flex_none()
                .text_color(rgb(palette.text_dimmed))
                .child(if row.kind == NoteNodeKind::Folder && row.has_children {
                    if row.expanded { "⌄" } else { "›" }
                } else {
                    ""
                }),
        )
        .child(mono_icon(
            if row.kind == NoteNodeKind::Folder {
                "icons/conn/folder.svg"
            } else {
                "icons/notes.svg"
            },
            rgb(if selected {
                palette.link
            } else {
                palette.text_muted
            })
            .into(),
            14.,
        ))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_size(px(12.))
                .text_color(rgb(palette.text))
                .child(row.name),
        );

    let open_app = app.clone();
    let delete_app = app.clone();
    let refresh_app = app;
    let open_id = row_id.clone();
    let delete_id = row_id;
    let rename_panel = cx.weak_entity();
    let rename_id = delete_id.clone();
    let rename_name = row_name;
    let mut move_items = Vec::with_capacity(move_folders.len() + 1);
    let root_app = app_for_context.clone();
    let root_id = delete_id.clone();
    move_items.push(
        NyaMenuItem::action(t!("notes.root").to_string()).on_click(move |_, _, cx| {
            if let Some(app) = root_app.upgrade() {
                let id = root_id.clone();
                app.update(cx, |app, cx| app.move_note_node_to(id, None, cx));
            }
        }),
    );
    for folder in move_folders {
        let move_app = app_for_context.clone();
        let id = delete_id.clone();
        let parent_id = folder.id.clone();
        move_items.push(NyaMenuItem::action(folder.name).on_click(move |_, _, cx| {
            if let Some(app) = move_app.upgrade() {
                let id = id.clone();
                let parent_id = parent_id.clone();
                app.update(cx, |app, cx| app.move_note_node_to(id, Some(parent_id), cx));
            }
        }));
    }
    let create_note_app = app_for_context.clone();
    let create_note_id = delete_id.clone();
    let create_folder_app = app_for_context;
    let create_folder_id = delete_id.clone();
    let items = vec![
        NyaMenuItem::action(t!("notes.open").to_string()).on_click(move |_, _, cx| {
            if let Some(app) = open_app.upgrade() {
                let id = open_id.clone();
                app.update(cx, |app, cx| match kind {
                    NoteNodeKind::Folder => app.toggle_note_folder(&id, cx),
                    NoteNodeKind::Note => app.open_note_editor(id, cx),
                });
            }
        }),
        NyaMenuItem::action(t!("notes.newNote").to_string()).on_click(move |_, _, cx| {
            if let Some(app) = create_note_app.upgrade() {
                let id = create_note_id.clone();
                app.update(cx, |app, cx| {
                    app.select_note_node(Some(id), cx);
                    app.create_note_in_selected_folder(cx);
                });
            }
        }),
        NyaMenuItem::action(t!("notes.newFolder").to_string()).on_click(move |_, _, cx| {
            if let Some(app) = create_folder_app.upgrade() {
                let id = create_folder_id.clone();
                app.update(cx, |app, cx| {
                    app.select_note_node(Some(id), cx);
                    app.create_note_folder_in_selected_folder(cx);
                });
            }
        }),
        NyaMenuItem::action(t!("notes.rename").to_string()).on_click(move |_, window, cx| {
            let _ = rename_panel.update(cx, |panel, cx| {
                panel.begin_rename(rename_id.clone(), rename_name.clone(), window, cx)
            });
        }),
        NyaMenuItem::submenu(t!("notes.moveTo").to_string(), move_items),
        NyaMenuItem::action(t!("common.delete").to_string())
            .danger()
            .on_click(move |_, window, cx| {
                if let Some(app) = delete_app.upgrade() {
                    let id = delete_id.clone();
                    app.update(cx, |app, cx| {
                        app.select_note_node(Some(id), cx);
                        app.request_delete_selected_note_node(window, cx);
                    });
                }
            }),
        NyaMenuItem::action(t!("common.refresh").to_string()).on_click(move |_, _, cx| {
            if let Some(app) = refresh_app.upgrade() {
                app.update(cx, |app, cx| app.refresh_notes(cx));
            }
        }),
    ];
    NyaContextMenu::new(base, items).into_any_element()
}
