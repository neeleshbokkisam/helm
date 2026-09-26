import type { DemoPhase, HelloMessage, TickSnapshot } from "../types";
import { phaseMessage } from "../types";

interface Props {
  phase: DemoPhase;
  snapshot: TickSnapshot | null;
  hello: HelloMessage | null;
}

export function StatusBanner({ phase, snapshot, hello }: Props) {
  const message = phaseMessage(phase, snapshot, hello);
  const elapsed =
    snapshot != null ? (snapshot.tick * snapshot.dt_secs).toFixed(1) : null;

  return (
    <section className={`status-banner phase-${phase}`} aria-live="polite">
      {hello?.backend === "fake-serial" && (
        <p className="status-message">
          Plant: simulated serial device (PTY), not physical hardware.
        </p>
      )}
      <p className="status-message">{message}</p>
      {snapshot != null && (
        <p className="status-meta">
          tick {snapshot.tick}
          {elapsed != null && <> · {elapsed}s elapsed</>}
          {hello?.loops && (
            <>
              {" "}
              · {hello.mode === "replay" ? "replay loops" : "demo loops every ~15 s"}
            </>
          )}
        </p>
      )}
    </section>
  );
}
