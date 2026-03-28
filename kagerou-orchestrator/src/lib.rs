//! Local Tor testnet orchestrator.
//!
//! Manages C-tor processes to create private Tor networks for testing.
//! Generates torrc configurations, spawns `tor` processes, and waits
//! for consensus.

pub mod orchestrator;
pub mod process;
pub mod torrc;

pub use orchestrator::LocalOrchestrator;
pub use process::TorProcess;
pub use torrc::TorrcBuilder;
