import { CartChart } from "./components/CartChart";
import { DebugPanel } from "./components/DebugPanel";
import { ForceChart } from "./components/ForceChart";
import { MetricsRow } from "./components/MetricsRow";
import { PoleView } from "./components/PoleView";
import { SafetyBadge } from "./components/SafetyBadge";
import { StatusBanner } from "./components/StatusBanner";
import { ThetaChart } from "./components/ThetaChart";
import { useDashboardSocket } from "./useDashboardSocket";
import { derivePhase } from "./types";
import "./App.css";

export function App() {
  const {
    snapshot,
    hello,
    forceHistory,
    thetaHistory,
    xHistory,
    status,
    runEnded,
    endedReason,
    debug,
  } = useDashboardSocket();

  const phase = derivePhase(snapshot, status, runEnded);
  const live = status === "connected" && !runEnded && debug.ticksReceived > 0;

  return (
    <main className="app">
      <header>
        <h1>helm cart-pole</h1>
        <div className="meta">
          {live && <span className="live-dot" title="Receiving live ticks" aria-hidden="true" />}
          <span className={`status ${status}`}>{status}</span>
        </div>
      </header>

      <StatusBanner phase={phase} snapshot={snapshot} hello={hello} />

      {runEnded && (
        <section className="ended-overlay">
          <strong>Run ended</strong>
          <p>
            {endedReason === "stopped"
              ? "The control loop stopped (Ctrl-C or timed run)."
              : "The control loop finished."}
          </p>
          <code>cargo run -p helm-cli --features dashboard -- --demo</code>
        </section>
      )}

      <section className="panel">
        <h2>Cart &amp; pole</h2>
        <PoleView state={snapshot?.state ?? null} />
        <MetricsRow
          snapshot={snapshot}
          thetaHistory={thetaHistory}
          xHistory={xHistory}
        />
      </section>

      <section className="panel">
        <h2>Cart position</h2>
        <p className="panel-sub">Last ~2 s of cart drift (auto-scaled)</p>
        <CartChart values={xHistory} />
      </section>

      <section className="panel">
        <h2>Controller force</h2>
        <p className="panel-sub">Force sent to the cart after the safety monitor</p>
        <ForceChart values={forceHistory} />
        <div className="readout">{snapshot?.force_safe_n.toFixed(2) ?? "—"} N</div>
      </section>

      <section className="panel">
        <h2>Pole angle</h2>
        <p className="panel-sub">Last ~2 s of angle history</p>
        <ThetaChart values={thetaHistory} />
      </section>

      <section className="panel">
        <h2>Safety monitor</h2>
        <p className="panel-sub">Zeros force if a fault latches</p>
        <SafetyBadge safety={snapshot?.safety ?? null} />
      </section>

      <DebugPanel status={status} hello={hello} debug={debug} />
    </main>
  );
}
