use rust_i18n::t;

use std::collections::HashMap;

use gpui::{Context, KeyDownEvent, Window};
use nyaterm_core::fuzzy_search_items;

use crate::entities::{OverlayStore, QuickSwitchState};
use crate::features::{
    NyaTermApp, formatting::session_kind_label, formatting::short_id, text_inputs::TextInputSetup,
};
use crate::models::QuickSwitchItem;

impl NyaTermApp {
    pub(in crate::features) fn quick_switch_state(
        &self,
        cx: &mut Context<Self>,
    ) -> QuickSwitchState {
        self.stores
            .overlays
            .read_with(cx, |store, _| store.quick_switch().clone())
    }

    pub(in crate::features) fn quick_switch_open(&self, cx: &mut Context<Self>) -> bool {
        self.quick_switch_state(cx).is_open()
    }

    pub(in crate::features) fn update_quick_switch_state(
        &mut self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut OverlayStore) -> bool,
    ) -> bool {
        let changed = self.stores.overlays.update(cx, |store, cx| {
            let changed = update(store);
            if changed {
                cx.notify();
            }
            changed
        });
        if changed {
            cx.notify();
        }
        changed
    }

    pub(in crate::features) fn open_quick_switch(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_quick_switch_state(cx, |store| store.open_quick_switch());
        self.forget_text_inputs("quick-switch.query");
        let field = self.text_input(
            "quick-switch.query",
            "",
            TextInputSetup::placeholder(t!("sessionQuickSwitcher.searchPlaceholder")),
            cx,
        );
        self.shell.set_status("quick switch opened".to_string());
        window.focus(&field.read(cx).focus_handle(), cx);
        cx.notify();
    }

    pub(in crate::features) fn close_quick_switch(&mut self, cx: &mut Context<Self>) {
        self.update_quick_switch_state(cx, |store| store.close_quick_switch());
        self.forget_text_inputs("quick-switch.query");
        self.shell.set_status("quick switch closed".to_string());
        cx.notify();
    }

    pub(in crate::features) fn quick_switch_items(&self) -> Vec<QuickSwitchItem> {
        let sessions = self.session.ordered_sessions();
        let mut session_items = Vec::new();
        for session in &sessions {
            let title = self.session.display_name_by_info(session);
            let active = self.session.active_id() == Some(session.id.as_str());
            let mut subtitle = format!(
                "{} - {}",
                session_kind_label(session.kind),
                short_id(&session.id)
            );
            if let Some(path) = session.working_dir.as_ref() {
                subtitle.push_str(" - ");
                subtitle.push_str(&path.display().to_string());
            }
            session_items.push(QuickSwitchItem::Session {
                id: session.id.clone(),
                title,
                subtitle,
                active,
            });
        }

        let mut transient_items = self
            .session
            .start_pending_entries()
            .filter(|(_, pending)| pending.reconnect_session_id.is_none())
            .map(|(request_id, pending)| {
                let insert_index = quick_switch_transient_insert_index(
                    sessions.len(),
                    pending.insert_index,
                    pending.after_session_id.as_deref().and_then(|after_id| {
                        sessions.iter().position(|session| session.id == after_id)
                    }),
                );
                let title = pending
                    .custom_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .unwrap_or(&pending.connection_name)
                    .to_string();
                (
                    insert_index,
                    pending.requested_at,
                    pending.connection_name.clone(),
                    QuickSwitchItem::Pending {
                        request_id: request_id.clone(),
                        title,
                        subtitle: format!(
                            "{} - {}",
                            t!("sessionQuickSwitcher.connecting"),
                            session_kind_label(pending.kind)
                        ),
                        active: self.session.start_request_is_active(request_id),
                        failed: false,
                        search_detail: None,
                    },
                )
            })
            .chain(
                self.session
                    .start_failed_entries()
                    .map(|(request_id, failed)| {
                        let pending = &failed.pending;
                        let insert_index = quick_switch_transient_insert_index(
                            sessions.len(),
                            pending.insert_index,
                            pending.after_session_id.as_deref().and_then(|after_id| {
                                sessions.iter().position(|session| session.id == after_id)
                            }),
                        );
                        let title = pending
                            .custom_name
                            .as_deref()
                            .map(str::trim)
                            .filter(|name| !name.is_empty())
                            .unwrap_or(&pending.connection_name)
                            .to_string();
                        (
                            insert_index,
                            pending.requested_at,
                            pending.connection_name.clone(),
                            QuickSwitchItem::Pending {
                                request_id: request_id.clone(),
                                title,
                                subtitle: format!(
                                    "{} - {}",
                                    t!("terminal.connectionFailed"),
                                    session_kind_label(pending.kind)
                                ),
                                active: self.session.start_request_is_active(request_id),
                                failed: true,
                                search_detail: Some(failed.error.clone()),
                            },
                        )
                    }),
            )
            .collect::<Vec<_>>();
        transient_items.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then(left.1.cmp(&right.1))
                .then(left.2.cmp(&right.2))
        });
        for (offset, (insert_index, _, _, item)) in transient_items.into_iter().enumerate() {
            session_items.insert((insert_index + offset).min(session_items.len()), item);
        }

        let mut items = session_items;

        let mut connections = self.connection_state.connections().to_vec();
        connections.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
                .then(left.id.cmp(&right.id))
        });
        for connection in connections {
            items.push(QuickSwitchItem::Connection {
                title: connection.name.clone(),
                subtitle: format!("{} - {}", connection.kind_label(), connection.endpoint()),
                connection: Box::new(connection),
            });
        }
        items
    }

    pub(in crate::features) fn filtered_quick_switch_items(
        &self,
        cx: &mut Context<Self>,
    ) -> Vec<QuickSwitchItem> {
        let items = self.quick_switch_items();
        let query = self
            .quick_switch_state(cx)
            .query()
            .trim()
            .to_ascii_lowercase();
        if query.is_empty() {
            return items;
        }
        let candidates = items
            .iter()
            .map(|item| (item.search_text(), item.id()))
            .collect::<Vec<_>>();
        let candidate_refs = candidates
            .iter()
            .map(|(display, id)| (display.as_str(), id.as_str()))
            .collect::<Vec<_>>();
        let matches = fuzzy_search_items(
            &candidate_refs,
            &query,
            "sessionQuickSwitcher",
            50,
            None,
            None,
        );
        let mut items_by_id = items
            .into_iter()
            .map(|item| (item.id(), item))
            .collect::<HashMap<_, _>>();
        matches
            .into_iter()
            .filter_map(|matched| items_by_id.remove(&matched.command))
            .collect()
    }

    pub(in crate::features) fn select_quick_switch_item(
        &mut self,
        item: QuickSwitchItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_quick_switch_state(cx, |store| store.reset_quick_switch_input_and_close());
        self.forget_text_inputs("quick-switch.query");
        match item {
            QuickSwitchItem::Session { id, .. } => {
                self.select_session(id, cx);
            }
            QuickSwitchItem::Connection { connection, .. } => {
                self.start_saved_connection(*connection, window, cx);
            }
            QuickSwitchItem::Pending {
                request_id, failed, ..
            } => {
                if failed {
                    self.select_failed_session_start(request_id, cx);
                } else {
                    self.select_pending_session_start(request_id, cx);
                }
                cx.notify();
            }
        }
    }

    pub(in crate::features) fn handle_quick_switch_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mark_user_activity();
        let items = self.filtered_quick_switch_items(cx);
        let keystroke = &event.keystroke;
        if keystroke.modifiers.platform || keystroke.modifiers.alt || keystroke.modifiers.control {
            return;
        }

        match keystroke.key.as_str() {
            "escape" => self.close_quick_switch(cx),
            "up" => {
                if !items.is_empty() {
                    let selected_index = self.quick_switch_state(cx).selected_index();
                    let next_index = (selected_index + items.len() - 1) % items.len();
                    self.update_quick_switch_state(cx, |store| {
                        store.set_quick_switch_selected_index(next_index)
                    });
                }
                cx.notify();
            }
            "down" => {
                if !items.is_empty() {
                    let selected_index = self.quick_switch_state(cx).selected_index();
                    let next_index = (selected_index + 1) % items.len();
                    self.update_quick_switch_state(cx, |store| {
                        store.set_quick_switch_selected_index(next_index)
                    });
                }
                cx.notify();
            }
            "enter" => {
                let selected_index = self.quick_switch_state(cx).selected_index();
                if let Some(item) = items
                    .get(selected_index.min(items.len().saturating_sub(1)))
                    .cloned()
                {
                    self.select_quick_switch_item(item, window, cx);
                }
            }
            _ => {}
        }
    }

    pub(in crate::features) fn apply_quick_switch_query(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.update_quick_switch_state(cx, |store| store.set_quick_switch_query(text));
    }
}

fn quick_switch_transient_insert_index(
    session_count: usize,
    insert_index: Option<usize>,
    after_position: Option<usize>,
) -> usize {
    insert_index
        .or_else(|| after_position.map(|position| position + 1))
        .unwrap_or(session_count)
        .min(session_count)
}

#[cfg(test)]
mod tests {
    use super::quick_switch_transient_insert_index;

    #[test]
    fn transient_items_follow_workspace_tab_insertion_rules() {
        assert_eq!(quick_switch_transient_insert_index(3, None, None), 3);
        assert_eq!(quick_switch_transient_insert_index(3, None, Some(0)), 1);
        assert_eq!(quick_switch_transient_insert_index(3, Some(2), Some(0)), 2);
        assert_eq!(quick_switch_transient_insert_index(3, Some(99), None), 3);
    }
}
