# helm

Async robot control runtime: cart-pole sim, PD stabilizer, safety monitor, and optional live dashboard.

## Quick demo (recommended)

Build the dashboard UI once:

```bash
cd crates/helm-dashboard/frontend
npm ci
npm run build
```

Live sim demo — pole starts tilted, stabilizes in ~5 s, then **auto-resets every ~15 s** (runs until Ctrl-C):

```bash
cargo run -p helm-cli --features dashboard -- --demo
```

Open **http://127.0.0.1:8080** while the terminal is still running. Connect anytime — the next reset replays tilt → settle.

Looped offline replay (~6 s of action per loop, no static tail):

```bash
cargo run -p helm-cli --features dashboard -- \
  --replay demos/cart_pole_showcase.csv
```

Full 15 s recording (includes ~8 s settled tail between loops):

```bash
cargo run -p helm-cli --features dashboard -- \
  --replay demos/cart_pole_settle.csv
```

## Basic run (no UI)

```bash
cargo run -p helm-cli -- --seconds 5 --csv out.csv
```

## Tests

```bash
cargo test
cargo test -p helm-cli --features dashboard
```
