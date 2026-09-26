import { chartHalfSpan, formatForce } from "../format";

interface Props {
  values: number[];
}

export function ForceChart({ values }: Props) {
  const w = 360;
  const h = 100;
  const halfSpan = chartHalfSpan(values, 20);
  const active = values.some((v) => Math.abs(v) > 0.05);

  const points =
    values.length === 0
      ? ""
      : values
          .map((v, i) => {
            const x = (i / Math.max(values.length - 1, 1)) * w;
            const y = h / 2 - (v / halfSpan) * (h / 2 - 8);
            return `${x},${y}`;
          })
          .join(" ");

  const min = values.length ? Math.min(...values) : 0;
  const max = values.length ? Math.max(...values) : 0;

  return (
    <div className="chart-wrap">
      <svg viewBox={`0 0 ${w} ${h}`} className={`force-chart ${active ? "active" : ""}`} aria-label="force trace">
        <line x1={0} y1={h / 2} x2={w} y2={h / 2} stroke="#444" strokeWidth={1} />
        {points && (
          <polyline fill="none" stroke="#22c55e" strokeWidth={2} points={points} />
        )}
      </svg>
      {values.length > 1 && (
        <p className="chart-range">
          window {formatForce(min)} to {formatForce(max)}
        </p>
      )}
    </div>
  );
}
