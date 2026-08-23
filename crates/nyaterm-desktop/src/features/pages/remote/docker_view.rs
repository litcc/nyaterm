use rust_i18n::t;

use gpui::{Context, IntoElement, div, prelude::*, px, rgb};

use crate::features::{NyaTermApp, remote::DockerDerivedItems, text_inputs::TextInputSetup};
use crate::widgets::empty_panel;

use super::docker::{
    DockerComposePanelState, DockerContainersPanelState, DockerLabels, DockerRenderContext,
    DockerTabBarLabels, docker_compose_panel, docker_containers_panel, docker_details_panel,
    docker_images_panel, docker_networks_panel, docker_overview_strip, docker_tab_bar,
    docker_volumes_panel,
};

impl NyaTermApp {
    pub(in crate::features) fn docker_view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut docker = self.remote_ops.docker_presentation();
        let palette = self.theme_palette();
        let labels = DockerLabels {
            search: t!("dockerManager.search"),
            no_session: t!("dockerManager.noSession"),
            error: t!("dockerManager.error"),
            unavailable: t!("dockerManager.unavailable"),
            no_matches: t!("dockerManager.noMatches"),
            logs: t!("dockerManager.logs"),
            enter: t!("dockerManager.enter"),
            start: t!("dockerManager.start"),
            stop: t!("dockerManager.stop"),
            restart: t!("dockerManager.restart"),
            kill: t!("dockerManager.kill"),
            delete: t!("common.delete"),
            confirm_action_title: t!("dockerManager.confirmActionTitle"),
            networks: t!("dockerManager.networks"),
            remove_image: t!("dockerManager.removeImage"),
            remove_volume: t!("dockerManager.removeVolume"),
            remove_network: t!("dockerManager.removeNetwork"),
            up: t!("dockerManager.up"),
            down: t!("dockerManager.down"),
            loading_services: t!("dockerManager.loadingServices"),
            service_load_failed: t!("dockerManager.serviceLoadFailed"),
            no_services: t!("dockerManager.noServices"),
            no_containers: t!("dockerManager.noContainers"),
            not_created: t!("dockerManager.notCreated"),
            retry: t!("common.retry"),
            loading: t!("common.loading"),
            container_details: t!("dockerManager.containerDetails"),
            identity: t!("dockerManager.identity"),
            container_name: t!("dockerManager.containerName"),
            container_id: t!("dockerManager.containerId"),
            image: t!("dockerManager.image"),
            status: t!("dockerManager.status"),
            created_at: t!("dockerManager.createdAt"),
            size: t!("dockerManager.size"),
            started_at: t!("dockerManager.startedAt"),
            finished_at: t!("dockerManager.finishedAt"),
            restart_count: t!("dockerManager.restartCount"),
            entrypoint: t!("dockerManager.entrypoint"),
            command: t!("dockerManager.command"),
            networking: t!("dockerManager.networking"),
            ports: t!("dockerManager.ports"),
            io: t!("dockerManager.io"),
            net_io: t!("dockerManager.netIo"),
            block_io: t!("dockerManager.blockIo"),
            mounts: t!("dockerManager.mounts"),
            cpu: t!("dockerManager.cpu"),
            memory: t!("dockerManager.memory"),
            pids: t!("dockerManager.pids"),
            copy: t!("common.copyToClipboard"),
            refresh: t!("common.refresh"),
            close: t!("common.close"),
            state_created: t!("dockerManager.stateLabels.created"),
            state_dead: t!("dockerManager.stateLabels.dead"),
            state_exited: t!("dockerManager.stateLabels.exited"),
            state_paused: t!("dockerManager.stateLabels.paused"),
            state_removing: t!("dockerManager.stateLabels.removing"),
            state_restarting: t!("dockerManager.stateLabels.restarting"),
            state_running: t!("dockerManager.stateLabels.running"),
            state_unknown: t!("dockerManager.stateLabels.unknown"),
        };
        // Built before the view, which reads `self` throughout: creating the
        // box needs it mutably.
        let docker_search_input = self
            .search_input_box(
                "remote.docker.filter",
                &docker.search_draft.clone(),
                TextInputSetup::placeholder(labels.search.clone()),
                cx,
            )
            .into_any_element();
        if self.session.active_ssh_config().is_none() {
            return div()
                .size_full()
                .bg(self.shell_transparent_color(palette.surface))
                .child(empty_panel(labels.no_session.clone(), palette));
        }
        let Some(overview) = docker.overview.take() else {
            let message = if docker.pending || !docker.status.contains("failed") {
                labels.loading.clone()
            } else {
                labels.error.clone()
            };
            return div()
                .size_full()
                .bg(self.shell_transparent_color(palette.surface))
                .child(empty_panel(message, palette));
        };
        if !overview.available {
            return div()
                .size_full()
                .bg(self.shell_transparent_color(palette.surface))
                .child(empty_panel(labels.unavailable.clone(), palette));
        }
        // Both the effective tab and the filtered list are resolved by
        // `RemoteOpsFeatureState`, which recomputes them when the overview, the query
        // or the tab changes. This pass only reads them.
        let active_tab = self.remote_ops.docker_effective_tab();
        let query_empty = docker.search_draft.trim().is_empty();
        let filtered = self.remote_ops.derived_docker_items();
        let menu_bg = self.shell_surface_color(palette.surface);
        let dialog_bg = self.shell_surface_color(palette.bg);
        let render_context = DockerRenderContext {
            palette,
            menu_bg,
            labels: labels.clone(),
        };
        let docker_content = match filtered {
            DockerDerivedItems::Containers(filtered) => docker_containers_panel(
                render_context.clone(),
                DockerContainersPanelState {
                    has_snapshot: true,
                    has_session: self.session.active_ssh_config().is_some(),
                    docker_available: overview.available,
                    filtered_containers: filtered.as_ref(),
                    query_empty,
                    open_menu_id: docker.container_menu_id.as_deref(),
                    list_offset: docker.list_offset,
                },
                cx,
            )
            .into_any_element(),
            DockerDerivedItems::Images(filtered) => docker_images_panel(
                palette,
                filtered.as_ref(),
                docker.resource_list_offset,
                labels.clone(),
                cx,
            )
            .into_any_element(),
            DockerDerivedItems::Volumes(filtered) => docker_volumes_panel(
                palette,
                filtered.as_ref(),
                docker.resource_list_offset,
                labels.clone(),
                cx,
            )
            .into_any_element(),
            DockerDerivedItems::Networks(filtered) => docker_networks_panel(
                palette,
                filtered.as_ref(),
                docker.resource_list_offset,
                labels.clone(),
                cx,
            )
            .into_any_element(),
            DockerDerivedItems::Compose(filtered) => docker_compose_panel(
                render_context.clone(),
                DockerComposePanelState {
                    projects: filtered.as_ref(),
                    expanded_projects: &docker.compose_expanded,
                    services_by_project: &docker.compose_services,
                    service_errors: &docker.compose_service_errors,
                    open_menu_id: docker.compose_menu_id.as_deref(),
                },
                cx,
            )
            .into_any_element(),
        };

