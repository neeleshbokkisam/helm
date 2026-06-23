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
        <strong>{latched ? "latched fault" : "armed / ok"}</strong>
        <div className="sub">{latched ? label : "no active fault"}</div>
      </div>
    </div>
  );
}
