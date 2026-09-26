import { chartHalfSpan } from "../format";

interface Props {
  values: number[];
}

export function CartChart({ values }: Props) {
  const w = 360;
  const h = 80;
  const cm = values.map((v) => v * 100);
  const halfSpan = chartHalfSpan(cm, 5);

  const points =
    cm.length === 0
      ? ""
      : cm
          .map((v, i) => {
            const x = (i / Math.max(cm.length - 1, 1)) * w;
            const y = h / 2 - (v / halfSpan) * (h / 2 - 6);
            return `${x},${y}`;
          })
          .join(" ");

  return (
    <div className="chart-wrap">
      <svg viewBox={`0 0 ${w} ${h}`} className="cart-chart" aria-label="cart position trace">
        <line x1={0} y1={h / 2} x2={w} y2={h / 2} stroke="#444" strokeWidth={1} />
        {points && (
          <polyline fill="none" stroke="#a78bfa" strokeWidth={2} points={points} />
        )}
      </svg>
    </div>
  );
}
