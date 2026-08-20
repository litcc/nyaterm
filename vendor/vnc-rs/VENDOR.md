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
