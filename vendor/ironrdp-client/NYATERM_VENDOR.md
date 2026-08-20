# NyaTerm IronRDP Client Vendor Note

- Upstream: `ironrdp-client` 0.1.0 from <https://github.com/Devolutions/IronRDP>
- Source: crates.io package `ironrdp-client-0.1.0`
- Local reason: NyaTerm needs dirty-region framebuffer output, explicit desktop reset/full-frame events, a non-reconnecting notification when Display Control is unavailable, a certificate decision hook between TLS certificate retrieval and CredSSP credential submission, and correct FastPath decompressor replacement after deactivation/reactivation. The upstream public API currently emits a full-screen allocation for every graphics update, reconnects to resize, does not expose that trust-decision boundary, and does not rebuild the negotiated bulk decompressor on reactivation.
- Scope: the patch changes public input/output/configuration events and their emission in the existing connection and active-session loops, plus the minimum negotiated-compression helper used by the reactivation loop. It does not fork the connector, CredSSP, graphics decoder, or active-stage protocol state machines.
- Validation: `cargo check --workspace`, `cargo test --workspace`, `cargo clippy --workspace --all-targets`, the architecture boundary script, and the helper lifecycle/clipboard tests. Cross-platform Windows/macOS validation is tracked separately because this repository environment is Linux.

Certificate policy and the headless text-only clipboard backend remain NyaTerm-owned helper concerns. Keep future changes minimal and rebase them onto an upstream release when equivalent hooks are available.

## Upstream fork branch

These changes are maintained as a patch series on <https://github.com/nyakang/IronRDP>,
branch `nyaterm`, based on upstream `11a0810cfbbabd8b8023875a05e3041216d4b01b`. Branch head at the
time of writing: `512a19d41cbf080eaff6ee1a5a5445a50a456249`.

The branch carries the functional patches only. Vendoring artifacts (this note,
crates.io packaging files, retained lock files, and sibling-path dependency
repoints for this directory layout) are deliberately not on it, so a `diff`
between the branch and this directory should show only those.

  Two differences between this snapshot and the fork branch are intentional. The
  branch calls `tracing::warn!` by full path in `new_fast_path_bulk_decompressor`,
  because this crate imports `warn` only under `#[cfg(feature = "clipboard")]`
  while that function is always compiled, so the form used here does not build
  unless that feature is enabled. The branch also uses
  `#[expect(dead_code, reason = ...)]` in the connector's `credssp.rs` where this
  snapshot uses `#[allow(dead_code)]`, which IronRDP's own
  `clippy::allow_attributes` lint reports. Additionally, `src/rdp.rs` here was
  reformatted at rustfmt's default `max_width = 100` while IronRDP sets 120; that
  reformatting is not part of the patch and is not carried on the branch.
