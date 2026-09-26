import { chartHalfSpan } from "../format";

interface Props {
  values: number[];
}

export function ThetaChart({ values }: Props) {
  const w = 360;
  const h = 80;
  const deg = values.map((v) => (v * 180) / Math.PI);
  const halfSpan = chartHalfSpan(deg, 5);

  const points =
    deg.length === 0
      ? ""
      : deg
          .map((v, i) => {
            const x = (i / Math.max(deg.length - 1, 1)) * w;
            const y = h / 2 - (v / halfSpan) * (h / 2 - 6);
            return `${x},${y}`;
          })
          .join(" ");

  return (
    <div className="chart-wrap">
      <svg viewBox={`0 0 ${w} ${h}`} className="theta-chart" aria-label="angle trace">
        <line x1={0} y1={h / 2} x2={w} y2={h / 2} stroke="#444" strokeWidth={1} />
        {points && (
          <polyline fill="none" stroke="#60a5fa" strokeWidth={2} points={points} />
        )}
      </svg>
    </div>
  );
}
