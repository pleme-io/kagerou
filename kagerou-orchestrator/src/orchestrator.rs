//! Local network orchestrator implementation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use kagerou_core::{
    Consensus, Error, NetworkOrchestrator, NetworkStatus, NodeConfig, NodeHandle, NodeManager,
    NodeRole, Result, TestNetwork, Topology,
};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::process::TorProcess;
use crate::torrc::TorrcBuilder;

/// Orchestrates a private Tor network on the local machine.
#[derive(Debug)]
pub struct LocalOrchestrator {
    /// Base directory for all networks. Each network gets a subdirectory.
    base_dir: PathBuf,
    /// Base port for allocation. Ports are assigned sequentially.
    base_port: u16,
    /// Running networks keyed by network ID.
    networks: Arc<Mutex<HashMap<String, ManagedNetwork>>>,
}

/// Internal state for a managed network.
#[derive(Debug)]
struct ManagedNetwork {
    test_network: TestNetwork,
    processes: Vec<TorProcess>,
}

impl LocalOrchestrator {
    /// Create a new orchestrator that stores network data under `base_dir`.
    #[must_use]
    pub fn new(base_dir: impl AsRef<Path>, base_port: u16) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
            base_port,
            networks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Generate a unique network ID.
    fn generate_network_id() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("net-{ts}")
    }

    /// Allocate ports for all nodes in the topology.
    fn allocate_ports(&self, topology: &Topology) -> Vec<(u16, u16, u16)> {
        let mut ports = Vec::new();
        let total = topology.total_nodes();
        for i in 0..total {
            let base = self.base_port + i as u16 * 3;
            ports.push((base, base + 1, base + 2));
        }
        ports
    }

    /// Generate node configs from a topology.
    fn generate_node_configs(
        &self,
        topology: &Topology,
        network_dir: &Path,
    ) -> Vec<NodeConfig> {
        let ports = self.allocate_ports(topology);
        let mut configs = Vec::new();
        let mut port_idx = 0;

        // Directory authorities
        for i in 0..topology.authority_count {
            let (or_port, dir_port, control_port) = ports[port_idx];
            configs.push(NodeConfig {
                role: NodeRole::DirAuthority,
                nickname: format!("auth{i}"),
                or_port,
                dir_port,
                control_port,
                data_dir: network_dir.join(format!("auth{i}")),
            });
            port_idx += 1;
        }

        // Relays
        for i in 0..topology.relay_count {
            let (or_port, dir_port, control_port) = ports[port_idx];
            configs.push(NodeConfig {
                role: NodeRole::Relay,
                nickname: format!("relay{i}"),
                or_port,
                dir_port,
                control_port,
                data_dir: network_dir.join(format!("relay{i}")),
            });
            port_idx += 1;
        }

        // Exits
        for i in 0..topology.exit_count {
            let (or_port, dir_port, control_port) = ports[port_idx];
            configs.push(NodeConfig {
                role: NodeRole::Exit,
                nickname: format!("exit{i}"),
                or_port,
                dir_port,
                control_port,
                data_dir: network_dir.join(format!("exit{i}")),
            });
            port_idx += 1;
        }

        // Bridges
        for i in 0..topology.bridge_count {
            let (or_port, dir_port, control_port) = ports[port_idx];
            configs.push(NodeConfig {
                role: NodeRole::Bridge,
                nickname: format!("bridge{i}"),
                or_port,
                dir_port,
                control_port,
                data_dir: network_dir.join(format!("bridge{i}")),
            });
            port_idx += 1;
        }

        // Hidden service nodes
        for i in 0..topology.hs_count {
            let (or_port, dir_port, control_port) = ports[port_idx];
            configs.push(NodeConfig {
                role: NodeRole::Client,
                nickname: format!("hs{i}"),
                or_port,
                dir_port,
                control_port,
                data_dir: network_dir.join(format!("hs{i}")),
            });
            port_idx += 1;
        }

        configs
    }

    /// Build a torrc string for a node config.
    fn build_torrc(config: &NodeConfig) -> String {
        TorrcBuilder::new()
            .enable_testing_network()
            .set_role(config.role)
            .set_nickname(&config.nickname)
            .set_or_port(config.or_port)
            .set_dir_port(config.dir_port)
            .set_control_port(config.control_port)
            .set_data_dir(&config.data_dir)
            .build()
    }
}

