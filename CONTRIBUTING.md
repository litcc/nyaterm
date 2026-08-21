# Contributing to NyaTerm

## Before changing code

Read `AGENTS.md` and the relevant crate-level architecture in
`docs/architecture/gpui-migration-status.md`. Keep one authoritative owner for
each piece of state. Do not introduce WebView/Tauri layers, `#[path]` module
aliases, `use super::*`, snapshot-only stores, or direct database access from
GPUI views.

Choose the crate that owns the behavior:

- Pure models, parsing, compatibility formats, and policies belong in
  `nyaterm-core`.
- Database execution and compatibility readers belong in `nyaterm-store`.
- PTY, SSH, Telnet, Serial, SFTP, transfer, and tunnel runtime code belongs in
  `nyaterm-transport`.
- Terminal parsing and snapshots belong in `nyaterm-terminal`; GPUI layout and
  painting belong in `nyaterm-terminal-gpui`.
- GPUI state, views, and background coordination belong in
  `nyaterm-desktop`.
- Shared GPUI controls and theme integration belong in `nyaterm-ui`.

For a change that crosses crates, keep the boundary adapter small and document
which crate owns the resulting state. Do not put filesystem, database, network,
SSH, SFTP, subprocess, or image-decoding work in a render path or a long-running
GPUI update callback.

## Local checks

Run focused checks while working, then the full checks before review:

```bash
cargo check -p <crate-name>
cargo test -p <crate-name>
cargo check --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
bash scripts/check-architecture-boundaries.sh
bash scripts/check-icon-references.sh
```

Use `cargo run -p nyaterm-app --bin nyaterm` for a local graphical smoke test.
The full workspace and application checks may need platform-native GPUI, PTY,
serial, clipboard, and window-system dependencies. Do not add real credentials
to tests; use fixtures or ignored integration tests with environment variables.

Tests should live beside the behavior they cover. Storage, credentials,
encryption, backup, cloud-sync, known-host, and session changes must include
new-data round trips and representative legacy-data tests. Never log passwords,
private keys, OTP secrets, API keys, or unredacted terminal context.

## Pull requests

Use a Conventional Commit-style subject, for example
`fix(transport): handle closed SSH channels`. Describe behavior, architectural
ownership, commands and platforms tested, and any persistence, credential,
migration, or forked-dependency impact. Keep unrelated formatting or
generated metadata out of the change.

Keep pull requests focused and explain compatibility-sensitive decisions in the
description. Patched third-party dependencies are not vendored: each is a patch
series on a fork under <https://github.com/nyakang> on branch `nyaterm`, pinned
by revision in the root `Cargo.toml`. Change one by committing to its fork
branch and bumping that revision, and identify the upstream project/version or
commit, the reason for the modification, and the validation performed. `temp/vendor/`
holds untracked read-only copies for reading only; nothing there is compiled.
Update `docs/architecture/gpui-migration-status.md` when a migration boundary,
ownership rule, or debt count changes.

## Compatibility

Preserve existing table names, keys, serialized field names, document keys,
encryption prefixes, master-key wrapping, backup formats, and fallback
decryption behavior unless the change includes a tested migration path. Do not
overwrite user data until validation succeeds, and keep secret-bearing values
masked when returning settings to the UI.
