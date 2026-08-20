# NyaTerm vnc-rs fork

This directory vendors `vnc-rs` 0.5.3 from
<https://github.com/HsuJv/vnc-rs> at commit
`ab684d009d767c968af2f7559576334038623124`.

The upstream crate is dual licensed under MIT or Apache-2.0. The original
`LICENSE-MIT`, `LICENSE-APACHE`, Cargo package authors, and source-level
attribution are preserved.

NyaTerm vendors this crate to harden its network parser before application
integration. Local changes forbid unsafe Rust, replace network-controlled panic
and undefined-behavior paths with typed errors, add bounded protocol limits,
reduce queue sizes, add explicit security-selection policy, and add
deterministic handshake/parser regression tests.

This fork is linked only by `crates/nyaterm-vnc-helper`, the isolated helper
process that decodes VNC on behalf of the native GPUI remote desktop surface. It
must not be added as a dependency of any crate the application itself links: the
process boundary is what keeps this parser, and the `flate2`/`image` decoders it
pulls in, away from server-controlled bytes in the main process. Raw must remain
the required fallback. ZRLE/Tight support should only be advertised after the
corresponding decoder hardening tests and interoperability checks pass.

## Refresh procedure

1. Fetch the exact upstream revision recorded above.
2. Preserve both license files and upstream attribution.
3. Reapply the smallest reviewed hardening diff.
4. Run `cargo fmt --check`, `cargo check`, and debug/release `cargo test` using
   this manifest, then run the root NyaTerm workspace checks.
5. Update the revision and local-change notes here.

## Upstream fork branch

These changes are maintained as a patch series on <https://github.com/nyakang/vnc-rs>,
branch `nyaterm`, based on upstream `ab684d009d767c968af2f7559576334038623124`. Branch head at the
time of writing: `99a82f83b05e4fb62aeb94fda6aa356e0251a766`.

The branch carries the functional patches only. Vendoring artifacts (this note,
crates.io packaging files, retained lock files, and sibling-path dependency
repoints for this directory layout) are deliberately not on it, so a `diff`
between the branch and this directory should show only those.

Note the upstream revision recorded above already replaced the
`VncEncoding::from(u32)` transmute with a safe `match` (upstream commit
`3015219`), falling back to `Raw` for an unknown encoding. The local change on
top of that is `From` to `TryFrom` with a typed `InvalidEncoding` error, so an
unknown encoding is rejected instead of silently decoded as Raw.