#[async_trait]
impl NetworkOrchestrator for LocalOrchestrator {
    async fn create_network(&self, topology: &Topology) -> Result<TestNetwork> {
        topology.validate()?;

        let network_id = Self::generate_network_id();
        let network_dir = self.base_dir.join(&network_id);
        tokio::fs::create_dir_all(&network_dir).await?;

        info!(id = %network_id, "creating test network");

        let configs = self.generate_node_configs(topology, &network_dir);
        let mut nodes = Vec::new();
        let mut processes = Vec::new();

        for config in &configs {
            // Create node data directory
            tokio::fs::create_dir_all(&config.data_dir).await?;

            // Write torrc
            let torrc_content = Self::build_torrc(config);
            let torrc_path = config.data_dir.join("torrc");
            tokio::fs::write(&torrc_path, &torrc_content).await?;
            debug!(nickname = %config.nickname, path = %torrc_path.display(), "wrote torrc");

            // Spawn tor process
            match TorProcess::spawn(&torrc_path).await {
                Ok(proc) => {
                    let pid = proc.pid().unwrap_or(0);
                    nodes.push(NodeHandle {
                        pid,
                        role: config.role,
                        nickname: config.nickname.clone(),
                        or_port: config.or_port,
                        dir_port: config.dir_port,
                        control_port: config.control_port,
                    });
                    processes.push(proc);
                    info!(nickname = %config.nickname, pid, "started node");
                }
                Err(e) => {
                    warn!(nickname = %config.nickname, error = %e, "failed to start node");
                    // Clean up already-started processes
                    for mut p in processes {
                        if let Err(kill_err) = p.kill().await {
                            warn!(error = %kill_err, "failed to kill process during cleanup");
                        }
                    }
                    return Err(e);
                }
            }
        }

        let test_network = TestNetwork {
            id: network_id.clone(),
            topology: topology.clone(),
            data_dir: network_dir,
            nodes,
        };

        let managed = ManagedNetwork {
            test_network: test_network.clone(),
            processes,
        };

        self.networks.lock().await.insert(network_id, managed);

        Ok(test_network)
    }

    async fn destroy_network(&self, network_id: &str) -> Result<()> {
        let mut networks = self.networks.lock().await;
        let managed = networks
            .remove(network_id)
            .ok_or_else(|| Error::NetworkNotFound(network_id.to_owned()))?;

        info!(id = %network_id, "destroying test network");

        for mut proc in managed.processes {
            if let Err(e) = proc.kill().await {
                warn!(error = %e, "failed to kill process during teardown");
            }
        }

        // Remove data directory
        if managed.test_network.data_dir.exists() {
            tokio::fs::remove_dir_all(&managed.test_network.data_dir).await?;
        }

        Ok(())
    }

    async fn wait_for_consensus(&self, network_id: &str, timeout_secs: u64) -> Result<Consensus> {
        let networks = self.networks.lock().await;
        let managed = networks
            .get(network_id)
            .ok_or_else(|| Error::NetworkNotFound(network_id.to_owned()))?;

        // Look for the cached-consensus file in the first authority's data dir
        let consensus_dir = managed.test_network.data_dir.clone();
        drop(networks);

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);

        loop {
            if start.elapsed() > timeout {
                return Err(Error::ConsensusTimeout(timeout_secs));
            }

            // Check for consensus in any authority data dir
            let mut found = false;
            if let Ok(mut entries) = tokio::fs::read_dir(&consensus_dir).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let consensus_path = entry.path().join("cached-consensus");
                    if consensus_path.exists() {
                        found = true;
                        break;
                    }
                }
            }

            if found {
                info!(id = %network_id, "consensus reached");
                return Ok(Consensus {
                    valid_after: "testing".to_owned(),
                    valid_until: "testing".to_owned(),
                    relay_count: 0,
                });
            }

            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    async fn network_status(&self, network_id: &str) -> Result<NetworkStatus> {
        let mut networks = self.networks.lock().await;
        let managed = networks
            .get_mut(network_id)
            .ok_or_else(|| Error::NetworkNotFound(network_id.to_owned()))?;

        let mut running_count: u32 = 0;
        for proc in &mut managed.processes {
            if proc.is_running() {
                running_count += 1;
            }
        }

        let total = managed.test_network.nodes.len() as u32;

        Ok(NetworkStatus {
            running: running_count > 0,
            node_count: total,
            consensus_valid: false,
        })
    }
}

