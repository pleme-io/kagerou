//! Core types and traits for Tor testnet orchestration.
//!
//! Defines the foundational abstractions for creating and managing
//! private Tor networks: topologies, node configuration, consensus,
//! and the orchestrator/manager trait interfaces.

use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during network orchestration.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failed to start a Tor process.
    #[error("failed to start tor process: {0}")]
    ProcessStart(String),

    /// Failed to stop a Tor process.
    #[error("failed to stop tor process: {0}")]
    ProcessStop(String),

    /// Consensus was not reached within the timeout.
    #[error("consensus timeout after {0} seconds")]
    ConsensusTimeout(u64),

    /// Network not found.
    #[error("network not found: {0}")]
    NetworkNotFound(String),

    /// Invalid topology configuration.
    #[error("invalid topology: {0}")]
    InvalidTopology(String),

    /// I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Convenience result type.
pub type Result<T> = std::result::Result<T, Error>;

// ---------------------------------------------------------------------------
// Topology
// ---------------------------------------------------------------------------

/// Describes the shape of a private Tor network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topology {
    /// Number of directory authorities.
    pub authority_count: u32,
    /// Number of middle relays.
    pub relay_count: u32,
    /// Number of exit relays.
    pub exit_count: u32,
    /// Number of bridge relays.
    pub bridge_count: u32,
    /// Number of hidden-service directories.
    pub hs_count: u32,
}

impl Topology {
    /// A minimal topology suitable for quick tests: 3 authorities, 1 relay, 1 exit.
    #[must_use]
    pub fn minimal() -> Self {
        Self {
            authority_count: 3,
            relay_count: 1,
            exit_count: 1,
            bridge_count: 0,
            hs_count: 0,
        }
    }

    /// A standard topology for more realistic testing: 3 authorities, 3 relays, 2 exits.
    #[must_use]
    pub fn standard() -> Self {
        Self {
            authority_count: 3,
            relay_count: 3,
            exit_count: 2,
            bridge_count: 0,
            hs_count: 0,
        }
    }

    /// Total number of nodes described by this topology.
    #[must_use]
    pub fn total_nodes(&self) -> u32 {
        self.authority_count + self.relay_count + self.exit_count + self.bridge_count + self.hs_count
    }

    /// Validate the topology.
    pub fn validate(&self) -> Result<()> {
        if self.authority_count == 0 {
            return Err(Error::InvalidTopology(
                "at least one directory authority is required".into(),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Node types
// ---------------------------------------------------------------------------

/// The role a node plays in the test network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeRole {
    /// Directory authority — votes on consensus.
    DirAuthority,
    /// Middle relay.
    Relay,
    /// Exit relay.
    Exit,
    /// Bridge relay.
    Bridge,
    /// Client node (hidden service or plain client).
    Client,
}

impl std::fmt::Display for NodeRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DirAuthority => write!(f, "DirAuthority"),
            Self::Relay => write!(f, "Relay"),
            Self::Exit => write!(f, "Exit"),
            Self::Bridge => write!(f, "Bridge"),
            Self::Client => write!(f, "Client"),
        }
    }
}

/// Configuration for a single Tor node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Role of this node.
    pub role: NodeRole,
    /// Nickname (used in torrc and logs).
    pub nickname: String,
    /// OR (onion router) listening port.
    pub or_port: u16,
    /// Directory port (authorities and relays).
    pub dir_port: u16,
    /// Control port for the Tor control protocol.
    pub control_port: u16,
    /// Data directory for this node.
    pub data_dir: PathBuf,
}

/// Handle to a running Tor node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHandle {
    /// OS process ID.
    pub pid: u32,
    /// Role of the running node.
    pub role: NodeRole,
    /// Nickname.
    pub nickname: String,
    /// Active ports (OR, Dir, Control).
    pub or_port: u16,
    /// Directory port.
    pub dir_port: u16,
    /// Control port.
    pub control_port: u16,
}

// ---------------------------------------------------------------------------
// Network types
// ---------------------------------------------------------------------------

