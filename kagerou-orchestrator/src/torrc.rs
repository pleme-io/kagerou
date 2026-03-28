//! Torrc configuration file builder.

use std::path::{Path, PathBuf};

use kagerou_core::NodeRole;

/// Builder for generating torrc configuration files.
#[derive(Debug, Clone)]
pub struct TorrcBuilder {
    role: Option<NodeRole>,
    nickname: Option<String>,
    or_port: Option<u16>,
    dir_port: Option<u16>,
    control_port: Option<u16>,
    data_dir: Option<PathBuf>,
    testing_network: bool,
    authority_lines: Vec<String>,
    extra_lines: Vec<String>,
}

impl TorrcBuilder {
    /// Create a new empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            role: None,
            nickname: None,
            or_port: None,
            dir_port: None,
            control_port: None,
            data_dir: None,
            testing_network: false,
            authority_lines: Vec::new(),
            extra_lines: Vec::new(),
        }
    }

    /// Set the node role.
    #[must_use]
    pub fn set_role(mut self, role: NodeRole) -> Self {
        self.role = Some(role);
        self
    }

    /// Set the node nickname.
    #[must_use]
    pub fn set_nickname(mut self, name: impl Into<String>) -> Self {
        self.nickname = Some(name.into());
        self
    }

    /// Set the OR (onion router) port.
    #[must_use]
    pub fn set_or_port(mut self, port: u16) -> Self {
        self.or_port = Some(port);
        self
    }

    /// Set the directory port.
    #[must_use]
    pub fn set_dir_port(mut self, port: u16) -> Self {
        self.dir_port = Some(port);
        self
    }

    /// Set the control port.
    #[must_use]
    pub fn set_control_port(mut self, port: u16) -> Self {
        self.control_port = Some(port);
        self
    }

    /// Set the data directory for this node.
    #[must_use]
    pub fn set_data_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.data_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Enable `TestingTorNetwork 1` for private network operation.
    #[must_use]
    pub fn enable_testing_network(mut self) -> Self {
        self.testing_network = true;
        self
    }

    /// Add a `DirAuthority` line (required for all nodes in a test network).
    #[must_use]
    pub fn set_authority_line(mut self, line: impl Into<String>) -> Self {
        self.authority_lines.push(line.into());
        self
    }

    /// Add an arbitrary extra configuration line.
    #[must_use]
    pub fn add_extra_line(mut self, line: impl Into<String>) -> Self {
        self.extra_lines.push(line.into());
        self
    }

    /// Build the torrc configuration string.
    #[must_use]
    pub fn build(&self) -> String {
        let mut lines = Vec::new();

        // Testing network flag
        if self.testing_network {
            lines.push("TestingTorNetwork 1".to_owned());
        }

        // Nickname
        if let Some(ref nick) = self.nickname {
            lines.push(format!("Nickname {nick}"));
        }

        // Ports
        if let Some(port) = self.or_port {
            lines.push(format!("ORPort {port}"));
        }
        if let Some(port) = self.dir_port {
            lines.push(format!("DirPort {port}"));
        }
        if let Some(port) = self.control_port {
            lines.push(format!("ControlPort {port}"));
        }

        // Data directory
        if let Some(ref dir) = self.data_dir {
            lines.push(format!("DataDirectory {}", dir.display()));
        }

        // Role-specific directives
        if let Some(role) = self.role {
            match role {
                NodeRole::DirAuthority => {
                    lines.push("AuthoritativeDirectory 1".to_owned());
                    lines.push("V3AuthoritativeDirectory 1".to_owned());
                }
                NodeRole::Exit => {
                    lines.push("ExitRelay 1".to_owned());
                    lines.push("ExitPolicy accept *:*".to_owned());
                }
                NodeRole::Bridge => {
                    lines.push("BridgeRelay 1".to_owned());
                }
                NodeRole::Relay | NodeRole::Client => {}
            }
        }

        // Authority lines
        for auth_line in &self.authority_lines {
            lines.push(format!("DirAuthority {auth_line}"));
        }

        // Extra lines
        for extra in &self.extra_lines {
            lines.push(extra.clone());
        }

        // Logging
        lines.push("Log notice stdout".to_owned());
        lines.push("SafeLogging 0".to_owned());

        lines.join("\n") + "\n"
    }
}

impl Default for TorrcBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_testing_tor_network() {
        let torrc = TorrcBuilder::new().enable_testing_network().build();
        assert!(torrc.contains("TestingTorNetwork 1"));
    }

    #[test]
    fn sets_ports_correctly() {
        let torrc = TorrcBuilder::new()
            .set_or_port(9001)
            .set_dir_port(9030)
            .set_control_port(9051)
            .build();
        assert!(torrc.contains("ORPort 9001"));
        assert!(torrc.contains("DirPort 9030"));
        assert!(torrc.contains("ControlPort 9051"));
    }

    #[test]
    fn authority_line_present() {
        let torrc = TorrcBuilder::new()
            .set_authority_line("test-auth orport=9001 v3ident=ABCD 127.0.0.1:9030 FINGERPRINT")
            .build();
        assert!(torrc.contains("DirAuthority test-auth"));
    }

    #[test]
    fn data_dir_set() {
        let torrc = TorrcBuilder::new()
            .set_data_dir("/tmp/tor/node0")
            .build();
        assert!(torrc.contains("DataDirectory /tmp/tor/node0"));
    }

    #[test]
    fn dir_authority_role_directives() {
        let torrc = TorrcBuilder::new()
            .set_role(NodeRole::DirAuthority)
            .build();
        assert!(torrc.contains("AuthoritativeDirectory 1"));
        assert!(torrc.contains("V3AuthoritativeDirectory 1"));
    }

    #[test]
    fn exit_role_directives() {
        let torrc = TorrcBuilder::new().set_role(NodeRole::Exit).build();
        assert!(torrc.contains("ExitRelay 1"));
        assert!(torrc.contains("ExitPolicy accept *:*"));
    }

    #[test]
    fn nickname_included() {
        let torrc = TorrcBuilder::new().set_nickname("myrelay").build();
        assert!(torrc.contains("Nickname myrelay"));
    }
}
