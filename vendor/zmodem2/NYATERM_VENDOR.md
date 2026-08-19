# NyaTerm zmodem2 Vendor Note

- Upstream: `zmodem2` 0.5.0 from <https://codeberg.org/jarkko/zmodem2>.
- Source: crates.io package `zmodem2-0.5.0`.
- Local reason: `build.rs` probes `PATH` for the lrzsz `rz`/`sz` binaries and
  emits `cargo:warning=lrzsz not found` when they are absent. The `has_lrzsz`
  cfg it sets only gates two of this crate's own integration tests, and
  `vendor/zmodem2` is in the workspace `exclude` list, so NyaTerm never builds
  them. lrzsz is also not normally present on Windows or macOS developer
  machines, which made the warning fire on every `cargo build --workspace`.
- Patch: `build.rs` only. The absent-lrzsz match arm is now silent; probing and
  the `has_lrzsz` / `ZMODEM_RZ_BIN` / `ZMODEM_SZ_BIN` emissions are untouched,
  so running `cargo test` inside `vendor/zmodem2` still gates those tests
  correctly on a machine that has lrzsz. No library source is modified.
- Validation: `cargo build --workspace` on Windows is free of the warning, and
  `cargo test -p nyaterm-transport` still reports `207 passed; 1 failed`, the
  single failure being the pre-existing `bash -n` shell-integration syntax
  check that is unrelated to ZMODEM. All ZMODEM transfer tests pass.