/// A running test network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestNetwork {
    /// Unique identifier for this network instance.
    pub id: String,
    /// The topology this network was created with.
    pub topology: Topology,
    /// Root data directory holding all node subdirectories.
    pub data_dir: PathBuf,
    /// Handles to running nodes.
    pub nodes: Vec<NodeHandle>,
}

/// Tor consensus document metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Consensus {
    /// When this consensus became valid.
    pub valid_after: String,
    /// When this consensus expires.
    pub valid_until: String,
    /// Number of relays listed in the consensus.
    pub relay_count: u32,
}

/// Summary status of a test network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatus {
    /// Whether the network is currently running.
    pub running: bool,
    /// Number of nodes in the network.
    pub node_count: u32,
    /// Whether the latest consensus is valid.
    pub consensus_valid: bool,
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// Manages the lifecycle of an entire test network.
#[async_trait]
pub trait NetworkOrchestrator: Send + Sync {
    /// Create a new test network with the given topology.
    async fn create_network(&self, topology: &Topology) -> Result<TestNetwork>;

    /// Tear down a running test network.
    async fn destroy_network(&self, network_id: &str) -> Result<()>;

    /// Block until the network reaches consensus or a timeout occurs.
    async fn wait_for_consensus(&self, network_id: &str, timeout_secs: u64) -> Result<Consensus>;

    /// Query the current status of a network.
    async fn network_status(&self, network_id: &str) -> Result<NetworkStatus>;
}

/// Manages individual Tor node processes.
#[async_trait]
pub trait NodeManager: Send + Sync {
    /// Start a Tor node with the given configuration.
    async fn start_node(&self, config: &NodeConfig) -> Result<NodeHandle>;

    /// Stop a running node.
    async fn stop_node(&self, pid: u32) -> Result<()>;

    /// Check whether a node is still running.
    async fn node_status(&self, pid: u32) -> Result<bool>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_topology_values() {
        let t = Topology::minimal();
        assert_eq!(t.authority_count, 3);
        assert_eq!(t.relay_count, 1);
        assert_eq!(t.exit_count, 1);
        assert_eq!(t.bridge_count, 0);
        assert_eq!(t.hs_count, 0);
        assert_eq!(t.total_nodes(), 5);
    }

    #[test]
    fn standard_topology_values() {
        let t = Topology::standard();
        assert_eq!(t.authority_count, 3);
        assert_eq!(t.relay_count, 3);
        assert_eq!(t.exit_count, 2);
        assert_eq!(t.total_nodes(), 8);
    }

    #[test]
    fn topology_validation_requires_authority() {
        let t = Topology {
            authority_count: 0,
            relay_count: 1,
            exit_count: 1,
            bridge_count: 0,
            hs_count: 0,
        };
        assert!(t.validate().is_err());
    }

    #[test]
    fn topology_validation_passes_minimal() {
        let t = Topology::minimal();
        assert!(t.validate().is_ok());
    }

    #[test]
    fn node_role_display() {
        assert_eq!(NodeRole::DirAuthority.to_string(), "DirAuthority");
        assert_eq!(NodeRole::Exit.to_string(), "Exit");
    }

    #[test]
    fn node_config_serialization_roundtrip() {
        let config = NodeConfig {
            role: NodeRole::Relay,
            nickname: "test_relay".into(),
            or_port: 9001,
            dir_port: 9030,
            control_port: 9051,
            data_dir: PathBuf::from("/tmp/tor/relay"),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: NodeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.nickname, "test_relay");
        assert_eq!(deserialized.or_port, 9001);
    }

    #[test]
    fn network_status_serialization() {
        let status = NetworkStatus {
            running: true,
            node_count: 5,
            consensus_valid: false,
        };
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: NetworkStatus = serde_json::from_str(&json).unwrap();
        assert!(deserialized.running);
        assert_eq!(deserialized.node_count, 5);
        assert!(!deserialized.consensus_valid);
    }
}
