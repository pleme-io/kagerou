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

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::ProcessStart(a), Self::ProcessStart(b))
            | (Self::ProcessStop(a), Self::ProcessStop(b))
            | (Self::NetworkNotFound(a), Self::NetworkNotFound(b))
            | (Self::InvalidTopology(a), Self::InvalidTopology(b)) => a == b,
            (Self::ConsensusTimeout(a), Self::ConsensusTimeout(b)) => a == b,
            (Self::Io(a), Self::Io(b)) => a.kind() == b.kind(),
            (Self::Serialization(_), Self::Serialization(_)) => true,
            _ => false,
        }
    }
}

impl Error {
    /// Whether the error is transient and the operation could be retried.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::ConsensusTimeout(_) | Self::ProcessStart(_) | Self::Io(_)
        )
    }
}

/// Convenience result type.
pub type Result<T> = std::result::Result<T, Error>;

// ---------------------------------------------------------------------------
// Relay flags
// ---------------------------------------------------------------------------

/// Tor consensus relay flags as defined in the directory protocol specification.
///
/// Flags are assigned by directory authorities to characterize relay behaviour
/// and capabilities. A relay may carry multiple flags simultaneously.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelayFlag {
    /// Runs as a directory authority.
    Authority,
    /// Allowed to serve as an exit relay.
    Exit,
    /// Suitable for high-bandwidth circuits.
    Fast,
    /// Suitable as the first hop in a circuit.
    Guard,
    /// Participates in the hidden-service directory ring.
    HSDir,
    /// Has been running long enough to be considered stable.
    Stable,
    /// Currently reachable by the authorities.
    Running,
    /// Not known to be broken or misconfigured.
    Valid,
    /// Flagged as a bad exit by the authorities.
    BadExit,
    /// Restricted to middle position only.
    MiddleOnly,
}

impl std::fmt::Display for RelayFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authority => write!(f, "Authority"),
            Self::Exit => write!(f, "Exit"),
            Self::Fast => write!(f, "Fast"),
            Self::Guard => write!(f, "Guard"),
            Self::HSDir => write!(f, "HSDir"),
            Self::Stable => write!(f, "Stable"),
            Self::Running => write!(f, "Running"),
            Self::Valid => write!(f, "Valid"),
            Self::BadExit => write!(f, "BadExit"),
            Self::MiddleOnly => write!(f, "MiddleOnly"),
        }
    }
}

// ---------------------------------------------------------------------------
// Network health
// ---------------------------------------------------------------------------

/// Health status of a test network, modelled after chutney's `verify` stages.
///
/// Progresses from `Unknown` through bootstrapping phases to `Healthy`,
/// or falls back to `Degraded` / `Failed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NetworkHealth {
    /// Health has not been evaluated yet.
    #[default]
    Unknown,
    /// Nodes are starting up and exchanging descriptors.
    Bootstrapping,
    /// Directory authorities have published a consensus document.
    ConsensusReached,
    /// Circuits are being constructed through the network.
    CircuitsBuilding,
    /// All checks pass — the network is fully operational.
    Healthy,
    /// Some nodes are down or circuits are failing, but the network is usable.
    Degraded,
    /// The network cannot route traffic.
    Failed,
}

impl std::fmt::Display for NetworkHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => write!(f, "unknown"),
            Self::Bootstrapping => write!(f, "bootstrapping"),
            Self::ConsensusReached => write!(f, "consensus_reached"),
            Self::CircuitsBuilding => write!(f, "circuits_building"),
            Self::Healthy => write!(f, "healthy"),
            Self::Degraded => write!(f, "degraded"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl NetworkHealth {
    /// Whether the network can route traffic (either fully healthy or degraded).
    #[must_use]
    pub fn is_operational(&self) -> bool {
        matches!(self, Self::Healthy | Self::Degraded)
    }
}

// ---------------------------------------------------------------------------
// Time mode
// ---------------------------------------------------------------------------

/// Simulation time mode, inspired by Shadow's virtual-time scheduler.
///
/// `RealTime` uses wall-clock time (default). `Accelerated` runs the
/// simulation faster by the given factor. `Virtual` decouples the
/// simulation clock entirely from wall-clock time (future Shadow
/// integration).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TimeMode {
    /// Use wall-clock time (default behaviour).
    #[default]
    RealTime,
    /// Run faster than real time by the given integer factor.
    Accelerated {
        /// Speed-up multiplier (e.g. 10 means 10x faster).
        factor: u32,
    },
    /// Fully virtual time decoupled from the wall clock.
    Virtual,
}

