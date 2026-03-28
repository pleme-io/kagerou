# Kagerou — Tor Testnet Orchestrator

Pure Rust replacement for Chutney. Creates and manages private Tor networks
for integration testing by orchestrating C-tor processes.

**Tests:** 58

## Architecture

```
kagerou-core          — types + traits (Topology, NodeConfig, NodeRole, NetworkOrchestrator, NodeManager)
kagerou-orchestrator  — local implementation (torrc generation, process management, consensus polling)
kagerou-cli           — clap CLI (create, destroy, status, wait), execute() extracted for testability
```

### Key Types

| Type | Kind | Description |
|------|------|-------------|
| `RelayFlag` | Enum | 10 Tor consensus flags (Authority, Exit, Fast, Guard, HSDir, Running, Stable, StaleDesc, V2Dir, Valid) |
| `NetworkHealth` | Enum | 7 states (Healthy, Degraded, ConsensusStale, InsufficientRelays, NoExits, Bootstrapping, Unknown) |
| `TimeMode` | Enum | RealTime / Accelerated / Virtual |
| `ConsensusParams` | Struct | Consensus parameters for testnet configuration |
| `Error` | Struct | PartialEq + is_retryable() |

## Build

```bash
cargo check           # type check
cargo test            # run all tests
cargo build --release # optimized binary
nix build             # via substrate workspace builder
```

## CLI Usage

```bash
# Create a minimal test network (3 auth, 1 relay, 1 exit)
kagerou create

# Create with standard topology (3 auth, 3 relay, 2 exit)
kagerou create --standard

# Custom topology
kagerou create --authorities 3 --relays 5 --exits 3 --bridges 1

# Wait for consensus (up to 300s)
kagerou wait <network-id>

# Check status
kagerou status <network-id>

# Tear down
kagerou destroy <network-id>
```

## Topology Presets

| Preset | Authorities | Relays | Exits | Total |
|--------|-------------|--------|-------|-------|
| minimal | 3 | 1 | 1 | 5 |
| standard | 3 | 3 | 2 | 8 |

## Key Files

| File | Purpose |
|------|---------|
| `kagerou-core/src/lib.rs` | Core types, error enum, orchestrator/manager traits, RelayFlag, NetworkHealth, TimeMode, ConsensusParams |
| `kagerou-orchestrator/src/torrc.rs` | TorrcBuilder for generating torrc configs |
| `kagerou-orchestrator/src/process.rs` | TorProcess for managing C-tor child processes |
| `kagerou-orchestrator/src/orchestrator.rs` | LocalOrchestrator implementation |
| `kagerou-cli/src/main.rs` | CLI entry point with clap subcommands, execute() extracted for testability |

## Testing

Tests run without `tor` installed — process spawn tests expect failure and verify
error handling. Torrc generation and topology logic are tested directly.
Silent kill() errors replaced with tracing::warn.

```bash
cargo test --workspace
```

## Conventions

- Edition 2024, Rust 1.89.0+, MIT license
- clippy pedantic, release profile (codegen-units=1, lto=true)
- Pure Rust — no C FFI
- shikumi for config, tokio async runtime, thiserror 2
- Nix build via substrate `rust-workspace-release-flake.nix`
