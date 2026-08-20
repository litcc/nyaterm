//! Process plumbing shared by the RDP and VNC helper session managers.
//!
//! Both protocols run their decoder in a child process and speak the typed IPC
//! packet protocol in [`crate::ipc`] over stdin/stdout. Only the control-message
//! vocabulary differs, so spawning, locating, and reaping the child lives here.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How long a helper is given to exit on its own before it is killed.
const GRACEFUL_CLOSE_TIMEOUT: Duration = Duration::from_millis(750);

/// A spawned helper with its pipes detached from the [`Child`].
pub(crate) struct HelperProcess {
    pub(crate) child: Child,
    pub(crate) stdin: ChildStdin,
    pub(crate) stdout: ChildStdout,
}

/// Locate a helper binary.
///
/// `env_var` overrides the search outright. Otherwise the helper must sit beside
/// the running executable: that is the layout every release package produces,
/// and `scripts/release/package_native.py` is what guarantees it.
pub(crate) fn resolve_helper(package: &str, env_var: &str) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(env_var).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(path);
        return path
            .is_file()
            .then_some(path)
            .ok_or_else(|| format!("{env_var} does not name an existing file"));
    }
    let executable = std::env::current_exe()
        .map_err(|error| format!("cannot resolve the NyaTerm executable: {error}"))?;
    let filename = if cfg!(windows) {
        format!("{package}.exe")
    } else {
        package.to_string()
    };
    let path = executable
        .parent()
        .map(|parent| parent.join(&filename))
        .ok_or_else(|| "the NyaTerm executable has no parent directory".to_string())?;
    if path.is_file() {
        Ok(path)
    } else {
        // Debug builds only produce the helper when its package is built too, so
        // name the command instead of leaving a bare missing-file error.
        Err(format!(
            "{filename} is missing beside NyaTerm ({}); build it with `cargo build -p {package}`",
            path.display()
        ))
    }
}

/// Spawn a helper with piped stdin/stdout and a discarded stderr.
pub(crate) fn spawn_helper(path: &Path) -> io::Result<HelperProcess> {
    let mut command = Command::new(path);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    configure_child(&mut command);
    let mut child = command.spawn()?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "helper stdin is unavailable"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "helper stdout is unavailable"))?;
    Ok(HelperProcess {
        child,
        stdin,
        stdout,
    })
}

/// Reap a helper, then join its reader thread.
///
/// The caller is expected to have already asked the helper to disconnect, so the
/// child normally exits within the grace period; killing is the fallback. Both
/// slots are cleared so a second call is a no-op.
pub(crate) fn cleanup_child(child: &mut Option<Child>, reader: &mut Option<JoinHandle<()>>) {
    if let Some(child) = child.as_mut() {
        let deadline = Instant::now() + GRACEFUL_CLOSE_TIMEOUT;
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(_) => break,
            }
        }
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
        }
        let _ = child.wait();
    }
    *child = None;
    if let Some(reader) = reader.take() {
        let _ = reader.join();
    }
}

#[cfg(windows)]
fn configure_child(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW: helpers are console subsystem binaries and would flash a
    // console window otherwise.
    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn configure_child(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::resolve_helper;

    #[test]
    fn env_override_must_name_an_existing_file() {
        let error = resolve_helper("nyaterm-rdp-helper", "NYATERM_TEST_HELPER_OVERRIDE_MISSING")
            .expect_err("no helper sits beside the test binary");
        // The env var is unset, so the search falls through to the sibling lookup
        // and must name the package to build.
        assert!(
            error.contains("cargo build -p nyaterm-rdp-helper"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn env_override_pointing_at_a_directory_is_rejected() {
        let variable = "NYATERM_TEST_HELPER_OVERRIDE_DIR";
        // SAFETY-adjacent: this test owns a uniquely named variable, so it does
        // not race other tests in the same process.
        unsafe { std::env::set_var(variable, env!("CARGO_MANIFEST_DIR")) };
        let error = resolve_helper("nyaterm-vnc-helper", variable)
            .expect_err("a directory is not an executable");
        unsafe { std::env::remove_var(variable) };
        assert_eq!(error, format!("{variable} does not name an existing file"));
    }
}
