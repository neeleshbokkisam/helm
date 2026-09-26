import { chartHalfSpan, formatForce } from "../format";

interface Props {
  commanded: number[];
  safe: number[];
}

function polyline(values: number[], w: number, h: number, halfSpan: number): string {
  if (values.length === 0) return "";
  return values
    .map((v, i) => {
      const x = (i / Math.max(values.length - 1, 1)) * w;
      const y = h / 2 - (v / halfSpan) * (h / 2 - 8);
      return `${x},${y}`;
    })
    .join(" ");
}

export function ForceChart({ commanded, safe }: Props) {
  const w = 360;
  const h = 100;
  const halfSpan = chartHalfSpan([...commanded, ...safe], 20);
  const active = safe.some((v) => Math.abs(v) > 0.05);
  const safePoints = polyline(safe, w, h, halfSpan);
  const cmdPoints = polyline(commanded, w, h, halfSpan);
  const combined = [...commanded, ...safe];
  const min = combined.length ? Math.min(...combined) : 0;
  const max = combined.length ? Math.max(...combined) : 0;

  return (
    <div className="chart-wrap">
      <svg viewBox={`0 0 ${w} ${h}`} className={`force-chart ${active ? "active" : ""}`} aria-label="force trace">
        <line x1={0} y1={h / 2} x2={w} y2={h / 2} stroke="#444" strokeWidth={1} />
        {cmdPoints && (
          <polyline fill="none" stroke="#ef4444" strokeWidth={2} points={cmdPoints} />
        )}
        {safePoints && (
          <polyline fill="none" stroke="#22c55e" strokeWidth={2} points={safePoints} />
        )}
      </svg>
      {combined.length > 1 && (
        <p className="chart-range">
          red commanded, green safe · window {formatForce(min)} to {formatForce(max)}
        </p>
      )}
    </div>
  );
}
