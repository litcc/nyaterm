# NyaTerm IronRDP Connector Vendor Note

- Upstream: `ironrdp-connector` 0.10.0 from the IronRDP 0.17 release family.
- Source: crates.io package `ironrdp-connector-0.10.0`.
- Local reason: upstream reaches `picky` only for `picky::key::PrivateKey` on the
  smart-card credential path, but its unrestricted `picky` dependency enables the
  default JOSE and PKCS12 features. Those unused defaults pin
  `aes-gcm 0.11.0-rc.4`, which cannot coexist in this workspace with NyaTerm's
  stable `aes-gcm 0.11.0` persistence dependency.
- Patch: disable SSPI's unused `scard` feature, make the connector's otherwise
  unreachable smart-card credential branch return an error, and drop the `picky`
  dependency. Phase one explicitly excludes smart-card
  authentication/redirection; username/password NLA is unchanged.
  - With the smart-card branch gone the connector no longer names `picky` at all
    (the separate `picky-asn1-der` and `picky-asn1-x509` crates are still used),
    so the dependency only tripped upstream's own `unused_crate_dependencies`
    lint. `picky` stays in the graph through vendored SSPI, which already pins
    `=7.0.0-rc.26` with `default-features = false`, so the prerelease `aes-gcm`
    conflict remains avoided and the resolved feature set merely loses the
    unused `x509`.
  - Removed a stale `#[expect(single_use_lifetimes)]` on `create_gcc_blocks` in
    `src/connection.rs`. Rust 1.97 does not fire `single_use_lifetimes` for a
    lifetime used in an argument-position `impl Trait` bound, so the expectation
    was unfulfilled and warned on every build. Upstream's
    `[lints.clippy] allow_attributes` rules out swapping it for `#[allow]`.
- Validation: `cargo check -p ironrdp-connector` and `cargo build --workspace`
  are warning-free on Windows with rustc 1.97.1, `cargo test -p
  nyaterm-remote-desktop -p nyaterm-rdp-helper` passes, and `cargo tree` still
  resolves a single `picky 7.0.0-rc.26` alongside a single stable
  `aes-gcm 0.11.0`. Exercise NLA against the manual Windows matrix before
  release.

## Upstream fork branch

These changes are maintained as a patch series on <https://github.com/nyakang/IronRDP>,
branch `nyaterm`, based on upstream `11a0810cfbbabd8b8023875a05e3041216d4b01b`. Branch head at the
time of writing: `512a19d41cbf080eaff6ee1a5a5445a50a456249`.

The branch carries the functional patches only. Vendoring artifacts (this note,
crates.io packaging files, retained lock files, and sibling-path dependency
repoints for this directory layout) are deliberately not on it, so a `diff`
between the branch and this directory should show only those.
