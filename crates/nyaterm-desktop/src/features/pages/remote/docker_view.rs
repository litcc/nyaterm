use gpui::{Context, IntoElement, div, prelude::*, px, rgb};

use crate::features::{NyaTermApp, remote::DockerDerivedItems, text_inputs::TextInputSetup};
use crate::models::DockerTab;
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
            search: self.tr("dockerManager.search"),
            no_session: self.tr("dockerManager.noSession"),
            error: self.tr("dockerManager.error"),
            unavailable: self.tr("dockerManager.unavailable"),
            no_matches: self.tr("dockerManager.noMatches"),
            logs: self.tr("dockerManager.logs"),
            enter: self.tr("dockerManager.enter"),
            start: self.tr("dockerManager.start"),
            stop: self.tr("dockerManager.stop"),
            restart: self.tr("dockerManager.restart"),
            kill: self.tr("dockerManager.kill"),
            delete: self.tr("common.delete"),
            confirm_action_title: self.tr("dockerManager.confirmActionTitle"),
            confirm_action_desc: self.tr("dockerManager.confirmActionDesc"),
            networks: self.tr("dockerManager.networks"),
            remove_image: self.tr("dockerManager.removeImage"),
            remove_volume: self.tr("dockerManager.removeVolume"),
            remove_network: self.tr("dockerManager.removeNetwork"),
            volume_driver: self.tr("dockerManager.volumeDriver"),
            up: self.tr("dockerManager.up"),
            down: self.tr("dockerManager.down"),
            loading_services: self.tr("dockerManager.loadingServices"),
            service_load_failed: self.tr("dockerManager.serviceLoadFailed"),
            no_services: self.tr("dockerManager.noServices"),
            no_containers: self.tr("dockerManager.noContainers"),
            not_created: self.tr("dockerManager.notCreated"),
            retry: self.tr("common.retry"),
            loading: self.tr("common.loading"),
            container_details: self.tr("dockerManager.containerDetails"),
            identity: self.tr("dockerManager.identity"),
            container_name: self.tr("dockerManager.containerName"),
            container_id: self.tr("dockerManager.containerId"),
            image: self.tr("dockerManager.image"),
            status: self.tr("dockerManager.status"),
            created_at: self.tr("dockerManager.createdAt"),
            size: self.tr("dockerManager.size"),
            started_at: self.tr("dockerManager.startedAt"),
            finished_at: self.tr("dockerManager.finishedAt"),
            restart_count: self.tr("dockerManager.restartCount"),
            entrypoint: self.tr("dockerManager.entrypoint"),
            command: self.tr("dockerManager.command"),
            networking: self.tr("dockerManager.networking"),
            ports: self.tr("dockerManager.ports"),
            io: self.tr("dockerManager.io"),
            net_io: self.tr("dockerManager.netIo"),
            block_io: self.tr("dockerManager.blockIo"),
            mounts: self.tr("dockerManager.mounts"),
            cpu: self.tr("dockerManager.cpu"),
            memory: self.tr("dockerManager.memory"),
            pids: self.tr("dockerManager.pids"),
            copy: self.tr("common.copyToClipboard"),
            refresh: self.tr("common.refresh"),
            close: self.tr("common.close"),
            state_created: self.tr("dockerManager.stateLabels.created"),
            state_dead: self.tr("dockerManager.stateLabels.dead"),
            state_exited: self.tr("dockerManager.stateLabels.exited"),
            state_paused: self.tr("dockerManager.stateLabels.paused"),
            state_removing: self.tr("dockerManager.stateLabels.removing"),
            state_restarting: self.tr("dockerManager.stateLabels.restarting"),
            state_running: self.tr("dockerManager.stateLabels.running"),
            state_unknown: self.tr("dockerManager.stateLabels.unknown"),
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
        let active_tab = if docker.tab == DockerTab::Compose && !overview.compose_available {
            DockerTab::Containers
        } else {
            docker.tab
        };
        let query_empty = docker.search_draft.trim().is_empty();
        let filtered = self.remote_ops.derived_docker_items(active_tab);
        let menu_bg = self.shell_surface_color(palette.surface);
        let dialog_bg = self.shell_surface_color(palette.bg);
        let render_context = DockerRenderContext {
            palette,
            menu_bg,
            labels: labels.clone(),
        };
        let docker_content = match filtered {
            DockerDerivedItems::Containers(filtered) => {
                const VIEWPORT_ROWS: usize = 16;
                let max_offset = filtered
                    .len()
                    .saturating_sub(VIEWPORT_ROWS.min(filtered.len()));
                docker.list_offset = self.remote_ops.clamp_docker_list_offset(max_offset);
                docker_containers_panel(
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
                .into_any_element()
            }
            DockerDerivedItems::Images(filtered) => {
                docker.resource_list_offset = self
                    .remote_ops
                    .clamp_docker_resource_offset(resource_max_offset(filtered.len()));
                docker_images_panel(
                    palette,
                    filtered.as_ref(),
                    docker.resource_list_offset,
                    labels.clone(),
                    cx,
                )
                .into_any_element()
            }
            DockerDerivedItems::Volumes(filtered) => {
                docker.resource_list_offset = self
                    .remote_ops
                    .clamp_docker_resource_offset(resource_max_offset(filtered.len()));
                docker_volumes_panel(
                    palette,
                    filtered.as_ref(),
                    docker.resource_list_offset,
                    labels.clone(),
                    cx,
                )
                .into_any_element()
            }
            DockerDerivedItems::Networks(filtered) => {
                docker.resource_list_offset = self
                    .remote_ops
                    .clamp_docker_resource_offset(resource_max_offset(filtered.len()));
                docker_networks_panel(
                    palette,
                    filtered.as_ref(),
                    docker.resource_list_offset,
                    labels.clone(),
                    cx,
                )
                .into_any_element()
            }
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
                        self.tr("dockerManager.running").to_string(),
                        self.tr("dockerManager.stopped").to_string(),
                        self.tr("dockerManager.images").to_string(),
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
                        self.tr("dockerManager.containers").to_string(),
                        self.tr("dockerManager.images").to_string(),
                        self.tr("dockerManager.volumes").to_string(),
                        self.tr("dockerManager.networks").to_string(),
                        self.tr("dockerManager.compose").to_string(),
                    ],
                    more: self.tr("common.more").to_string(),
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

fn resource_max_offset(total: usize) -> usize {
    const VIEWPORT_ROWS: usize = 14;
    total.saturating_sub(VIEWPORT_ROWS.min(total))
}