#[async_trait]
impl NodeManager for LocalOrchestrator {
    async fn start_node(&self, config: &NodeConfig) -> Result<NodeHandle> {
        tokio::fs::create_dir_all(&config.data_dir).await?;

        let torrc_content = Self::build_torrc(config);
        let torrc_path = config.data_dir.join("torrc");
        tokio::fs::write(&torrc_path, &torrc_content).await?;

        let proc = TorProcess::spawn(&torrc_path).await?;
        let pid = proc.pid().unwrap_or(0);

        Ok(NodeHandle {
            pid,
            role: config.role,
            nickname: config.nickname.clone(),
            or_port: config.or_port,
            dir_port: config.dir_port,
            control_port: config.control_port,
        })
    }

    async fn stop_node(&self, pid: u32) -> Result<()> {
        // Send SIGTERM via nix or fallback to kill
        let output = tokio::process::Command::new("kill")
            .arg(pid.to_string())
            .output()
            .await
            .map_err(|e| Error::ProcessStop(format!("failed to kill pid {pid}: {e}")))?;

        if !output.status.success() {
            return Err(Error::ProcessStop(format!(
                "kill pid {pid} exited with {}",
                output.status
            )));
        }

        Ok(())
    }

    async fn node_status(&self, pid: u32) -> Result<bool> {
        let output = tokio::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .output()
            .await
            .map_err(|e| Error::ProcessStop(format!("failed to check pid {pid}: {e}")))?;

        Ok(output.status.success())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topology_validation_rejects_no_authorities() {
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
    fn topology_validation_accepts_minimal() {
        let t = Topology::minimal();
        assert!(t.validate().is_ok());
    }

    #[tokio::test]
    async fn temp_dir_creation() {
        let tmp = tempfile::tempdir().unwrap();
        let orchestrator = LocalOrchestrator::new(tmp.path(), 10000);
        let network_dir = orchestrator.base_dir.join("test-net");
        tokio::fs::create_dir_all(&network_dir).await.unwrap();
        assert!(network_dir.exists());
    }

    #[test]
    fn network_id_generation() {
        let id1 = LocalOrchestrator::generate_network_id();
        let id2 = LocalOrchestrator::generate_network_id();
        assert!(id1.starts_with("net-"));
        assert!(id2.starts_with("net-"));
        // IDs should be unique (different timestamps)
        // In a fast test they might be the same millisecond, so we just check format.
    }

    #[test]
    fn port_allocation_sequential() {
        let tmp = tempfile::tempdir().unwrap();
        let orchestrator = LocalOrchestrator::new(tmp.path(), 10000);
        let topology = Topology::minimal(); // 5 nodes
        let ports = orchestrator.allocate_ports(&topology);
        assert_eq!(ports.len(), 5);
        assert_eq!(ports[0], (10000, 10001, 10002));
        assert_eq!(ports[1], (10003, 10004, 10005));
    }

    #[test]
    fn node_config_generation() {
        let tmp = tempfile::tempdir().unwrap();
        let orchestrator = LocalOrchestrator::new(tmp.path(), 10000);
        let topology = Topology::minimal();
        let network_dir = tmp.path().join("test");
        let configs = orchestrator.generate_node_configs(&topology, &network_dir);
        assert_eq!(configs.len(), 5);
        assert_eq!(configs[0].role, NodeRole::DirAuthority);
        assert_eq!(configs[0].nickname, "auth0");
        assert_eq!(configs[3].role, NodeRole::Relay);
        assert_eq!(configs[4].role, NodeRole::Exit);
    }
}
