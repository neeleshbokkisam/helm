# helm-dashboard

Live read-only WebSocket dashboard for the cart-pole runtime.

## Prerequisites — frontend build (required for the UI)

The SPA is **not** committed to git. Before opening the dashboard in a browser you **must** build it:

```bash
cd crates/helm-dashboard/frontend
npm ci
npm run build
```

This writes static files to `frontend/dist/`, which axum serves when `--dashboard` is enabled.

If `frontend/dist/` is missing, the WebSocket feed still works; the root URL will 404 until you run the build step above.

## Demo commands

**Live sim (recommended)** — pole starts tilted, stabilizes in ~5 s; runs until Ctrl-C:

```bash
cargo run -p helm-cli --features dashboard -- --demo
```

Open `http://127.0.0.1:8080`. The pole starts tilted (~3°), the controller pushes the cart, then the pole settles upright. **Demo mode auto-resets every ~15 s** so you always get another tilt → settle cycle.

**Looped replay** — ~6 s of action per loop (no long static tail):

```bash
cargo run -p helm-cli --features dashboard -- \
  --replay demos/cart_pole_showcase.csv
```

Full 15 s recording (includes settled tail between loops):

```bash
cargo run -p helm-cli --features dashboard -- \
  --replay demos/cart_pole_settle.csv
```

Timed live demo (sends run-ended to the UI):

```bash
cargo run -p helm-cli --features dashboard -- --demo --demo-seconds 60
```

Classic timed run:

```bash
cargo run -p helm-cli --features dashboard -- \
  --seconds 30 --dashboard --dashboard-port 8080
```

## WebSocket protocol

On connect, the server sends:

1. `hello` — session info (mode, dt, initial theta, whether replay loops)
2. `history` — last ~10 s of tick snapshots (for late browser refresh)
3. Live `tick` envelopes each control tick
4. `ended` when a timed run finishes

## Dev (optional)

Terminal 1 — runtime + API:

```bash
cargo run -p helm-cli --features dashboard -- --demo
```

Terminal 2 — Vite dev server with WS proxy:

```bash
cd crates/helm-dashboard/frontend
npm run dev
```

Open the Vite URL (usually `http://127.0.0.1:5173`).

## Scope

Read-only visualization only. No fault injection, gain tuning, or controller changes from the browser.

## Record a new replay CSV

```bash
cargo run -p helm-cli --features dashboard -- \
  --seconds 15 --csv demos/cart_pole_settle.csv
```

Header: `tick,x,x_dot,theta,theta_dot,force,force_safe,safety_fault`
