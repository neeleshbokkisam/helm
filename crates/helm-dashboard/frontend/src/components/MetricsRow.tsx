import { formatAngleDeg, formatCartCm, formatDelta, formatForce } from "../format";
import { deltaOverWindow } from "../poleLayout";
import type { TickSnapshot } from "../types";

interface Props {
  snapshot: TickSnapshot | null;
  thetaHistory: number[];
  xHistory: number[];
}

const DELTA_SAMPLES = 50;

export function MetricsRow({ snapshot, thetaHistory, xHistory }: Props) {
  const theta = snapshot?.state.theta ?? 0.05;
  const x = snapshot?.state.x ?? 0;
  const force = snapshot?.force_safe_n ?? 0;

  const thetaDelta = deltaOverWindow(thetaHistory, DELTA_SAMPLES);
  const xDelta = deltaOverWindow(xHistory, DELTA_SAMPLES);

  return (
    <dl className="metrics-row">
      <div>
        <dt>Angle</dt>
        <dd>{formatAngleDeg(theta)}</dd>
        <span className="metric-hint">
          target 0°
          {thetaDelta != null && (
            <> · Δ {formatDelta((thetaDelta * 180) / Math.PI, "°")} / 0.5s</>
          )}
        </span>
      </div>
      <div>
        <dt>Cart</dt>
        <dd>{formatCartCm(x)}</dd>
        <span className="metric-hint">
          from center
          {xDelta != null && <> · Δ {formatDelta(xDelta * 100, " cm")} / 0.5s</>}
        </span>
      </div>
      <div>
        <dt>Force</dt>
        <dd>{formatForce(force)}</dd>
        <span className="metric-hint">after safety</span>
      </div>
    </dl>
  );
}
