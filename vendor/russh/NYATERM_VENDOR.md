# NyaTerm russh vendor notes

Upstream: <https://github.com/warp-tech/russh>

Vendored version: `russh` 0.62.5 (`v0.62.5`) at commit
`4882af71cf27ea5293636bf4985ef296dcf20896`.

NyaTerm uses this complete source snapshot through the root workspace path
dependency. Upstream `.git` metadata is excluded.

Local modification:

- SSH name-list decoding accepts exactly one trailing comma for compatibility
  with servers that emit it. Empty name-lists remain valid, while leading,
  middle, single-comma and multiple-empty entries remain rejected. Unit tests
  cover each accepted and rejected form.
- The vendor workspace `Cargo.lock` is retained despite the upstream library
  ignore rule so NyaTerm's vendored validation resolves reproducibly.

Validation on 2026-08-05:

```text
cargo test --manifest-path vendor/russh/Cargo.toml -p russh --lib  # 159 passed
```

## Upstream fork branch

These changes are maintained as a patch series on <https://github.com/nyakang/russh>,
branch `nyaterm`, based on upstream `4882af71cf27ea5293636bf4985ef296dcf20896`. Branch head at the
time of writing: `074d4eb594bf0ce1271725cf96e50ed4af8e3285`.

The branch carries the functional patches only. Vendoring artifacts (this note,
crates.io packaging files, retained lock files, and sibling-path dependency
repoints for this directory layout) are deliberately not on it, so a `diff`
between the branch and this directory should show only those.
