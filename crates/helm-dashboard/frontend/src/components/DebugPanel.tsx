import type { ConnectionStatus, HelloMessage } from "../types";

export interface HealthInfo {
  ok: boolean;
  ws_path: string;
  mode: string;
  backend: string;
  loops: boolean;
}

export interface DebugInfo {
  wsUrl: string;
  connectAttempts: number;
  ticksReceived: number;
  lastEvent: string | null;
  lastError: string | null;
  health: HealthInfo | null;
  healthError: string | null;
}

interface Props {
  status: ConnectionStatus;
  hello: HelloMessage | null;
  debug: DebugInfo;
}

function showHint(status: ConnectionStatus, health: HealthInfo | null): string | null {
  if (status === "connected") return null;
  if (health?.ok) {
    return "Server is up but WebSocket is not connected yet — retrying…";
  }
  return "Connection refused usually means helm is not running. Start it and keep the terminal open:";
}

export function DebugPanel({ status, hello, debug }: Props) {
  const hint = showHint(status, debug.health);
  const expanded =
    new URLSearchParams(window.location.search).has("debug") || status !== "connected";

  return (
    <section className="panel debug-panel">
      <details open={expanded}>
        <summary>
          Diagnostics
          {debug.lastError && <span className="debug-warn"> — {debug.lastError}</span>}
        </summary>
        {hint && (
          <div className="debug-hint">
            <p>{hint}</p>
            <code>
              cargo run -p helm-cli --features dashboard -- --replay demos/cart_pole_settle.csv
            </code>
          </div>
        )}
        <dl className="debug-grid">
          <dt>HTTP health</dt>
          <dd>
            {debug.health?.ok ? "ok" : debug.healthError ?? "checking…"}
            {debug.health && (
              <span className="debug-muted">
                {" "}
                ({debug.health.mode}/{debug.health.backend}
                {debug.health.loops ? ", loops" : ""})
              </span>
            )}
          </dd>
          <dt>WebSocket</dt>
          <dd>{debug.wsUrl}</dd>
          <dt>Status</dt>
          <dd>{status}</dd>
          <dt>Connect attempts</dt>
          <dd>{debug.connectAttempts}</dd>
          <dt>Ticks received</dt>
          <dd>{debug.ticksReceived}</dd>
          <dt>Last event</dt>
          <dd>{debug.lastEvent ?? "—"}</dd>
          <dt>Session (hello)</dt>
          <dd>
            {hello
              ? `${hello.mode}/${hello.backend}${hello.loops ? ", loops" : ""}`
              : "waiting for hello…"}
          </dd>
        </dl>
      </details>
    </section>
  );
}
