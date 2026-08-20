# NyaTerm russh-sftp vendor notes

This directory is based on `russh-sftp` 2.4.0 from
<https://github.com/AspectUnk/russh-sftp> at commit
`e145c1f7ece99f41f558949ef59731f2cd1a9dfe` and is used through the workspace
path dependency in the root `Cargo.toml`.

NyaTerm carries compatibility changes first developed in the Tauri codebase,
including raw-byte remote paths and server limit handling. The GPUI port also
includes the request lifecycle and file-handle fixes from NyaTerm Tauri commits
`20c1db67de736b1fd907f62692239476a3dc526c` and
`bd8e0f75c49a215ef515edc8994c406b49b4f37d`:

- pending requests own their timeout and remove themselves when cancelled;
- stream failures wake all pending requests instead of leaving them to time out;
- late replies do not terminate the SFTP packet handler;
- dropped file handles are closed by a tracked background request instead of
  the upstream untracked `close_nowait` behavior;
- the upstream `File::close` API waits for pending writes and the remote close,
  while file shutdown remains idempotent and clears the closed handle;
- close responses, failures and timeouts release handle accounting without
  underflow;
- high-level reads and writes explicitly close remote handles.

NyaTerm also retains the compatibility APIs used by its transport boundary:
`File::read_at`, `SftpSession::symlink_openssh`, and server-limit accessors.
For the saved-connection SFTP filename encoding parity work, NyaTerm extends
the raw/session/fs client APIs so remote path fields can be sent and received
as raw `Vec<u8>` values. The original `String` APIs remain available and
delegate through UTF-8 bytes for compatibility, while NyaTerm transport uses
the new bytes APIs to apply per-connection UTF-8/GBK/GB2312/GB18030 path
codecs before packets are serialized. Server-side path packets are bridged
back to the upstream `Handler` string interface with lossy UTF-8 conversion so
the vendored server API remains source-compatible.

The raw path bytes change is intentionally limited to SFTP protocol path
fields and high-level path operations (`open`, `opendir`, `stat`, `lstat`,
`setstat`, `realpath`, `readlink`, `rename`, `remove`, `mkdir`, `rmdir`, and
`symlink`). It does not change handle bytes, file contents, OpenSSH extension
payload schemas, or NyaTerm UI path joining semantics.

The vendor tests use the sibling patched `vendor/russh` path. The workspace
`Cargo.lock` is retained despite the upstream library ignore rule so vendored
validation resolves reproducibly. The upstream `.git` metadata is excluded
from the snapshot.

These changes prevent stalled writes and leaked server handles during uploads,
downloads, remote editing, cancellation, and error cleanup.

Validation on 2026-08-05:

```text
cargo test --manifest-path vendor/russh-sftp/Cargo.toml --lib  # 12 passed
cargo test -p nyaterm-transport                               # 147 passed
```

The SFTP service E2E test was skipped because `NYATERM_TEST_SFTP_*` and a
disposable remote directory were not configured.

Additional validation on 2026-08-09 after the raw path bytes API expansion:

```text
cargo test -p nyaterm-transport  # 160 passed, 1 ignored
cargo test -p nyaterm-desktop    # 819 passed, 3 ignored
```

## Upstream fork branch

These changes are maintained as a patch series on <https://github.com/nyakang/russh-sftp>,
branch `nyaterm`, based on upstream `e145c1f7ece99f41f558949ef59731f2cd1a9dfe`. Branch head at the
time of writing: `687c578b0199a16b0a326e4a5983f0f7471e2ac1`.

The branch carries the functional patches only. Vendoring artifacts (this note,
crates.io packaging files, retained lock files, and sibling-path dependency
repoints for this directory layout) are deliberately not on it, so a `diff`
between the branch and this directory should show only those.
