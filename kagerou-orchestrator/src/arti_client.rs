//! Arti client integration for connecting to kagerou test networks.
//!
//! Feature-gated behind `kakuremino`. Provides configuration types for
//! pointing an Arti-based client at a running kagerou test network's
//! custom directory authorities.

use std::path::PathBuf;

use kagerou_core::{Error, NodeRole, Result, TestNetwork};

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

/// A directory authority endpoint extracted from a test network.
#[derive(Debug, Clone)]
pub struct DirAuthority {
    /// Authority nickname.
    pub nickname: String,
    /// Bind address.
    pub address: String,
    /// OR (onion router) port.
    pub or_port: u16,
    /// Directory port.
    pub dir_port: u16,
    /// Identity fingerprint (may be empty if not yet bootstrapped).
    pub fingerprint: String,
}

/// Configuration for connecting an Arti client to a kagerou test network.
#[derive(Debug, Clone)]
pub struct ArtiTestConfig {
    /// Custom directory authorities extracted from the test network.
    pub dir_authorities: Vec<DirAuthority>,
    /// Data directory for Arti state.
    pub state_dir: PathBuf,
    /// Cache directory for Arti.
    pub cache_dir: PathBuf,
}

impl ArtiTestConfig {
    /// Build config from a running test network's node handles.
    ///
    /// Extracts directory authorities and derives Arti data/cache
    /// directories under the network's root data directory.
    #[must_use]
    pub fn from_test_network(network: &TestNetwork) -> Self {
        let dir_authorities = network
            .nodes
            .iter()
            .filter(|n| matches!(n.role, NodeRole::DirAuthority))
            .map(|n| DirAuthority {
                nickname: n.nickname.clone(),
                address: "127.0.0.1".into(),
                or_port: n.or_port,
                dir_port: n.dir_port,
                fingerprint: String::new(), // Would need real fingerprints from bootstrapped nodes
            })
            .collect();

        Self {
            dir_authorities,
            state_dir: network.data_dir.join("arti-state"),
            cache_dir: network.data_dir.join("arti-cache"),
        }
    }

    /// Number of configured directory authorities.
    #[must_use]
    pub fn authority_count(&self) -> usize {
        self.dir_authorities.len()
    }

    /// Validate that the config has the minimum requirements for operation.
    pub fn validate(&self) -> Result<()> {
        if self.dir_authorities.is_empty() {
            return Err(Error::InvalidTopology(
                "no directory authorities configured".into(),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use kagerou_core::{NodeHandle, NodeRole, TestNetwork, Topology};

    use super::*;

    /// Helper to build a test network with the given authority and relay counts.
    fn make_test_network(auth_count: u32, relay_count: u32) -> TestNetwork {
        let mut nodes = Vec::new();

        for i in 0..auth_count {
            nodes.push(NodeHandle {
                pid: 100 + i,
                role: NodeRole::DirAuthority,
                nickname: format!("auth{i}"),
                or_port: 5000 + i as u16,
                dir_port: 7000 + i as u16,
                control_port: 9000 + i as u16,
            });
        }

        for i in 0..relay_count {
            nodes.push(NodeHandle {
                pid: 200 + i,
                role: NodeRole::Relay,
                nickname: format!("relay{i}"),
                or_port: 6000 + i as u16,
                dir_port: 0,
                control_port: 9100 + i as u16,
            });
        }

        TestNetwork {
            id: "test-net-1".into(),
            topology: Topology {
                authority_count: auth_count,
                relay_count,
                exit_count: 0,
                bridge_count: 0,
                hs_count: 0,
            },
            data_dir: PathBuf::from("/tmp/kagerou/test-net-1"),
            nodes,
        }
    }

    #[test]
    fn config_from_test_network_extracts_authorities() {
        let network = make_test_network(3, 2);
        let config = ArtiTestConfig::from_test_network(&network);

        assert_eq!(config.authority_count(), 3);

        // Verify each authority was extracted correctly
        for (i, auth) in config.dir_authorities.iter().enumerate() {
            assert_eq!(auth.nickname, format!("auth{i}"));
            assert_eq!(auth.address, "127.0.0.1");
            assert_eq!(auth.or_port, 5000 + i as u16);
            assert_eq!(auth.dir_port, 7000 + i as u16);
        }

        // Relays should not appear in dir_authorities
        assert!(config
            .dir_authorities
            .iter()
            .all(|a| a.nickname.starts_with("auth")));
    }

    #[test]
    fn config_validate_empty_fails() {
        let config = ArtiTestConfig {
            dir_authorities: vec![],
            state_dir: PathBuf::from("/tmp/state"),
            cache_dir: PathBuf::from("/tmp/cache"),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn config_validate_with_authorities_passes() {
        let network = make_test_network(1, 0);
        let config = ArtiTestConfig::from_test_network(&network);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_authority_count() {
        let network = make_test_network(5, 3);
        let config = ArtiTestConfig::from_test_network(&network);
        assert_eq!(config.authority_count(), 5);
    }

    #[test]
    fn config_directories_derived_from_network() {
        let network = make_test_network(1, 0);
        let config = ArtiTestConfig::from_test_network(&network);

        assert_eq!(
            config.state_dir,
            PathBuf::from("/tmp/kagerou/test-net-1/arti-state")
        );
        assert_eq!(
            config.cache_dir,
            PathBuf::from("/tmp/kagerou/test-net-1/arti-cache")
        );
    }
}
