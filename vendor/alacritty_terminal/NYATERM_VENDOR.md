# NyaTerm vendoring notes

- Upstream: [`alacritty_terminal`](https://github.com/alacritty/alacritty), crates.io 0.26.0.
- Upstream revision: `94e7c8874e526b1e67b349d9ba30ddf81669119e` (from
  `.cargo_vcs_info.json`), the commit titled "Alacritty version 0.17.0" that
  0.26.0 was published from. Note it is not an ancestor of upstream `master`.
- License: Apache-2.0; the upstream `LICENSE-APACHE` is retained.

## Local patch

NyaTerm adds a wrapping `u64` epoch to each grid. It advances by the number of rows
rotated only when upward scrolling starts at row zero. Read-only `Term` accessors expose
the stable primary/alternate epochs, screen generations, and RIS reset generation.
Entering the alternate screen advances its generation because Alacritty clears that
screen before swapping it into use. No ANSI, grid rotation, event, or parsing behavior is
changed.

`Grid::history_size()` cannot serve this purpose: once scrollback reaches its configured
limit, Alacritty keeps rotating the ring buffer while the reported history size remains
constant. Presentation metadata keyed to physical lines would consequently stop moving.

The crates.io package's 46 MiB reference-terminal fixtures are omitted. They are upstream
integration fixtures rather than library sources; NyaTerm retains and runs the upstream
unit tests embedded under `src/` and adds focused epoch/generation coverage there.

## Validation

```sh
cargo test --manifest-path vendor/alacritty_terminal/Cargo.toml
cargo test -p nyaterm-terminal
cargo check --workspace
```

## Upstream fork branch

These changes are maintained as a patch series on <https://github.com/nyakang/alacritty>,
branch `nyaterm`, based on upstream `94e7c8874e526b1e67b349d9ba30ddf81669119e`. Branch head at the
time of writing: `e7561331aa6e84df5dff9e1feb9433406fa0a4f9`.

The branch carries the functional patches only. Vendoring artifacts (this note,
crates.io packaging files, retained lock files, and sibling-path dependency
repoints for this directory layout) are deliberately not on it, so a `diff`
between the branch and this directory should show only those.