        // Tauri DockerManager shell: header actions + dense search + tabs + flex list body.
        // Shared PanelHeader already shows title/meta; avoid page-like section headers.
        div()
            .flex()
            .flex_col()
            .size_full()
            .relative()
            .overflow_hidden()
            .p(px(10.))
            .gap(px(10.))
            .bg(self.shell_transparent_color(palette.surface))
            .when(overview.available, |this| {
                this.child(docker_overview_strip(
                    palette,
                    &overview,
                    [
                        t!("dockerManager.running").to_string(),
                        t!("dockerManager.stopped").to_string(),
                        t!("dockerManager.images").to_string(),
                    ],
                ))
            })
            .child(
                div()
                    .h(px(32.))
                    .flex_none()
                    .px_2()
                    .border_b_1()
                    .border_color(rgb(palette.border))
                    .bg(self.shell_transparent_color(palette.section_header))
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(div().flex_1().min_w_0().child(docker_search_input)),
            )
            .child(docker_tab_bar(
                render_context,
                active_tab,
                &overview,
                DockerTabBarLabels {
                    tabs: [
                        t!("dockerManager.containers").to_string(),
                        t!("dockerManager.images").to_string(),
                        t!("dockerManager.volumes").to_string(),
                        t!("dockerManager.networks").to_string(),
                        t!("dockerManager.compose").to_string(),
                    ],
                    more: t!("common.more").to_string(),
                },
                self.shell.right_panel_width(),
                docker.tab_menu_open,
                cx,
            ))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(docker_content),
            )
            .when_some(docker.details_container_id.clone(), |this, container_id| {
                this.child(docker_details_panel(
                    palette,
                    dialog_bg,
                    Some(container_id.clone()),
                    docker.details.clone(),
                    overview
                        .containers
                        .iter()
                        .find(|container| container.id == container_id)
                        .cloned(),
                    labels,
                    cx,
                ))
            })
    }
}
