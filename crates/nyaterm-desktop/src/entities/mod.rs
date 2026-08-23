//! GPUI entity-state boundaries for the native shell.
//!
//! `NyaTermApp` and its feature-state structs are the authoritative UI state.
//! Every store here owns something the app does not: the window runtime pump,
//! the startup-restore queue, and the quick switch overlay state. Read-only or
//! otherwise unobserved projections used to live here too; they were removed
//! once it turned out nothing consumed them.

mod handles;
mod overlay;
mod startup_restore;

#[cfg(test)]
mod tests;

pub use handles::UiStoreHandles;
pub use overlay::{OverlayStore, QuickSwitchState};
pub use startup_restore::StartupRestoreStore;
