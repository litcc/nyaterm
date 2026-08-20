# NyaTerm SSPI Vendor Note

- Upstream source: `sspi` 0.21.3 from <https://github.com/Devolutions/sspi-rs>, exposed as 0.21.0 to satisfy IronRDP 0.10's locked compatibility slot.
- Source: crates.io package `sspi-0.21.3`.
- Local reason: earlier 0.21 patch releases pin mutually incompatible prerelease RSA, Curve25519, Ed25519, and picky packages. The 0.21.3 source aligns RSA, and this manifest aligns picky plus the macOS Dalek crates with the stable dependency line used by NyaTerm.
- Patch: manifest only; no SSPI Rust source is modified (the sources here are
  byte-identical to the published `sspi 0.21.3`). Three changes: the package
  version is relabelled 0.21.3 to 0.21.0 so it satisfies
  `ironrdp-connector 0.10.0`'s `sspi = "=0.21.0"` requirement through
  `[patch.crates-io]`; the `picky` pin moves from `=7.0.0-rc.25` to
  `=7.0.0-rc.26`; and in the Apple-target dependency block the dalek crates move
  to the released `ed25519-dalek` 3.0.0 and `curve25519-dalek` 5.0.0 while
  upstream's `pkcs1`, `p256`, `primeorder`, `p384`, `p521`, `rustcrypto-ff`,
  `rustcrypto-ff_derive` and `rustcrypto-group` prerelease pins are dropped
  entirely. This crate does not use those directly and each exact pin is a hard
  conflict for a workspace that resolves them differently.
- Validation: workspace build on Linux plus required Windows/macOS helper build and NLA manual test before release.

## Upstream fork branch

These changes are maintained as a patch series on <https://github.com/nyakang/sspi-rs>,
branch `nyaterm`, based on upstream `09088ac49cf13449656dca94b68f5228919a4d95`. Branch head at the
time of writing: `878f55fd05257c86e9326b06f05735321f3f2352`.

The branch carries the functional patches only. Vendoring artifacts (this note,
crates.io packaging files, retained lock files, and sibling-path dependency
repoints for this directory layout) are deliberately not on it, so a `diff`
between the branch and this directory should show only those.
