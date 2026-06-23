import { ForceChart } from "./components/ForceChart";
import { PoleView } from "./components/PoleView";
import { SafetyBadge } from "./components/SafetyBadge";
import { useDashboardSocket } from "./useDashboardSocket";
import "./App.css";

export function App() {
  const { snapshot, forceHistory, status } = useDashboardSocket();

  return (
    <main className="app">
      <header>
        <h1>helm cart-pole</h1>
        <div className="meta">
          <span>tick {snapshot?.tick ?? "—"}</span>
          <span className={`status ${status}`}>{status}</span>
        </div>
      </header>

      <section className="panel">
        <h2>pole</h2>
        <PoleView state={snapshot?.state ?? null} />
      </section>

      <section className="panel">
        <h2>force safe (N)</h2>
        <ForceChart values={forceHistory} />
        <div className="readout">{snapshot?.force_safe_n.toFixed(2) ?? "—"}</div>
      </section>

      <section className="panel">
        <h2>safety</h2>
        <SafetyBadge safety={snapshot?.safety ?? null} />
      </section>
    </main>
  );
}
