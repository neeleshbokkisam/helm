# helm-hardware

Plant backend over a real PTY transport with binary framed wire protocol (`helm-wire`).

## Timeout policy

Hardware response timeout is a **tick-scheduling budget**, not a wire-speed bound.

```rust
pub const HOST_RESERVE_MS: u64 = 2;

pub fn hardware_response_timeout(dt_ms: u64) -> Duration {
    Duration::from_millis(dt_ms.saturating_sub(HOST_RESERVE_MS).max(1))
}
```

For `--dt-ms 10`, the plant waits up to **8 ms** for `RSP_STATE` after sending
`CMD_SET_FORCE`. The remaining **2 ms** is reserved for controller → safety →
publish on the host within the same tick.

### PTY round-trip (measured)

Spawned child, 32-byte payload, 1000 iterations on macOS:

| Run | Median | p99 | Max |
|-----|--------|-----|-----|
| Normal | ~0.0015 ms | ~0.002 ms | ~0.05 ms |
| Under CPU load | ~0.0016 ms | ~0.008 ms | ~0.5 ms |

PTY RTT is ~4000× smaller than the 8 ms timeout. The timeout exists so the plant
does not consume the entire tick window waiting on wire I/O.

See `tools/pty_rtt_measure_spawn.py` for the harness.

### Host reserve (measured)

`HOST_RESERVE_MS = 2` was an initial estimate for upper-stack work within a tick.
Measured on the full hardware stack (`measure_host_reserve_ms` in
`crates/helm-cli/tests/hardware_plant_swap.rs`): **~0.009 ms** max tick →
`FORCE_CMD_SAFE` latency at `--dt-ms 10`. The 2 ms reserve is conservative by
~200×; it remains the scheduling budget constant until tighter profiling says
otherwise.

## Usage

```bash
# Parent spawns fake device on PTY slave, host uses master
cargo run -p helm-cli --features hardware -- \
  --backend hardware --spawn-fake-device --seconds 30

# Manual two-terminal demo
cargo run -p helm-hardware --bin helm-fake-device -- --pty /dev/ttys00N
cargo run -p helm-cli --features hardware -- \
  --backend hardware --pty-path /dev/ttys00N --seconds 30
```

## Device transport faults

Injected on the wire by `helm-fake-device` (distinct from v2 bus `FaultConfig`):

| Fault | Behavior |
|-------|----------|
| `drop-bytes` | Omits a byte from outbound frames after tick N |
| `corrupt-crc` | Flips CRC bit after tick N |
| `silent` | Stops responding after tick N |
| `link-down` | Closes PTY after tick N |

On timeout or link error the plant **skips publish** for that tick; safety
`StateStale` latches if updates stop.
