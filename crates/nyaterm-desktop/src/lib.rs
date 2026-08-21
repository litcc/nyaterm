//! GPUI presentation crate for NyaTerm.

mod action_links;
mod i18n;
mod send_command;
mod shortcuts;
mod temporary_ssh_link;

pub mod app_shell;
pub mod entities;
pub mod features;
pub mod http;
pub mod models;
pub mod terminal;
pub mod theme;
pub mod widgets;

// Generates `crate::_rust_i18n_t!`, which `rust_i18n::t!` forwards to, so this must
// stay in the crate root. `locales/` is resolved against CARGO_MANIFEST_DIR at macro
// expansion time; `build.rs` is what makes cargo notice edits to it.
rust_i18n::i18n!("locales", fallback = "en");

pub use app_shell::AppShell;

pub fn init(cx: &mut gpui::App) {
    features::init(cx);
}
