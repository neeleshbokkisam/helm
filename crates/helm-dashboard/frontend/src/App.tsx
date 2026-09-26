import { CartChart } from "./components/CartChart";
import { DebugPanel } from "./components/DebugPanel";
import { ForceChart } from "./components/ForceChart";
import { MetricsRow } from "./components/MetricsRow";
import { PeriodChart } from "./components/PeriodChart";
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
    forceCmdHistory,
    thetaHistory,
    xHistory,
    periodHistory,
    status,
    runEnded,
    endedReason,
    debug,
  } = useDashboardSocket();

  const phase = derivePhase(snapshot, status, runEnded);
  const live = status === "connected" && !runEnded && debug.ticksReceived > 0;
  const loop = snapshot?.loop_stats;
  const showLoop = (loop?.core_count ?? 0) > 0 || (loop?.hz ?? 0) > 0;

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

      {showLoop && loop && (
        <section className="panel">
          <h2>Loop timing</h2>
          <p className="panel-sub">
            Rate is ticks per elapsed second. Jitter is lateness versus the deadline.
            A skip is a gap of at least 15 ms. A pipeline miss means safe force was late.
          </p>
          <dl className="metrics-row">
            <div>
              <dt>Ticks / s</dt>
              <dd>{loop.hz > 0 ? `${loop.hz.toFixed(1)} Hz` : "—"}</dd>
            </div>
            <div>
              <dt>Lateness p50 / p99 / max</dt>
              <dd>
                {(loop.jitter_p50_us / 1000).toFixed(2)} / {(loop.jitter_p99_us / 1000).toFixed(2)} /{" "}
                {(loop.jitter_max_us / 1000).toFixed(2)} ms
              </dd>
            </div>
            <div>
              <dt>Skipped ticks</dt>
              <dd>{loop.skip_count ?? 0}</dd>
            </div>
            <div>
              <dt>Pipeline misses</dt>
              <dd>{loop.miss_count}</dd>
            </div>
            <div>
              <dt>Compute</dt>
              <dd>
                {loop.compute_us > 0 ? `${(loop.compute_us / 1000).toFixed(2)} ms` : "—"}
              </dd>
            </div>
            <div>
              <dt>Cores</dt>
              <dd>{loop.core_count}</dd>
            </div>
            <div>
              <dt>Stress threads</dt>
              <dd>{loop.stress_threads}</dd>
            </div>
          </dl>
          <PeriodChart periodsMs={periodHistory} deadlineMs={(snapshot?.dt_secs ?? 0.01) * 1000} />
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
        <p className="panel-sub">Red is commanded force. Green is what safety forwards.</p>
        <ForceChart commanded={forceCmdHistory} safe={forceHistory} />
        <div className="readout">
          {(snapshot?.force_cmd_n ?? snapshot?.force_safe_n ?? 0).toFixed(2)} N commanded ·{" "}
          {snapshot?.force_safe_n.toFixed(2) ?? "—"} N safe
        </div>
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
