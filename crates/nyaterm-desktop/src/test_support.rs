use std::path::{Path, PathBuf};

/// Removes a test's isolated configuration tree after its owners have shut down.
///
/// Bind this guard before the GPUI test context. Rust drops locals in reverse
/// order, which closes database and log handles before cleanup runs on Windows.
pub(crate) struct TestConfigDir {
    path: PathBuf,
}

impl TestConfigDir {
    pub(crate) fn new(prefix: &str) -> Self {
        Self {
            path: std::env::temp_dir().join(format!(
                "{prefix}-{}-{}",
                std::process::id(),
                nyaterm_core::uuid()
            )),
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestConfigDir {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!(
                "failed to remove test configuration directory {}: {error}",
                self.path.display()
            );
        }
    }
}

#[test]
fn test_config_dir_removes_its_tree_on_drop() {
    let path = {
        let dir = TestConfigDir::new("nyaterm-test-cleanup");
        std::fs::create_dir_all(dir.path().join("config")).expect("create test tree");
        dir.path().to_path_buf()
    };

    assert!(!path.exists(), "temporary test tree was not removed");
}
