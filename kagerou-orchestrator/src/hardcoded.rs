//! In-process orchestrator using synthetic consensus (no C-tor needed).
//!
//! Generates a valid consensus document from a topology without spawning
//! any real Tor processes. This enables pure-Rust testing of network
//! scenarios and fast CI pipelines.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use kagerou_core::{
    Consensus, Error, NetworkOrchestrator, NetworkStatus, NodeHandle, NodeRole, Result, TestNetwork,
    Topology,
};
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Synthetic consensus types
// ---------------------------------------------------------------------------

/// A synthetic consensus document for testing.
#[derive(Debug, Clone)]
pub struct SyntheticConsensus {
    /// When this consensus became valid.
    pub valid_after: String,
    /// When this consensus expires.
    pub valid_until: String,
    /// Relays described by this consensus.
    pub relays: Vec<SyntheticRelay>,
}

/// A single relay entry in a synthetic consensus.
#[derive(Debug, Clone)]
pub struct SyntheticRelay {
    /// Relay nickname.
    pub nickname: String,
    /// Hex-encoded 20-byte fingerprint.
    pub fingerprint: String,
    /// Bind address.
    pub address: String,
    /// OR (onion router) port.
    pub or_port: u16,
    /// Directory port (0 for non-authority relays).
    pub dir_port: u16,
    /// Consensus flags.
    pub flags: Vec<String>,
    /// Advertised bandwidth in bytes/s.
    pub bandwidth: u64,
}

impl SyntheticConsensus {
    /// Generate a consensus matching the given topology.
    #[must_use]
    pub fn from_topology(topology: &Topology) -> Self {
        let mut relays = Vec::new();
        let mut idx = 0u32;

        // Directory authorities
        for i in 0..topology.authority_count {
            relays.push(SyntheticRelay {
                nickname: format!("auth{i}"),
                fingerprint: generate_fingerprint(idx),
                address: format!("127.0.0.{}", idx + 1),
                or_port: 5000 + idx as u16,
                dir_port: 7000 + idx as u16,
                flags: vec![
                    "Authority".into(),
                    "V2Dir".into(),
                    "Valid".into(),
                    "Running".into(),
                ],
                bandwidth: 1000,
            });
            idx += 1;
        }

        // Middle relays
        for i in 0..topology.relay_count {
            relays.push(SyntheticRelay {
                nickname: format!("relay{i}"),
                fingerprint: generate_fingerprint(idx),
                address: format!("127.0.0.{}", idx + 1),
                or_port: 5000 + idx as u16,
                dir_port: 0,
                flags: vec![
                    "Fast".into(),
                    "Guard".into(),
                    "Stable".into(),
                    "Valid".into(),
                    "Running".into(),
                ],
                bandwidth: 5000,
            });
            idx += 1;
        }

        // Exit relays
        for i in 0..topology.exit_count {
            relays.push(SyntheticRelay {
                nickname: format!("exit{i}"),
                fingerprint: generate_fingerprint(idx),
                address: format!("127.0.0.{}", idx + 1),
                or_port: 5000 + idx as u16,
                dir_port: 0,
                flags: vec![
                    "Exit".into(),
                    "Fast".into(),
                    "Stable".into(),
                    "Valid".into(),
                    "Running".into(),
                ],
                bandwidth: 3000,
            });
            idx += 1;
        }

        // Bridges
        for i in 0..topology.bridge_count {
            relays.push(SyntheticRelay {
                nickname: format!("bridge{i}"),
                fingerprint: generate_fingerprint(idx),
                address: format!("127.0.0.{}", idx + 1),
                or_port: 5000 + idx as u16,
                dir_port: 0,
                flags: vec!["Valid".into(), "Running".into()],
                bandwidth: 2000,
            });
            idx += 1;
        }

        Self {
            valid_after: "2026-01-01 00:00:00".into(),
            valid_until: "2026-01-01 01:00:00".into(),
            relays,
        }
    }

    /// Number of relays in this consensus.
    #[must_use]
    pub fn relay_count(&self) -> u32 {
        self.relays.len() as u32
    }

