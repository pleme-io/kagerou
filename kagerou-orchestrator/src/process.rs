//! Tor process management.

use std::path::Path;

use kagerou_core::{Error, Result};
use tokio::process::Command;

/// A managed C-tor child process.
#[derive(Debug)]
pub struct TorProcess {
    child: tokio::process::Child,
}

impl TorProcess {
    /// Spawn a new `tor` process using the given torrc file.
    pub async fn spawn(torrc_path: &Path) -> Result<Self> {
        let child = Command::new("tor")
            .arg("-f")
            .arg(torrc_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| Error::ProcessStart(format!("failed to spawn tor: {e}")))?;

        Ok(Self { child })
    }

    /// Get the OS process ID, if available.
    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    /// Check whether the process is still running.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Kill the tor process.
    pub async fn kill(&mut self) -> Result<()> {
        self.child
            .kill()
            .await
            .map_err(|e| Error::ProcessStop(format!("failed to kill tor process: {e}")))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tor_process_struct_exists() {
        // Verify the TorProcess type is constructable (via internal fields)
        // We cannot easily construct without spawning, so this is a type-level check.
        fn _assert_debug<T: std::fmt::Debug>() {}
        _assert_debug::<TorProcess>();
    }

    #[tokio::test]
    async fn spawn_nonexistent_binary_returns_error() {
        // Use a path to a non-existent torrc — the `tor` binary likely isn't
        // installed in test environments, so we expect a ProcessStart error.
        let result = TorProcess::spawn(Path::new("/nonexistent/torrc")).await;
        assert!(result.is_err());
    }
}
