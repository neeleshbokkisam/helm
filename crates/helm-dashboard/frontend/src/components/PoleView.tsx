import { computePoleLayout, TRACK_LEFT, TRACK_W } from "../poleLayout";
import type { CartPoleState } from "../types";

interface Props {
  state: CartPoleState | null;
}

export function PoleView({ state }: Props) {
  const layout = computePoleLayout(state);

  return (
    <div className="pole-wrap">
      <svg viewBox="0 0 360 180" className="pole-view" aria-label="cart pole">
        {/* track */}
        <line
          x1={TRACK_LEFT}
          y1={layout.pivotY}
          x2={TRACK_LEFT + TRACK_W}
          y2={layout.pivotY}
          stroke="#555"
          strokeWidth={2}
        />
        {/* upright target */}
        <line
          x1={layout.cartX}
          y1={layout.pivotY}
          x2={layout.uprightTipX}
          y2={layout.uprightTipY}
          stroke="#3f3f46"
          strokeWidth={2}
          strokeDasharray="4 4"
        />
        {/* cart */}
        <rect
          x={layout.cartX - 22}
          y={layout.pivotY - 14}
          width={44}
          height={28}
          rx={5}
          fill="#3b82f6"
        />
        {/* wheels */}
        <circle cx={layout.cartX - 14} cy={layout.pivotY + 14} r={5} fill="#1e3a5f" stroke="#3b82f6" strokeWidth={1.5} />
        <circle cx={layout.cartX + 14} cy={layout.pivotY + 14} r={5} fill="#1e3a5f" stroke="#3b82f6" strokeWidth={1.5} />
        {/* pole */}
        <line
          x1={layout.cartX}
          y1={layout.pivotY}
          x2={layout.tipX}
          y2={layout.tipY}
          stroke="#ef4444"
          strokeWidth={5}
          strokeLinecap="round"
        />
        {/* pole tip */}
        <circle cx={layout.tipX} cy={layout.tipY} r={6} fill="#ef4444" />
      </svg>
      <ul className="pole-legend">
        <li><span className="swatch track" /> track</li>
        <li><span className="swatch cart" /> cart</li>
        <li><span className="swatch pole" /> pole</li>
        <li><span className="swatch target" /> upright target</li>
      </ul>
    </div>
  );
}