    /// Convert to the core [`Consensus`] type.
    #[must_use]
    pub fn to_consensus(&self) -> Consensus {
        Consensus {
            valid_after: self.valid_after.clone(),
            valid_until: self.valid_until.clone(),
            relay_count: self.relay_count(),
        }
    }
}

/// Generate a deterministic hex fingerprint from an index.
///
/// Produces a 40-character hex string (20 bytes) by repeating the
/// big-endian encoding of `idx` five times.
fn generate_fingerprint(idx: u32) -> String {
    let bytes = idx.to_be_bytes();
    let mut fp = String::with_capacity(40);
    for _ in 0..5 {
        for b in &bytes {
            write!(fp, "{b:02X}").unwrap();
        }
    }
    fp
}

// ---------------------------------------------------------------------------
// In-process orchestrator
// ---------------------------------------------------------------------------

/// In-process orchestrator using synthetic consensus (no C-tor needed).
///
/// All network state lives in memory. [`wait_for_consensus`] returns
/// immediately because the consensus is pre-built from the topology.
#[derive(Debug)]
pub struct InProcessOrchestrator {
    networks: Arc<RwLock<HashMap<String, (Topology, SyntheticConsensus)>>>,
}

impl InProcessOrchestrator {
    /// Create a new in-process orchestrator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            networks: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InProcessOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NetworkOrchestrator for InProcessOrchestrator {
    async fn create_network(&self, topology: &Topology) -> Result<TestNetwork> {
        topology.validate()?;

        let consensus = SyntheticConsensus::from_topology(topology);

        let network_id = format!(
            "synth-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );

        // Build synthetic node handles (pid = 0 since no real processes)
        let mut nodes = Vec::new();
        for relay in &consensus.relays {
            let role = if relay.flags.contains(&"Authority".to_owned()) {
                NodeRole::DirAuthority
            } else if relay.flags.contains(&"Exit".to_owned()) {
                NodeRole::Exit
            } else if relay.nickname.starts_with("bridge") {
                NodeRole::Bridge
            } else {
                NodeRole::Relay
            };

            nodes.push(NodeHandle {
                pid: 0,
                role,
                nickname: relay.nickname.clone(),
                or_port: relay.or_port,
                dir_port: relay.dir_port,
                control_port: 0,
            });
        }

        let test_network = TestNetwork {
            id: network_id.clone(),
            topology: topology.clone(),
            data_dir: PathBuf::from("/dev/null"),
            nodes,
        };

        self.networks
            .write()
            .await
            .insert(network_id, (topology.clone(), consensus));

        Ok(test_network)
    }

    async fn destroy_network(&self, network_id: &str) -> Result<()> {
        self.networks
            .write()
            .await
            .remove(network_id)
            .ok_or_else(|| Error::NetworkNotFound(network_id.to_owned()))?;

        Ok(())
    }

    async fn wait_for_consensus(&self, network_id: &str, _timeout_secs: u64) -> Result<Consensus> {
        let networks = self.networks.read().await;
        let (_topology, consensus) = networks
            .get(network_id)
            .ok_or_else(|| Error::NetworkNotFound(network_id.to_owned()))?;

        Ok(consensus.to_consensus())
    }

    async fn network_status(&self, network_id: &str) -> Result<NetworkStatus> {
        let networks = self.networks.read().await;
        let (topology, _consensus) = networks
            .get(network_id)
            .ok_or_else(|| Error::NetworkNotFound(network_id.to_owned()))?;

        Ok(NetworkStatus {
            running: true,
            node_count: topology.total_nodes(),
            consensus_valid: true,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_consensus_minimal_topology() {
        let topology = Topology::minimal(); // 3 auth + 1 relay + 1 exit
        let consensus = SyntheticConsensus::from_topology(&topology);
        assert_eq!(consensus.relays.len(), 5);

        // First 3 are authorities
        for relay in &consensus.relays[..3] {
            assert!(relay.flags.contains(&"Authority".to_owned()));
            assert!(relay.nickname.starts_with("auth"));
        }
        // Next is a relay
        assert_eq!(consensus.relays[3].nickname, "relay0");
        // Last is an exit
        assert_eq!(consensus.relays[4].nickname, "exit0");
        assert!(consensus.relays[4].flags.contains(&"Exit".to_owned()));
    }

    #[test]
    fn synthetic_consensus_relay_count_matches() {
        let topology = Topology::standard(); // 3 + 3 + 2 = 8
        let consensus = SyntheticConsensus::from_topology(&topology);
        assert_eq!(consensus.relay_count(), 8);
        assert_eq!(consensus.to_consensus().relay_count, 8);
    }

    #[test]
    fn fingerprint_uniqueness() {
        let topology = Topology {
            authority_count: 3,
            relay_count: 5,
            exit_count: 3,
            bridge_count: 2,
            hs_count: 0,
        };
        let consensus = SyntheticConsensus::from_topology(&topology);
        let fingerprints: Vec<&str> = consensus.relays.iter().map(|r| r.fingerprint.as_str()).collect();

        // All fingerprints should be unique
        let mut unique = fingerprints.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(fingerprints.len(), unique.len());

        // Each fingerprint should be 40 hex characters
        for fp in &fingerprints {
            assert_eq!(fp.len(), 40);
            assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn synthetic_consensus_with_bridges() {
        let topology = Topology {
            authority_count: 1,
            relay_count: 0,
            exit_count: 0,
            bridge_count: 2,
            hs_count: 0,
        };
        let consensus = SyntheticConsensus::from_topology(&topology);
        assert_eq!(consensus.relay_count(), 3); // 1 auth + 2 bridges

        let bridges: Vec<_> = consensus
            .relays
            .iter()
            .filter(|r| r.nickname.starts_with("bridge"))
            .collect();
        assert_eq!(bridges.len(), 2);
        for bridge in &bridges {
            assert!(bridge.flags.contains(&"Valid".to_owned()));
            assert!(bridge.flags.contains(&"Running".to_owned()));
            assert_eq!(bridge.dir_port, 0);
        }
    }

    #[tokio::test]
    async fn inprocess_create_and_destroy() {
        let orch = InProcessOrchestrator::new();
        let topology = Topology::minimal();

        let network = orch.create_network(&topology).await.unwrap();
        assert!(network.id.starts_with("synth-"));
        assert_eq!(network.nodes.len(), 5);

        // Verify we can destroy it
        orch.destroy_network(&network.id).await.unwrap();

        // Destroying again should fail
        let result = orch.destroy_network(&network.id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn inprocess_wait_for_consensus_returns_immediately() {
        let orch = InProcessOrchestrator::new();
        let topology = Topology::minimal();

        let network = orch.create_network(&topology).await.unwrap();

        let start = std::time::Instant::now();
        let consensus = orch
            .wait_for_consensus(&network.id, 1)
            .await
            .unwrap();
        let elapsed = start.elapsed();

        // Should return nearly instantly (well under 100ms)
        assert!(elapsed.as_millis() < 100);
        assert_eq!(consensus.relay_count, 5);
        assert_eq!(consensus.valid_after, "2026-01-01 00:00:00");
    }

    #[tokio::test]
    async fn inprocess_network_status() {
        let orch = InProcessOrchestrator::new();
        let topology = Topology::minimal();

        let network = orch.create_network(&topology).await.unwrap();
        let status = orch.network_status(&network.id).await.unwrap();

        assert!(status.running);
        assert_eq!(status.node_count, 5);
        assert!(status.consensus_valid);
    }

    #[tokio::test]
    async fn inprocess_network_not_found() {
        let orch = InProcessOrchestrator::new();

        let result = orch.wait_for_consensus("nonexistent", 1).await;
        assert!(result.is_err());

        let result = orch.network_status("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn inprocess_invalid_topology_rejected() {
        let orch = InProcessOrchestrator::new();
        let topology = Topology {
            authority_count: 0,
            relay_count: 1,
            exit_count: 1,
            bridge_count: 0,
            hs_count: 0,
        };

        let result = orch.create_network(&topology).await;
        assert!(result.is_err());
    }
}
