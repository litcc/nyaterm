use rust_i18n::t;

use gpui::{Context, IntoElement, Window};
use nyaterm_ui::NyaDialogWindowExt as _;

use crate::features::NyaTermApp;
use crate::http::update::check_native_update;

use super::state::UpdateJobResult;

impl NyaTermApp {
    pub(in crate::features) fn open_update_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_nya_dialog(cx) {
            cx.notify();
            return;
        }
        self.open_content_dialog(
            t!("updater.checking").to_string(),
            560.,
            |app, _, cx| app.update_dialog_content(cx).into_any_element(),
            |_, _| {},
            window,
            cx,
        );
        self.start_update_check(cx);
    }

    pub(in crate::features) fn close_update_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.close_nya_dialog(cx);
        cx.notify();
    }

    pub(in crate::features) fn start_update_check(&mut self, cx: &mut Context<Self>) {
        let Some(tx) = self.update.begin_check() else {
            cx.notify();
            return;
        };
        std::thread::spawn(move || {
            let result = check_native_update();
            let _ = tx.send(UpdateJobResult::new(result));
        });
        cx.notify();
    }

    pub(in crate::features) fn drain_update_events(&mut self) -> bool {
        let dirty = self.update.drain_events();
        if dirty {
            self.shell.set_status(self.update.status().to_string());
        }
        dirty
    }
}
