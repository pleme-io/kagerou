# kagerou

Tor testnet orchestrator.

Creates and manages private Tor networks for integration testing. A pure Rust
replacement for Chutney that orchestrates C-tor processes, generates torrc
configs, and polls for consensus. Useful for testing onion services, relay
behavior, or client connectivity without touching the real Tor network.

## Quick Start

```bash
cargo test                   # run all 58 tests (no tor binary needed)
cargo build --release        # release binary
nix build                    # Nix hermetic build
```

## Crates

| Crate | Purpose |
|-------|---------|
| `kagerou-core` | Types and traits: `NetworkOrchestrator`, `NodeManager`, topology, health |
| `kagerou-orchestrator` | Local implementation: torrc generation, process management, consensus polling |
| `kagerou-cli` | CLI binary with `create`, `status`, `wait`, and `destroy` subcommands |

## Topology Presets

| Preset | Authorities | Relays | Exits | Total |
|--------|-------------|--------|-------|-------|
| minimal (default) | 3 | 1 | 1 | 5 |
| standard | 3 | 3 | 2 | 8 |

## Usage

```bash
# Create a minimal test network (3 authorities, 1 relay, 1 exit)
kagerou create

# Create with the standard topology
kagerou create --standard

# Custom topology
kagerou create --authorities 3 --relays 5 --exits 3 --bridges 1

# Wait for consensus (up to 300s)
kagerou wait <network-id>

# Check network health
kagerou status <network-id>

# Tear down
kagerou destroy <network-id>
```

## License

MIT
