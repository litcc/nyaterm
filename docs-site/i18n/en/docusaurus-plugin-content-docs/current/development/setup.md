# Development Setup

The NyaTerm application is a Cargo workspace. Node.js and pnpm are only required for the Docusaurus documentation site in this repository.

## Application prerequisites

### Rust and Git

Install the latest stable Rust toolchain:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Windows users can install from [rustup.rs](https://rustup.rs/). Every platform also needs Git and its native compiler toolchain.

### Platform dependencies

#### Windows

Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the **Desktop development with C++** workload.

#### macOS

Install Xcode and its command-line tools:

```bash
xcode-select --install
```

GPUI uses Metal on macOS, so a working macOS SDK is required.

#### Linux (Ubuntu / Debian)

Install Rust crate build tools, font/window-system development libraries, and the Vulkan loader:

```bash
sudo apt update
sudo apt install build-essential clang pkg-config cmake \
  libfontconfig1-dev libfreetype6-dev libssl-dev libudev-dev \
  libwayland-dev libx11-dev libx11-xcb-dev \
  libxcb-cursor-dev libxcb-icccm4-dev libxcb-image0-dev \
  libxcb-keysyms1-dev libxcb-randr0-dev libxcb-render0-dev \
  libxcb-shape0-dev libxcb-xfixes0-dev libxcb-xinerama0-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libzstd-dev \
  libvulkan1 mesa-vulkan-drivers
```

Running the desktop application also requires a working Vulkan driver and an X11 or Wayland session.

## Clone the repository

```bash
git clone https://github.com/nyakang/nyaterm.git
cd nyaterm
```

Cargo manages all application dependencies. The Node.js dependencies in this repository belong to `docs-site` only.

## Run the application

```bash
cargo run -p nyaterm-app --bin nyaterm
```

The first build compiles GPUI and every dependency, so it takes noticeably longer than subsequent incremental builds.

### RDP / VNC need their helpers built first

RDP and VNC each run in a separate helper process that the application resolves beside its own executable. The command above **only builds the application**, so both protocols fail with `HelperMissing`. Build the helpers first:

```bash
cargo build -p nyaterm-rdp-helper -p nyaterm-vnc-helper
```

A bare `cargo build` or `cargo check` covers all three — they are the workspace `default-members`.

`NYATERM_RDP_HELPER` and `NYATERM_VNC_HELPER` override the lookup with an explicit path, which is handy for pointing at binaries in another target directory.

## Common checks

Prefer checks scoped to the affected crate while iterating:

```bash
cargo check -p nyaterm-app
cargo test -p <crate-name>
```

Run the relevant workspace checks before review:

```bash
cargo check --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
```

`cargo fmt --all` writes formatting changes; use it only when you intend to apply them.

## Release-profile build

```bash
cargo build -p nyaterm-app --bin nyaterm --release
```

The native binary is written to `target/release/nyaterm`, or `target/release/nyaterm.exe` on Windows. This command builds only the application binary — neither the helpers nor any installer.

Release packages come from `scripts/release/package_native.py`, which puts the helpers next to the application and produces `-setup.exe` / `_portable.zip`, `.dmg` / `.app.tar.gz`, and `.AppImage` / `.deb` / `.rpm` per platform. When you add a helper, its `HELPER_BINS` list must be updated too.

## Documentation development

When editing `docs-site`, also install Node.js 22.13+ and [pnpm](https://pnpm.io/). Docusaurus itself only needs Node 18+, but the pnpm version this repository pins imports `node:sqlite` and crashes outright on anything older. The `packageManager` field in `docs-site/package.json` pins that pnpm version, and corepack picks it up automatically.

```bash
pnpm --dir docs-site install
pnpm --dir docs-site start:zh
```

Start the English documentation server with:

```bash
pnpm --dir docs-site start:en
```

Build every locale with:

```bash
pnpm --dir docs-site build
```

The build checks pages and sidebars, and reports Markdown-link problems according to the site configuration.

What the build cannot catch is checked by a separate script:

```bash
python3 scripts/ci/check_docs_translations.py
```

It verifies that every page exists in both locales, that both copies have the same heading count (which catches a whole section going untranslated), that every page is referenced from `sidebars.ts`, and that no authoring notes were left in the published text. The `Documentation site` CI job runs this script and then builds every locale.

Note that the script compares heading counts, not translated wording.

## Changing third-party dependencies

The third-party dependencies NyaTerm patches are **not vendored into this repository**. Each is a patch series on a fork under [github.com/nyakang](https://github.com/nyakang) on branch `nyaterm`, consumed from a revision pinned in the root `Cargo.toml`: `alacritty`, `gpui-component`, `IronRDP`, `russh`, `russh-sftp`, `sspi-rs`, `vnc-rs`, `zed` (`gpui`), and `zmodem2`.

The workflow is: commit to the fork branch, push, then bump the pinned revision in the root `Cargo.toml`. Keep the patch series split by concern rather than squashed, and record the reason and the validation performed on the patch commit and in that branch's `NYATERM.md`. Prefer rebasing an existing series onto a newer upstream revision over accumulating snapshots.

`temp/vendor/` holds read-only local copies of those sources, kept only so they can be read locally. **Nothing there is compiled — editing it has no effect on the build and produces no error.**

## Development conventions

- Read the root `AGENTS.md` and `CONTRIBUTING.md` first.
- UI state and views live in `nyaterm-desktop`; shared controls live in `nyaterm-ui`.
- Transport, terminal, and core crates stay independent of GPUI.
- New UI text updates both locale files under `crates/nyaterm-desktop/src/i18n/locales/`.
- Never use real credentials in tests, logs, or diagnostic data.
