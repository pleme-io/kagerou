//! Kagerou CLI — Tor testnet orchestrator.

use clap::{Parser, Subcommand};
use kagerou_core::{NetworkOrchestrator, Topology};
use kagerou_orchestrator::LocalOrchestrator;
use tracing::info;

/// Kagerou (陽炎) — private Tor network orchestrator for testing.
#[derive(Parser)]
#[command(name = "kagerou", version, about)]
struct Cli {
    /// Base directory for network data.
    #[arg(long, default_value = "/tmp/kagerou")]
    data_dir: String,

    /// Base port for node allocation.
    #[arg(long, default_value_t = 10000)]
    base_port: u16,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new private Tor test network.
    Create {
        /// Number of directory authorities.
        #[arg(long, default_value_t = 3)]
        authorities: u32,

        /// Number of middle relays.
        #[arg(long, default_value_t = 1)]
        relays: u32,

        /// Number of exit relays.
        #[arg(long, default_value_t = 1)]
        exits: u32,

        /// Number of bridge relays.
        #[arg(long, default_value_t = 0)]
        bridges: u32,

        /// Number of hidden-service nodes.
        #[arg(long, default_value_t = 0)]
        hs: u32,

        /// Use the standard topology (overrides individual counts).
        #[arg(long)]
        standard: bool,
    },

    /// Destroy a running test network.
    Destroy {
        /// Network ID to destroy.
        #[arg(required = true)]
        network_id: String,
    },

    /// Show status of a running test network.
    Status {
        /// Network ID to query.
        #[arg(required = true)]
        network_id: String,
    },

    /// Wait for consensus to be reached on a test network.
    Wait {
        /// Network ID to wait on.
        #[arg(required = true)]
        network_id: String,

        /// Timeout in seconds.
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
}

/// Execute CLI commands against an orchestrator.
///
/// Extracted from `main` for testability.
async fn execute(
    orchestrator: &LocalOrchestrator,
    command: Commands,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Commands::Create {
            authorities,
            relays,
            exits,
            bridges,
            hs,
            standard,
        } => {
            let topology = if standard {
                Topology::standard()
            } else {
                Topology {
                    authority_count: authorities,
                    relay_count: relays,
                    exit_count: exits,
                    bridge_count: bridges,
                    hs_count: hs,
                }
            };

            topology.validate()?;

            info!(
                authorities = topology.authority_count,
                relays = topology.relay_count,
                exits = topology.exit_count,
                "creating test network"
            );

            let network = orchestrator.create_network(&topology).await?;
            println!("Network created: {}", network.id);
            println!("Data directory: {}", network.data_dir.display());
            println!("Nodes: {}", network.nodes.len());
            for node in &network.nodes {
                println!(
                    "  {} ({}) — OR:{} Dir:{} Ctrl:{} PID:{}",
                    node.nickname, node.role, node.or_port, node.dir_port, node.control_port,
                    node.pid
                );
            }
        }

        Commands::Destroy { network_id } => {
            orchestrator.destroy_network(&network_id).await?;
            println!("Network {network_id} destroyed.");
        }

        Commands::Status { network_id } => {
            let status = orchestrator.network_status(&network_id).await?;
            println!("Network: {network_id}");
            println!("  Running: {}", status.running);
            println!("  Nodes: {}", status.node_count);
            println!("  Consensus valid: {}", status.consensus_valid);
        }

        Commands::Wait {
            network_id,
            timeout,
        } => {
            println!("Waiting for consensus on {network_id} (timeout: {timeout}s)...");
            let consensus = orchestrator
                .wait_for_consensus(&network_id, timeout)
                .await?;
            println!("Consensus reached!");
            println!("  Valid after: {}", consensus.valid_after);
            println!("  Valid until: {}", consensus.valid_until);
            println!("  Relay count: {}", consensus.relay_count);
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let orchestrator = LocalOrchestrator::new(&cli.data_dir, cli.base_port);

    execute(&orchestrator, cli.command).await
}
