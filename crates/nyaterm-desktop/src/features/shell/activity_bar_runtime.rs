use gpui::{
    Context, IntoElement, MouseDownEvent, ParentElement as _, Render, Styled as _, Window, div, px,
    rgb, rgba,
};
use nyaterm_core::truncate_preview;

use crate::features::{NyaTermApp, runtime_jobs::ActivitySide, view_widgets::mono_icon};
use crate::models::{
    ActivityBarContextMenuState, ActivityBarEntry, ActivityBarLayoutState, ActivityBarZone,
    BottomPanelMode, MainMode, NavItem, PanelSide,
};

#[derive(Clone, Debug)]
pub(in crate::features) struct ActivityBarDragPayload {
    pub entry_id: String,
    pub label: String,
    /// The dragged entry's own icon, so the preview reads as the thing being moved.
    pub icon_path: &'static str,
}

pub(in crate::features) struct ActivityBarDragPreview {
    payload: ActivityBarDragPayload,
    position: gpui::Point<gpui::Pixels>,
}

impl ActivityBarDragPreview {
    pub(in crate::features) fn new(
        payload: ActivityBarDragPayload,
        position: gpui::Point<gpui::Pixels>,
    ) -> Self {
        Self { payload, position }
    }
}

impl Render for ActivityBarDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .pl(self.position.x - px(72.))
            .pt(self.position.y - px(18.))
            .child(
                div()
                    .w(px(144.))
                    .h(px(36.))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(0x334155))
                    .bg(rgba(0x151b24dd))
                    .shadow_lg()
                    .child(mono_icon(self.payload.icon_path, rgb(0x93c5fd).into(), 14.))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_size(px(12.))
                            .text_color(rgb(0xc9d1d9))
                            .child(truncate_preview(&self.payload.label, 16)),
                    ),
            )
    }
}

impl NyaTermApp {
    pub(in crate::features) fn activity_side_has_items(&self, side: ActivitySide) -> bool {
        let zones = match side {
            ActivitySide::Left => [ActivityBarZone::LeftTop, ActivityBarZone::LeftBottom],
            ActivitySide::Right => [ActivityBarZone::RightTop, ActivityBarZone::RightBottom],
        };
        zones
            .into_iter()
            .any(|zone| !self.shell.chrome.activity_bar_layout.zone(zone).is_empty())
    }

    pub(in crate::features) fn activity_entries_for_zone(
        &self,
        zone: ActivityBarZone,
    ) -> Vec<ActivityBarEntry> {
        self.shell
            .chrome
            .activity_bar_layout
            .zone(zone)
            .iter()
            .filter_map(|id| ActivityBarEntry::from_persistence_id(id))
            .filter(|entry| self.activity_entry_visible(*entry))
            .collect()
    }

    fn activity_entry_visible(&self, entry: ActivityBarEntry) -> bool {
        let summary = self.settings.summary();
        match entry {
            ActivityBarEntry::Panel(NavItem::Stats) => summary.ui_show_remote_stats,
            ActivityBarEntry::Panel(NavItem::GpuMonitor) => summary.ui_show_gpu_monitor,
            ActivityBarEntry::Panel(NavItem::AscendNpuMonitor) => {
                summary.ui_show_ascend_npu_monitor
            }
            ActivityBarEntry::Panel(NavItem::Processes) => summary.ui_show_process_manager,
            ActivityBarEntry::Panel(NavItem::Docker) => summary.ui_show_docker_manager,
            _ => true,
        }
    }

    pub(in crate::features) fn apply_activity_layout_from_settings(&mut self) {
        self.shell.chrome.activity_bar_layout = ActivityBarLayoutState {
            left_top: self.settings.summary().ui_activity_bar_left_top.clone(),
            left_bottom: self.settings.summary().ui_activity_bar_left_bottom.clone(),
            right_top: self.settings.summary().ui_activity_bar_right_top.clone(),
            right_bottom: self.settings.summary().ui_activity_bar_right_bottom.clone(),
            show_labels: self.settings.summary().ui_activity_bar_show_labels,
        };
        self.normalize_activity_bar_layout();
    }

