use super::{OverlayStore, StartupRestoreStore};

#[test]
fn startup_restore_store_starts_after_window_open_once() {
    let mut store = StartupRestoreStore::default();

    assert!(store.mark_started_after_window_open());
    assert!(!store.mark_started_after_window_open());
    assert!(store.started_after_window_open());
}

#[test]
fn startup_restore_store_tracks_queue_admission() {
    let mut store = StartupRestoreStore::default();
    store.set_queue(vec![
        nyaterm_core::RestorableOpenTab::with_leaf_root("one", "Local", None, None, None),
        nyaterm_core::RestorableOpenTab::with_leaf_root("two", "Local", None, None, None),
    ]);

    assert_eq!(store.queue_len(), 2);
    assert!(store.can_pump_queue(false));
    assert!(!store.can_pump_queue(true));
    assert_eq!(
        store.pop_next_tab().as_ref().map(|tab| tab.title.as_str()),
        Some("one")
    );
    assert_eq!(store.queue_len(), 1);
    assert_eq!(
        store.pop_next_tab().as_ref().map(|tab| tab.title.as_str()),
        Some("two")
    );
    assert!(!store.can_pump_queue(false));
}

#[test]
fn overlay_store_owns_quick_switch_state() {
    let mut store = OverlayStore::default();

    assert!(!store.quick_switch().is_open());
    assert!(store.open_quick_switch());
    assert!(!store.open_quick_switch());
    assert!(store.quick_switch().is_open());
    assert!(store.set_quick_switch_query("ssh".to_string()));
    assert_eq!(store.quick_switch().query(), "ssh");
    assert!(!store.set_quick_switch_selected_index(0));
    assert!(store.set_quick_switch_selected_index(3));
    assert!(store.clamp_quick_switch_selected_index(2));
    assert_eq!(store.quick_switch().selected_index(), 1);
    assert!(store.set_quick_switch_query("host".to_string()));
    assert_eq!(store.quick_switch().query(), "host");
    assert!(store.close_quick_switch());
    assert_eq!(
        store.quick_switch(),
        &crate::entities::QuickSwitchState::default()
    );
}
