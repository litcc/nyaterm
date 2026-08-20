# NyaTerm Vendor Notes

Upstream: <https://github.com/longbridge/gpui-component>

Vendored version: `gpui-component` `0.5.2` at commit
`b1e78a515716b232a7d731cc092bdc25f3bfd787`, from upstream `main` on
2026-08-18. The previous NyaTerm snapshot was
`e1570bdc8fd2dc17d38cab09e74b1783bdf3b24b`.

Reason: NyaTerm uses gpui-component through the stable `nyaterm-ui` facade.
The complete upstream workspace is vendored, including the newer
`crates/base` and `crates/fps` members, while all GPUI types remain shared with
the sibling `vendor/zed` snapshot.

Local modifications and integration notes:

- Repointed `gpui`, `gpui_platform`, `gpui_web`, `gpui_macros`, and
  `reqwest_client` workspace dependencies to sibling paths under
  `vendor/zed`.
- Preserved the registry `zed-reqwest` dependency and feature strategy used by
  Zed's `reqwest_client` integration.
- Reapplied NyaTerm's segmented `TabBar` layout: the bar fills the available
  width and segmented tab wrappers use equal flexible widths without changing
  the public NyaTerm tab facade.
- Adapted the upstream Input/Textarea state split inside `nyaterm-ui`, keeping
  `NyaInput`, `NyaInputState::multi_line`, `NyaNumberInput`, selection, and
  other desktop call sites stable.
- Preserved ordinary-input focus when users click prefixes, suffixes, or the
  input shell instead of the text editing surface.
- Made the base dialog backdrop event wrapper fill the viewport so backdrop
  presses close the top dialog while continuing to block lower pointer events.
- Extended `ScrollbarMode::Hover` in `crates/base/src/scrollbar.rs` to reveal
  from anywhere in the scroll viewport, not only from the track strip. Upstream
  sets `hovered_axis` only when the pointer is inside the track bounds, so a
  hidden bar has to be aimed at blind. Added `ScrollbarStateInner::hovered_viewport`
  (fed by `HitboxId::is_hovered`, so an overlay does not reveal the bar behind
  it) and a `reveal_hover` predicate. Thumb styling still keys off track hover
  alone, and `Scrolling`/`Always` semantics are unchanged.
- Added `Theme::sync_scrollbar_theme` in `crates/ui/src/theme/mod.rs` and routed
  `Theme::change` and `Theme::set_scrollbar_mode` through a shared
  `scrollbar_theme(&Theme)` builder. `Scrollbar` reads the `gpui_base::Theme`
  projection, which upstream rebuilds only on a light/dark flip; NyaTerm assigns
  scrollbar mode and colors on every palette apply and needs to re-project.
- Removed upstream `.git` metadata after vendoring.

Validation performed on 2026-08-18:

- `cargo tree -i gpui` reports one active `gpui v0.2.2`, from
  `vendor/zed/crates/gpui`.
- `cargo check --workspace` passed on `x86_64-unknown-linux-gnu`.
- Package tests passed: `nyaterm-ui` (41), `nyaterm-desktop` (932, 4 ignored),
  `nyaterm-terminal-gpui` (127, 1 ignored), and `nyaterm-remote-desktop` (17).
- Input focus, dialog backdrop behavior, and segmented tab facade tests passed.
- `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets` passed. Clippy reported existing
  non-fatal workspace warnings.
- The installed Windows GNU Rust target could not complete because the host
  lacks `x86_64-w64-mingw32-gcc`; it stopped while building `aws-lc-sys`.
- No macOS Rust target or macOS host is installed, so macOS compilation and
  runtime behavior remain part of the platform release matrix.
- `scripts/check-architecture-boundaries.sh` is referenced by repository
  documentation but is not present in this checkout, so that command could
  not be run.

Validation performed on 2026-08-19 for the scrollbar reveal changes:

- `cargo test -p gpui-base` covers `reveal_hover` per mode and the fresh idle
  hold on viewport exit.
- `cargo test -p nyaterm-ui` asserts the Base projection follows
  `apply_component_theme`, including a palette change with no light/dark flip.
- `cargo check --workspace`, `cargo test --workspace`, and
  `cargo clippy --workspace --all-targets` run from the NyaTerm root.
- Hover reveal, overlay hit regions, and thumb drag verified by hand on
  Windows 11; GPUI hit testing is platform-specific, so macOS and Linux remain
  part of the platform release matrix.

## Upstream fork branch

These changes are maintained as a patch series on <https://github.com/nyakang/gpui-component>,
branch `nyaterm`, based on upstream `b1e78a515716b232a7d731cc092bdc25f3bfd787`. Branch head at the
time of writing: `a6f390f7aefd96c465b67ec4434c41e4aed4ef34`.

The branch carries the functional patches only. Vendoring artifacts (this note,
crates.io packaging files, retained lock files, and sibling-path dependency
repoints for this directory layout) are deliberately not on it, so a `diff`
between the branch and this directory should show only those.
