use super::{PortableSnapshotError, RawPortableSnapshot, validate_raw_snapshot};

#[test]
fn v3_hash_protects_notes_when_the_entity_is_present() {
    let mut snapshot = RawPortableSnapshot::backup("device-1", "test");
    snapshot.entities.insert(
        "notes".to_string(),
        r#"{"folders":[],"notes":[]}"#.to_string(),
    );
    snapshot.recalculate_hash().expect("hash with notes");
    validate_raw_snapshot(&snapshot).expect("valid snapshot");

    snapshot.entities.insert(
        "notes".to_string(),
        r#"{"folders":[],"notes":[{"id":"note-1","markdown":"changed"}]}"#.to_string(),
    );
    assert!(matches!(
        validate_raw_snapshot(&snapshot),
        Err(PortableSnapshotError::PayloadHashMismatch)
    ));
}

#[test]
fn v3_hash_still_accepts_the_legacy_shape_without_notes() {
    let mut snapshot = RawPortableSnapshot::backup("device-1", "test");
    assert!(!snapshot.entities.contains_key("notes"));
    snapshot.recalculate_hash().expect("legacy hash");
    snapshot.meta.entities_hash = None;
    validate_raw_snapshot(&snapshot).expect("legacy v3 remains valid");
}

#[test]
fn optional_entity_map_hash_protects_unknown_entities() {
    let mut snapshot = RawPortableSnapshot::backup("device-1", "test");
    snapshot.entities.insert(
        "future_entity".to_string(),
        r#"{"schema":7,"payload":{"future":true}}"#.to_string(),
    );
    snapshot.recalculate_hash().expect("hash all entities");
    validate_raw_snapshot(&snapshot).expect("valid snapshot");

    snapshot.entities.insert(
        "future_entity".to_string(),
        r#"{"schema":7,"payload":{"future":false}}"#.to_string(),
    );
    assert!(matches!(
        validate_raw_snapshot(&snapshot),
        Err(PortableSnapshotError::EntitiesHashMismatch)
    ));
}

#[test]
fn legacy_metadata_without_entity_map_hash_deserializes_and_validates() {
    let mut snapshot = RawPortableSnapshot::backup("device-1", "test");
    snapshot.recalculate_hash().expect("hash snapshot");
    let mut value = serde_json::to_value(&snapshot).expect("serialize snapshot");
    value["meta"]
        .as_object_mut()
        .expect("metadata object")
        .remove("entities_hash");

    let decoded: RawPortableSnapshot = serde_json::from_value(value).expect("legacy metadata");
    assert_eq!(decoded.meta.entities_hash, None);
    validate_raw_snapshot(&decoded).expect("legacy snapshot remains valid");
}

#[test]
fn detects_payload_hash_mismatch() {
    let mut snapshot = RawPortableSnapshot::backup("device-1", "test");
    snapshot.recalculate_hash().expect("hash");
    snapshot
        .entities
        .insert("history".to_string(), "[{\"changed\":true}]".to_string());

    assert!(matches!(
        validate_raw_snapshot(&snapshot),
        Err(PortableSnapshotError::PayloadHashMismatch)
    ));
}
