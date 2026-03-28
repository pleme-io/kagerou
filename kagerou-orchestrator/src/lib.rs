//! Local Tor testnet orchestrator.
//!
//! Manages C-tor processes to create private Tor networks for testing.
//! Generates torrc configurations, spawns `tor` processes, and waits
//! for consensus.
//!
//! Also provides an in-process orchestrator ([`hardcoded`]) that generates
//! synthetic consensus documents without spawning real processes.

#[cfg(feature = "kakuremino")]
pub mod arti_client;
pub mod hardcoded;
pub mod orchestrator;
pub mod process;
pub mod torrc;

pub use hardcoded::{InProcessOrchestrator, SyntheticConsensus, SyntheticRelay};
pub use orchestrator::LocalOrchestrator;
pub use process::TorProcess;
pub use torrc::TorrcBuilder;
