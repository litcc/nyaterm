//! Shared child-process ownership for external Agent CLIs.

use std::process::{Child, ChildStderr, ChildStdin, ChildStdout};

pub(super) struct ExternalAgentChild {
    child: Child,
}

impl ExternalAgentChild {
    pub fn new(child: Child) -> Self {
        Self { child }
    }

    pub fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }

    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin.take()
    }

    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr.take()
    }
}

impl Drop for ExternalAgentChild {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
