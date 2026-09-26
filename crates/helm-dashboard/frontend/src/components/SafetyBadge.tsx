import type { SafetyStatus } from "../types";
import { faultLabel } from "../types";

interface Props {
  safety: SafetyStatus | null;
}

export function SafetyBadge({ safety }: Props) {
  const latched = safety?.latched_fault != null;
  const label = faultLabel(safety?.latched_fault ?? null);

  return (
    <div className={`safety-badge ${latched ? "fault" : "ok"}`}>
      <span className="dot" />
      <div>
        <strong>{latched ? "Safety fault latched" : "Safety OK — force allowed through"}</strong>
        <div className="sub">
          {latched
            ? `${label}. Latch stays until the process restarts.`
            : "Zeros force if a fault latches"}
        </div>
      </div>
    </div>
  );
}
