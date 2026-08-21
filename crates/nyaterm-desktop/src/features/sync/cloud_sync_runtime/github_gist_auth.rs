use rust_i18n::t;

use gpui::{ClipboardItem, Context};

use crate::features::NyaTermApp;
use crate::http::cloud_sync::run_github_gist_device_flow;
use crate::models::GithubGistAuthEvent;

const GITHUB_GIST_AUTH_EVENT_DRAIN_LIMIT: usize = 8;

impl NyaTermApp {
    pub(in crate::features) fn start_github_gist_auth(&mut self, cx: &mut Context<Self>) {
        if !self.cloud_sync_form_enabled() {
            return;
        }
        let waiting_message = t!("settings.githubGistWaitingForAuth").to_string();
        let Some(job) = self.cloud_sync.begin_github_auth(waiting_message) else {
            return;
        };
        let job_id = job.job_id();
        let existing_gist_id = job.existing_gist_id();
        let cancel = job.cancel();
        let tx = job.sender();
        std::thread::spawn(move || {
            run_github_gist_device_flow(job_id, existing_gist_id, cancel, tx);
        });
        cx.notify();
    }

    pub(in crate::features) fn cancel_github_gist_auth(&mut self, cx: &mut Context<Self>) {
        self.cloud_sync.cancel_github_auth();
        cx.notify();
    }

    pub(in crate::features) fn copy_github_gist_user_code(&mut self, cx: &mut Context<Self>) {
        let Some(code) = self.cloud_sync.github_auth().user_code.clone() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(code));
        self.cloud_sync
            .set_status(t!("settings.githubGistUserCodeCopied").to_string());
        cx.notify();
    }

    pub(in crate::features) fn open_github_gist_verification_url(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(url) = self.cloud_sync.github_auth().verification_uri.clone() else {
            return;
        };
        self.open_external_url_for_ui(&url, cx);
    }

    pub(in crate::features) fn drain_github_gist_auth_events(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut dirty = false;
        for event in self
            .cloud_sync
            .drain_github_auth_events(GITHUB_GIST_AUTH_EVENT_DRAIN_LIMIT)
        {
            dirty = true;
            match event {
                GithubGistAuthEvent::Started {
                    user_code,
                    verification_uri,
                } => {
                    let message = t!("settings.githubGistWaitingForAuth").to_string();
                    self.cloud_sync.apply_github_auth_started(
                        user_code,
                        verification_uri.clone(),
                        message,
                    );
                    self.open_external_url_for_ui(&verification_uri, cx);
                }
                GithubGistAuthEvent::Polling { slow_down } => {
                    let message = t!(if slow_down {
                        "settings.githubGistSlowDown"
                    } else {
                        "settings.githubGistWaitingForAuth"
                    })
                    .to_string();
                    self.cloud_sync.apply_github_auth_polling(message);
                }
                GithubGistAuthEvent::Succeeded {
                    access_token,
                    gist_id,
                    login,
                } => {
                    let message = t!("settings.githubGistConnected").to_string();
                    self.cloud_sync.apply_github_auth_succeeded(
                        access_token,
                        gist_id,
                        login,
                        message,
                    );
                    self.shell.set_status(self.cloud_sync.status().to_string());
                }
                GithubGistAuthEvent::Failed(error) => {
                    let message = if error.contains("OAuth Client ID is not configured") {
                        t!("settings.githubGistClientIdMissing").to_string()
                    } else {
                        error
                    };
                    self.cloud_sync.apply_github_auth_failed(message);
                    self.shell.set_status(self.cloud_sync.status().to_string());
                }
                GithubGistAuthEvent::Cancelled => {
                    self.cloud_sync.apply_github_auth_cancelled();
                }
            }
        }
        dirty
    }
}