impl std::fmt::Display for TimeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RealTime => write!(f, "real_time"),
            Self::Accelerated { factor } => write!(f, "accelerated({factor}x)"),
            Self::Virtual => write!(f, "virtual"),
        }
    }
}

// ---------------------------------------------------------------------------
// Consensus parameters
// ---------------------------------------------------------------------------

/// Tor consensus parameters extracted from a consensus document.
///
/// Mirrors the header fields and tunable parameters found in real Tor
/// consensus documents. `known_flags` lists the flags the authorities
/// agreed on; `params` holds the integer-valued tunables (e.g.
/// `CircuitBuildTimeout`, `NumDirectoryGuards`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ConsensusParams {
    /// When this consensus became valid (ISO 8601 timestamp).
    pub valid_after: Option<String>,
    /// When a fresh consensus should replace this one.
    pub fresh_until: Option<String>,
    /// When this consensus expires.
    pub valid_until: Option<String>,
    /// Relay flags known to this consensus.
    pub known_flags: Vec<RelayFlag>,
    /// Integer-valued consensus parameters (e.g. `CircuitBuildTimeout`).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub params: std::collections::BTreeMap<String, i64>,
}

// ---------------------------------------------------------------------------
// Topology
// ---------------------------------------------------------------------------

/// Describes the shape of a private Tor network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Consensus {
    /// When this consensus became valid.
    pub valid_after: String,
    /// When this consensus expires.
    pub valid_until: String,
    /// Number of relays listed in the consensus.
    pub relay_count: u32,
}

