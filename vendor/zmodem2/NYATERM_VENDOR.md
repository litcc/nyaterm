# NyaTerm zmodem2 Vendor Note

- Upstream: `zmodem2` 0.5.0 from <https://codeberg.org/jarkko/zmodem2>.
- Source: crates.io package `zmodem2-0.5.0`.
- Local reason: `build.rs` probes `PATH` for the lrzsz `rz`/`sz` binaries and
  emits `cargo:warning=lrzsz not found` when they are absent. The `has_lrzsz`
  cfg it sets only gates two of this crate's own integration tests, and
  `vendor/zmodem2` is in the workspace `exclude` list, so NyaTerm never builds
  them. lrzsz is also not normally present on Windows or macOS developer
  machines, which made the warning fire on every `cargo build --workspace`.
- Patch: `build.rs` only. This is accurate again as of 2026-08-21: an unwired
  `ZfileManagementOption` / `Sender::set_file_options` addition had also been
  carried in `src/transmission.rs`, but the option was stored and never
  transmitted (`write_zfile` emits a fixed `&[0; 4]` for the ZFILE header
  flags) and nothing in NyaTerm called it, so it was removed here and on the
  fork branch. `src/transmission.rs` now matches upstream 0.5.0 exactly. The absent-lrzsz match arm is now silent; probing and
  the `has_lrzsz` / `ZMODEM_RZ_BIN` / `ZMODEM_SZ_BIN` emissions are untouched,
  so running `cargo test` inside `vendor/zmodem2` still gates those tests
  correctly on a machine that has lrzsz. No library source is modified.
- Validation: `cargo build --workspace` on Windows is free of the warning, and
  `cargo test -p nyaterm-transport` still reports `207 passed; 1 failed`, the
  single failure being the pre-existing `bash -n` shell-integration syntax
  check that is unrelated to ZMODEM. All ZMODEM transfer tests pass.

## Upstream fork branch

These changes are maintained as a patch series on <https://github.com/nyakang/zmodem2>,
branch `nyaterm`, based on upstream `9635055dbc74652765e4066d18e1cfaac880e58c`. Branch head at the
time of writing: `3e9129643205fccb27170e8f32db0d76068be6b5`.

The branch carries the functional patches only. Vendoring artifacts (this note,
crates.io packaging files, retained lock files, and sibling-path dependency
repoints for this directory layout) are deliberately not on it, so a `diff`
between the branch and this directory should show only those.