    pub(in crate::features) fn normalize_activity_bar_layout(&mut self) {
        let mut seen = std::collections::HashSet::new();
        for zone in ActivityBarZone::all() {
            let mut next = Vec::new();
            for id in self
                .shell
                .chrome
                .activity_bar_layout
                .zone(zone)
                .iter()
                .cloned()
            {
                if id == "fileTransfer" {
                    continue;
                }
                if seen.insert(id.clone()) {
                    next.push(id);
                }
            }
            *self.shell.chrome.activity_bar_layout.zone_mut(zone) = next;
        }
        // Keep intentionally empty zones empty. Tauri only restores missing entries;
        // it does not repopulate a zone after the user moves its last item away.
        let defaults = ActivityBarLayoutState::default();
        for zone in ActivityBarZone::all() {
            let missing = defaults
                .zone(zone)
                .iter()
                .filter(|id| !seen.contains(*id))
                .cloned()
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                self.shell
                    .chrome
                    .activity_bar_layout
                    .zone_mut(zone)
                    .extend(missing);
                seen.extend(defaults.zone(zone).iter().cloned());
            }
        }
    }

    pub(in crate::features) fn toggle_activity_bar_labels(&mut self, cx: &mut Context<Self>) {
        self.shell.chrome.activity_bar_layout.show_labels =
            !self.shell.chrome.activity_bar_layout.show_labels;
        self.shell
            .set_status(if self.shell.chrome.activity_bar_layout.show_labels {
                "activity labels shown".to_string()
            } else {
                "activity labels hidden".to_string()
            });
        self.persist_ui_layout();
        cx.notify();
    }

    pub(in crate::features) fn open_activity_bar_context_menu(
        &mut self,
        entry_id: String,
        zone: ActivityBarZone,
        index: usize,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        self.shell.chrome.activity_bar_context_menu = Some(ActivityBarContextMenuState {
            entry_id,
            zone,
            index,
            x: event.position.x,
            y: event.position.y,
            move_submenu_open: false,
        });
        cx.notify();
    }

    pub(in crate::features) fn open_activity_bar_move_submenu(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = self.shell.chrome.activity_bar_context_menu.as_mut() else {
            return;
        };
        if !menu.move_submenu_open {
            menu.move_submenu_open = true;
            cx.notify();
        }
    }

    pub(in crate::features) fn close_activity_bar_context_menu(&mut self, cx: &mut Context<Self>) {
        self.shell.chrome.activity_bar_context_menu = None;
        cx.notify();
    }

    pub(in crate::features) fn move_activity_entry(
        &mut self,
        entry_id: String,
        target_zone: ActivityBarZone,
        target_index: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let Some((source_zone, source_index)) =
            self.shell.chrome.activity_bar_layout.find_entry(&entry_id)
        else {
            self.shell.set_status("activity item not found".to_string());
            cx.notify();
            return;
        };

        // Same entry dropped on itself — no-op.
        if source_zone == target_zone {
            let len = self
                .shell
                .chrome
                .activity_bar_layout
                .zone(target_zone)
                .len();
            let mut insert_at = target_index.unwrap_or(len);
            if source_index < insert_at {
                insert_at = insert_at.saturating_sub(1);
            }
            insert_at = insert_at.min(len.saturating_sub(1));
            if insert_at == source_index {
                self.shell.chrome.activity_bar_context_menu = None;
                cx.notify();
                return;
            }
        }

        // Remove from source.
        let removed = self
            .shell
            .chrome
            .activity_bar_layout
            .zone_mut(source_zone)
            .remove(source_index);
        let mut insert_at = target_index.unwrap_or_else(|| {
            self.shell
                .chrome
                .activity_bar_layout
                .zone(target_zone)
                .len()
        });
        if source_zone == target_zone && source_index < insert_at {
            insert_at = insert_at.saturating_sub(1);
        }
        insert_at = insert_at.min(
            self.shell
                .chrome
                .activity_bar_layout
                .zone(target_zone)
                .len(),
        );
        self.shell
            .chrome
            .activity_bar_layout
            .zone_mut(target_zone)
            .insert(insert_at, removed);

        // Mirror Tauri: when an open panel moves across left/right, clear that side's open state.
        let source_side = Self::activity_zone_side(source_zone);
        let target_side = Self::activity_zone_side(target_zone);
        if source_side != target_side {
            self.clear_activity_entry_from_side(&entry_id, source_side);
        }

        self.shell.chrome.activity_bar_context_menu = None;
        self.shell.set_status(format!(
            "moved {} to {}",
            entry_id,
            target_zone.label().to_lowercase()
        ));
        self.persist_ui_layout();
        cx.notify();
    }

    fn activity_zone_side(zone: ActivityBarZone) -> PanelSide {
        match zone {
            ActivityBarZone::LeftTop | ActivityBarZone::LeftBottom => PanelSide::Left,
            ActivityBarZone::RightTop | ActivityBarZone::RightBottom => PanelSide::Right,
        }
    }

    fn clear_activity_entry_from_side(&mut self, entry_id: &str, side: PanelSide) {
        match side {
            PanelSide::Left => {
                self.shell.panels.left_open.retain(|id| id != entry_id);
                if self
                    .shell
                    .panels
                    .active_left
                    .is_some_and(|item| item.persistence_id() == entry_id)
                {
                    self.shell.panels.active_left = None;
                }
                if self.shell.panels.left_open.is_empty() && self.shell.panels.active_left.is_none()
                {
                    self.shell.panels.left_collapsed = true;
                }
            }
            PanelSide::Right => {
                self.shell.panels.right_open.retain(|id| id != entry_id);
                if self
                    .shell
                    .panels
                    .active_right
                    .is_some_and(|item| item.persistence_id() == entry_id)
                {
                    self.shell.panels.active_right = None;
                }
                if self.shell.panels.right_open.is_empty()
                    && self.shell.panels.active_right.is_none()
                {
                    self.shell.panels.right_collapsed = true;
                }
            }
        }
    }

    pub(in crate::features) fn activate_activity_entry(
        &mut self,
        entry: ActivityBarEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match entry {
            ActivityBarEntry::Panel(NavItem::Settings) => self.open_page(NavItem::Settings, cx),
            ActivityBarEntry::Panel(item) => {
                let side = self.panel_side_for_item(item);
                self.open_panel(item, cx);
                if !cfg!(target_os = "macos") {
                    match side {
                        Some(PanelSide::Left) if self.shell.viewport.size.0 < 1024. => {
                            self.shell.panels.mobile_left_open = true;
                        }
                        Some(PanelSide::Right) if self.shell.viewport.size.0 < 768. => {
                            self.shell.panels.mobile_right_open = true;
                        }
                        _ => {}
                    }
                }
                cx.notify();
            }
            ActivityBarEntry::QuickCommands => {
                let mode = if self.shell.bottom_panel.mode == BottomPanelMode::QuickCommands {
                    BottomPanelMode::Hidden
                } else {
                    BottomPanelMode::QuickCommands
                };
                self.set_bottom_panel_mode(mode);
                cx.notify();
            }
            ActivityBarEntry::CommandSend => {
                let mode = if self.shell.bottom_panel.mode == BottomPanelMode::CommandSend {
                    BottomPanelMode::Hidden
                } else {
                    BottomPanelMode::CommandSend
                };
                self.set_bottom_panel_mode(mode);
                cx.notify();
            }
            ActivityBarEntry::Recording => {
                let side = self.panel_side_for_item(NavItem::Recording);
                self.open_panel(NavItem::Recording, cx);
                if !cfg!(target_os = "macos") {
                    match side {
                        Some(PanelSide::Left) if self.shell.viewport.size.0 < 1024. => {
                            self.shell.panels.mobile_left_open = true;
                        }
                        Some(PanelSide::Right) if self.shell.viewport.size.0 < 768. => {
                            self.shell.panels.mobile_right_open = true;
                        }
                        _ => {}
                    }
                }
                cx.notify();
            }
            ActivityBarEntry::Lock => self.lock_app(window, cx),
        }
    }

    pub(in crate::features) fn activity_entry_selected(&self, entry: ActivityBarEntry) -> bool {
        match entry {
            ActivityBarEntry::Panel(NavItem::Settings) => {
                self.shell.navigation.settings.window.is_open()
                    || self.shell.navigation.main_mode == MainMode::Page
            }
            ActivityBarEntry::Panel(item) => self.panel_entry_selected(item),
            ActivityBarEntry::QuickCommands => {
                self.shell.bottom_panel.mode == BottomPanelMode::QuickCommands
            }
            ActivityBarEntry::CommandSend => {
                self.shell.bottom_panel.mode == BottomPanelMode::CommandSend
            }
            ActivityBarEntry::Recording => {
                self.panel_entry_selected(NavItem::Recording) || self.recording.active_count() > 0
            }
            ActivityBarEntry::Lock => self.security.screen_locked(),
        }
    }

    fn panel_entry_selected(&self, item: NavItem) -> bool {
        let Some(side) = self.panel_side_for_item(item) else {
            return false;
        };
        if self.shell.panels.multi_open {
            let id = item.persistence_id();
            self.side_open_panel_ids(side).iter().any(|open| open == id)
                || self.side_overlay_panel(side) == Some(item)
                || match side {
                    PanelSide::Left => self.shell.panels.active_left == Some(item),
                    PanelSide::Right => self.shell.panels.active_right == Some(item),
                }
        } else {
            match side {
                PanelSide::Left => self.current_left_panel() == Some(item),
                PanelSide::Right => self.current_right_panel() == Some(item),
            }
        }
    }
}
