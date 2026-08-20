# NyaTerm Vendor Notes

Upstream: <https://github.com/zed-industries/zed>

Vendored commit: `78712609912211332becb84cb7a667d1b7b23f78`, from
upstream `main` on 2026-08-18. The previous NyaTerm snapshot was
`4aad57fd1f002f9feeea2b7fb6229ccbcd576cb1`.

Reason: NyaTerm vendors the complete Zed workspace so `gpui`,
`gpui_platform`, `gpui_web`, `gpui_macros`, `reqwest_client`, and the
platform renderer crates remain on one coherent upstream snapshot. This update
also includes the current `gpui_apple` crate that replaced the older
macOS-specific layout.

Local modifications:

- Removed upstream `.git` metadata after vendoring.
- Preserved NyaTerm's `livekit.yaml` and `crates/collab/.env.toml` local
  configuration files.
- Reapplied NyaTerm's `DynamicTexture` API through GPUI assets, windows, and
  platform interfaces.
- Reapplied stride-aware BGRA8 dirty-region uploads to the DirectX, WGPU,
  Metal (`gpui_apple`), Linux headless, and test atlas backends.
- Kept Remote Desktop frame updates on the dynamic-texture path so a dirty
  region does not rebuild a `RenderImage` or clone the complete framebuffer.

Validation performed on 2026-08-18:

- `cargo tree -i gpui` reports one active `gpui v0.2.2`, from this snapshot.
- `cargo check --workspace` passed on `x86_64-unknown-linux-gnu`.
- `cargo test -p gpui strided_update_preserves_pixels_outside_the_dirty_rectangle`
  passed in this workspace. The test applies a strided 2x2 dirty upload to a
  4x4 BGRA texture and verifies pixels outside the dirty rectangle are
  unchanged.
- NyaTerm package tests passed: `nyaterm-ui` (41), `nyaterm-desktop` (932,
  4 ignored), `nyaterm-terminal-gpui` (127, 1 ignored), and
  `nyaterm-remote-desktop` (17).
- `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets` passed. Clippy reported existing
  non-fatal workspace warnings.
- The installed Windows GNU Rust target could not complete because the host
  lacks `x86_64-w64-mingw32-gcc`; it stopped while building `aws-lc-sys`.
- No macOS Rust target or macOS host is installed, so Metal compilation and
  runtime behavior remain part of the platform release matrix.
- `scripts/check-architecture-boundaries.sh` is referenced by repository
  documentation but is not present in this checkout, so that command could
  not be run.

## Upstream fork branch

These changes are maintained as a patch series on <https://github.com/nyakang/zed>,
branch `nyaterm`, based on upstream `78712609912211332becb84cb7a667d1b7b23f78`. Branch head at the
time of writing: `e83a00dc5daccb4ee2f077f4050e9315ea21f165`.

The branch carries the functional patches only. Vendoring artifacts (this note,
crates.io packaging files, retained lock files, and sibling-path dependency
repoints for this directory layout) are deliberately not on it, so a `diff`
between the branch and this directory should show only those.
