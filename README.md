# helm

[![ci](https://github.com/neeleshbokkisam/helm/actions/workflows/ci.yml/badge.svg)](https://github.com/neeleshbokkisam/helm/actions/workflows/ci.yml)

100 Hz cart-pole loop: sim or a fake serial plant, PD or a behavior-cloned linear policy, and a safety monitor that can drop the force.

```text
tick --> plant --> state --> PD or ONNX --> force_cmd --> safety --> force_cmd_safe --> plant
```

## Quickstart

```bash
cd crates/helm-dashboard/frontend && npm ci && npm run build
cargo run -p helm-cli --features dashboard -- --demo
```

Open http://127.0.0.1:8080. The pole starts at 0.3 rad and the sim resets every 15 s. Ctrl-C stops it.

Timing under load (`--stress` with no number uses one thread per core):

```bash
cargo run -p helm-cli --features dashboard -- --demo --stress
```

Fake serial plant, labeled as a PTY, not a device:

```bash
scripts/demo-fake-serial.sh
```

Fault on the dashboard (the pole falls after the latch because force goes to zero; restart the process to arm again):

```bash
cargo run -p helm-cli --features dashboard -- --demo --fault force-overshoot --fault-at 200
```

## 100 Hz

The tick loop is `tokio::time::interval` at 10 ms with `MissedTickBehavior::Skip`. The panel rate is ticks per elapsed second from the first fire. Jitter is lateness versus that tick's deadline. A skipped tick is a gap of at least 15 ms, so the interval jumped one or more slots. Pipeline misses are separate: `force_cmd_safe` for tick k was not published before tick k+1 fired. `--stress` with no number starts one burner thread per core; `--stress N` sets the count.

This is a desktop interval, not an RTOS deadline.

## Settle

`--demo` starts at about 17° and the PD controller stands the pole up in the first few seconds of each 15 s cycle. Record `docs/media/settle.gif` from http://127.0.0.1:8080 during that catch.

## Plant swap

`--plant sim` is the in-process model. `--plant fake-serial` is `--backend hardware --spawn-fake-device` and the UI says "simulated serial device (PTY), not physical hardware." `--backend` still works. `fake-serial` requires `--features hardware`.

## Safety

The chart plots commanded force in red and the force safety forwards in green. A latched fault holds the safe force at 0. The badge names the latch (`force out of range`, `state stale`, `command stale`) and stays red across the 15 s physics reset. `--fault stale-command` is an alias of `dropped-cmd`.

```text
running 3 tests
test rejects_out_of_range_force ... ok
test drops_stale_state ... ok
test zeros_force_on_stale_command ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

`cargo test -p helm-cli --test safety_faults --features dashboard,hardware,onnx`

## Physics contract

`cargo test -p helm-sim --test physics_contract` runs the Rust RK4 and `python3 tools/train/env.py --dump-trajectory` for 500 steps at `dt = 0.01` (501 samples, including t = 0). Each of `x`, `x_dot`, `theta`, `theta_dot` must match within `1e-10`. There are two cases: zero force, and a step of 12 N for 100 steps followed by `8 sin(0.05 · step)`. `1e-10` is the bar for two implementations of the same RK4, not for physical accuracy. Over 500 steps of f64, identical formulas should agree far below that; anything larger is a sign, mass, or integrator bug.

## PD vs ONNX

Both controllers balance ±0.05, ±0.1, and ±0.2 rad. `models/cartpole.onnx` is a linear policy trained by behavior cloning of the PD controller. The fitted gains are slightly smaller than the PD gains, so effort is a bit lower and settling is a few hundredths of a second later. Table, command, and plot: [docs/pd_vs_onnx.md](docs/pd_vs_onnx.md).

## Serial RTT

`tools/pty-rtt-bench` echoes 32 bytes on a PTY. Raw mode never calls `cfsetospeed`, so the round trip is scheduler latency and does not change with baud. Modeled mode waits `10 · n / baud` before the send and again before the echo (8N1, both legs). The modeled column is `2 · 10 · 32 / baud` in milliseconds: byte time on a PTY, not a UART. A plain `sleep` of 33 ms on macOS wakes about 4 ms late, which put an earlier run near 75 ms at 9600 instead of 66.7. The bench now sleeps most of each leg and spins the last few milliseconds. What is left between modeled and p50 is a fraction of a millisecond of PTY and scheduling.

| mode | baud | modeled (ms) | p50 (ms) | p99 (ms) |
| --- | ---: | ---: | ---: | ---: |
| raw | — | — | 0.012 | 0.044 |
| modeled | 9600 | 66.67 | 66.79 | 67.08 |
| modeled | 115200 | 5.56 | 5.59 | 6.13 |
| modeled | 921600 | 0.69 | 0.71 | 0.78 |

```bash
cargo run --manifest-path tools/pty-rtt-bench/Cargo.toml -- --mode raw
cargo run --manifest-path tools/pty-rtt-bench/Cargo.toml -- --mode modeled --baud 9600
```

## Recordings

Ten-second GIFs, width 960, under 8 MB, from http://127.0.0.1:8080 after `dashboard ready`:

- `docs/media/settle.gif` — `--demo`, from the tilt through upright.
- `docs/media/loop-stress.gif` — `--demo --stress`, Hz, jitter, skipped ticks, pipeline misses, and the 10 ms line.
- `docs/media/plant-swap.gif` — `scripts/demo-fake-serial.sh`, including the PTY label.
- `docs/media/fault.gif` — `--demo --fault force-overshoot --fault-at 200`. Start just before 2 s. The badge goes green to red, safe force hits zero, and the pole falls because safety zeroed the command. End on the red latch. A new process is required to arm again.

## Limits

The scheduler is a desktop interval, not an RTOS. The PTY is not a UART. The ONNX file is behavior cloning of the PD controller, with slightly smaller fitted gains. The safety latch is sticky. The hardware soak is 120 s, which is not a proof against every quantization plateau.