/// Summary status of a test network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

    #[test]
    fn topology_serialization_roundtrip() {
        let t = Topology::standard();
        let json = serde_json::to_string(&t).unwrap();
        let deserialized: Topology = serde_json::from_str(&json).unwrap();
        assert_eq!(t, deserialized);
    }

    #[test]
    fn consensus_serialization_roundtrip() {
        let c = Consensus {
            valid_after: "2026-01-01 00:00:00".into(),
            valid_until: "2026-01-01 01:00:00".into(),
            relay_count: 5,
        };
        let json = serde_json::to_string(&c).unwrap();
        let deserialized: Consensus = serde_json::from_str(&json).unwrap();
        assert_eq!(c, deserialized);
    }

    #[test]
    fn node_handle_serialization_roundtrip() {
        let h = NodeHandle {
            pid: 1234,
            role: NodeRole::Exit,
            nickname: "exit0".into(),
            or_port: 9001,
            dir_port: 0,
            control_port: 9051,
        };
        let json = serde_json::to_string(&h).unwrap();
        let deserialized: NodeHandle = serde_json::from_str(&json).unwrap();
        assert_eq!(h, deserialized);
    }

    #[test]
    fn node_role_display_all_variants() {
        assert_eq!(NodeRole::DirAuthority.to_string(), "DirAuthority");
        assert_eq!(NodeRole::Relay.to_string(), "Relay");
        assert_eq!(NodeRole::Exit.to_string(), "Exit");
        assert_eq!(NodeRole::Bridge.to_string(), "Bridge");
        assert_eq!(NodeRole::Client.to_string(), "Client");
    }

    #[test]
    fn error_display_variants() {
        assert_eq!(
            Error::ProcessStart("spawn failed".into()).to_string(),
            "failed to start tor process: spawn failed"
        );
        assert_eq!(
            Error::ProcessStop("kill failed".into()).to_string(),
            "failed to stop tor process: kill failed"
        );
        assert_eq!(
            Error::ConsensusTimeout(60).to_string(),
            "consensus timeout after 60 seconds"
        );
        assert_eq!(
            Error::NetworkNotFound("net-1".into()).to_string(),
            "network not found: net-1"
        );
        assert_eq!(
            Error::InvalidTopology("no auth".into()).to_string(),
            "invalid topology: no auth"
        );
    }

    #[test]
    fn error_partial_eq() {
        assert_eq!(
            Error::ProcessStart("a".into()),
            Error::ProcessStart("a".into())
        );
        assert_ne!(
            Error::ProcessStart("a".into()),
            Error::ProcessStop("a".into())
        );
        assert_eq!(
            Error::ConsensusTimeout(30),
            Error::ConsensusTimeout(30)
        );
        assert_ne!(
            Error::ConsensusTimeout(30),
            Error::ConsensusTimeout(60)
        );
    }

    #[test]
    fn error_is_retryable() {
        assert!(Error::ConsensusTimeout(60).is_retryable());
        assert!(Error::ProcessStart("failed".into()).is_retryable());
        assert!(!Error::NetworkNotFound("net-1".into()).is_retryable());
        assert!(!Error::InvalidTopology("bad".into()).is_retryable());
    }

    #[test]
    fn topology_eq() {
        assert_eq!(Topology::minimal(), Topology::minimal());
        assert_ne!(Topology::minimal(), Topology::standard());
    }

    // -- RelayFlag tests --------------------------------------------------

    #[test]
    fn relay_flag_display_all_variants() {
        assert_eq!(RelayFlag::Authority.to_string(), "Authority");
        assert_eq!(RelayFlag::Exit.to_string(), "Exit");
        assert_eq!(RelayFlag::Fast.to_string(), "Fast");
        assert_eq!(RelayFlag::Guard.to_string(), "Guard");
        assert_eq!(RelayFlag::HSDir.to_string(), "HSDir");
        assert_eq!(RelayFlag::Stable.to_string(), "Stable");
        assert_eq!(RelayFlag::Running.to_string(), "Running");
        assert_eq!(RelayFlag::Valid.to_string(), "Valid");
        assert_eq!(RelayFlag::BadExit.to_string(), "BadExit");
        assert_eq!(RelayFlag::MiddleOnly.to_string(), "MiddleOnly");
    }

    #[test]
    fn relay_flag_serialization_roundtrip() {
        let flags = vec![
            RelayFlag::Authority,
            RelayFlag::Exit,
            RelayFlag::Fast,
            RelayFlag::Guard,
            RelayFlag::HSDir,
            RelayFlag::Stable,
            RelayFlag::Running,
            RelayFlag::Valid,
            RelayFlag::BadExit,
            RelayFlag::MiddleOnly,
        ];
        let json = serde_json::to_string(&flags).unwrap();
        let deserialized: Vec<RelayFlag> = serde_json::from_str(&json).unwrap();
        assert_eq!(flags, deserialized);
    }

    #[test]
    fn relay_flag_hash_and_eq() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(RelayFlag::Guard);
        set.insert(RelayFlag::Guard);
        set.insert(RelayFlag::Exit);
        assert_eq!(set.len(), 2);
    }

    // -- NetworkHealth tests ----------------------------------------------

    #[test]
    fn network_health_default_is_unknown() {
        assert_eq!(NetworkHealth::default(), NetworkHealth::Unknown);
    }

    #[test]
    fn network_health_display() {
        assert_eq!(NetworkHealth::Unknown.to_string(), "unknown");
        assert_eq!(NetworkHealth::Bootstrapping.to_string(), "bootstrapping");
        assert_eq!(
            NetworkHealth::ConsensusReached.to_string(),
            "consensus_reached"
        );
        assert_eq!(NetworkHealth::CircuitsBuilding.to_string(), "circuits_building");
        assert_eq!(NetworkHealth::Healthy.to_string(), "healthy");
        assert_eq!(NetworkHealth::Degraded.to_string(), "degraded");
        assert_eq!(NetworkHealth::Failed.to_string(), "failed");
    }

    #[test]
    fn network_health_is_operational() {
        assert!(!NetworkHealth::Unknown.is_operational());
        assert!(!NetworkHealth::Bootstrapping.is_operational());
        assert!(!NetworkHealth::ConsensusReached.is_operational());
        assert!(!NetworkHealth::CircuitsBuilding.is_operational());
        assert!(NetworkHealth::Healthy.is_operational());
        assert!(NetworkHealth::Degraded.is_operational());
        assert!(!NetworkHealth::Failed.is_operational());
    }

    #[test]
    fn network_health_serialization_roundtrip() {
        let variants = vec![
            NetworkHealth::Unknown,
            NetworkHealth::Bootstrapping,
            NetworkHealth::ConsensusReached,
            NetworkHealth::CircuitsBuilding,
            NetworkHealth::Healthy,
            NetworkHealth::Degraded,
            NetworkHealth::Failed,
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let deserialized: NetworkHealth = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, deserialized);
        }
    }

    // -- TimeMode tests ---------------------------------------------------

    #[test]
    fn time_mode_default_is_real_time() {
        assert_eq!(TimeMode::default(), TimeMode::RealTime);
    }

    #[test]
    fn time_mode_display() {
        assert_eq!(TimeMode::RealTime.to_string(), "real_time");
        assert_eq!(
            TimeMode::Accelerated { factor: 10 }.to_string(),
            "accelerated(10x)"
        );
        assert_eq!(TimeMode::Virtual.to_string(), "virtual");
    }

    #[test]
    fn time_mode_serialization_roundtrip() {
        let modes = vec![
            TimeMode::RealTime,
            TimeMode::Accelerated { factor: 5 },
            TimeMode::Virtual,
        ];
        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let deserialized: TimeMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, deserialized);
        }
    }

    #[test]
    fn time_mode_eq() {
        assert_eq!(
            TimeMode::Accelerated { factor: 10 },
            TimeMode::Accelerated { factor: 10 }
        );
        assert_ne!(
            TimeMode::Accelerated { factor: 10 },
            TimeMode::Accelerated { factor: 20 }
        );
        assert_ne!(TimeMode::RealTime, TimeMode::Virtual);
    }

    // -- ConsensusParams tests --------------------------------------------

    #[test]
    fn consensus_params_default() {
        let params = ConsensusParams::default();
        assert!(params.valid_after.is_none());
        assert!(params.fresh_until.is_none());
        assert!(params.valid_until.is_none());
        assert!(params.known_flags.is_empty());
        assert!(params.params.is_empty());
    }

    #[test]
    fn consensus_params_serialization_roundtrip() {
        let mut tunable = std::collections::BTreeMap::new();
        tunable.insert("CircuitBuildTimeout".into(), 60);
        tunable.insert("NumDirectoryGuards".into(), 3);

        let params = ConsensusParams {
            valid_after: Some("2026-01-01 00:00:00".into()),
            fresh_until: Some("2026-01-01 00:30:00".into()),
            valid_until: Some("2026-01-01 01:00:00".into()),
            known_flags: vec![
                RelayFlag::Authority,
                RelayFlag::Exit,
                RelayFlag::Guard,
                RelayFlag::Running,
                RelayFlag::Valid,
            ],
            params: tunable,
        };

        let json = serde_json::to_string(&params).unwrap();
        let deserialized: ConsensusParams = serde_json::from_str(&json).unwrap();
        assert_eq!(params, deserialized);
    }

    #[test]
    fn consensus_params_empty_params_skipped_in_json() {
        let params = ConsensusParams {
            valid_after: Some("2026-01-01 00:00:00".into()),
            fresh_until: None,
            valid_until: None,
            known_flags: vec![],
            params: std::collections::BTreeMap::new(),
        };
        let json = serde_json::to_string(&params).unwrap();
        assert!(!json.contains("params"));
    }
}
