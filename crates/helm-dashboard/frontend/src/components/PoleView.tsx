import type { CartPoleState } from "../types";

const TRACK_W = 320;
const POLE_L = 90;

interface Props {
  state: CartPoleState | null;
}

export function PoleView({ state }: Props) {
  const x = state?.x ?? 0;
  const theta = state?.theta ?? 0.05;

  const cartX = TRACK_W / 2 + x * 40;
  const pivotY = 120;
  const tipX = cartX + Math.sin(theta) * POLE_L;
  const tipY = pivotY - Math.cos(theta) * POLE_L;

  return (
    <svg viewBox="0 0 360 160" className="pole-view" aria-label="cart pole">
      <line x1={20} y1={pivotY} x2={340} y2={pivotY} stroke="#555" strokeWidth={2} />
      <rect
        x={cartX - 20}
        y={pivotY - 12}
        width={40}
        height={24}
        rx={4}
        fill="#3b82f6"
      />
      <line
        x1={cartX}
        y1={pivotY}
        x2={tipX}
        y2={tipY}
        stroke="#ef4444"
        strokeWidth={4}
        strokeLinecap="round"
      />
      <circle cx={tipX} cy={tipY} r={5} fill="#ef4444" />
    </svg>
  );
}
