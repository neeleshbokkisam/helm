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

## Run

```bash
cargo run -p helm-cli --features dashboard -- \
  --seconds 30 --dashboard --dashboard-port 8080
```

Open `http://127.0.0.1:8080`.

## Dev (optional)

Terminal 1 — runtime + API:

```bash
cargo run -p helm-cli --features dashboard -- --seconds 300 --dashboard
```

Terminal 2 — Vite dev server with WS proxy:

```bash
cd crates/helm-dashboard/frontend
npm run dev
```

Open the Vite URL (usually `http://127.0.0.1:5173`).

## Scope

Read-only visualization only. No fault injection, gain tuning, or controller changes from the browser.

CSV record/replay for offline demo is a planned fast-follow, not part of v3.
