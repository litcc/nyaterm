mod activation;
mod ai;
mod app_state;
mod commands;
mod connections;
mod font_catalog;
mod formatting;
mod icons;
mod inspector;
mod layout;
mod pages;
mod panels;
mod perf;
mod recording;
mod remote;
mod remote_desktop;
mod root;
mod runtime_jobs;
mod selects;
mod session;
mod settings;
mod shell;
mod sync;
mod sync_input;
mod terminal;
mod text_inputs;
mod transfers;
mod translation;
mod tunnels;
mod update;
mod view_widgets;

pub(crate) fn init(cx: &mut gpui::App) {
    terminal::init_key_bindings(cx);
    view_widgets::init_child_window_key_bindings(cx);
}

pub use app_state::NyaTermApp;
pub(in crate::features) use font_catalog::{
    FontAvailability, FontAvailabilityReason, FontCatalogEntry, FontCatalogKind,
    FontCatalogLoadState, FontCatalogPresentation, FontCatalogSnapshot, FontCatalogState,
    FontResolutionSource, FontResolutionStatus, font_names_fingerprint, normalize_font_family,
};
