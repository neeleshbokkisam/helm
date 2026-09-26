interface Props {
  periodsMs: number[];
  deadlineMs: number;
}

export function PeriodChart({ periodsMs, deadlineMs }: Props) {
  const w = 360;
  const h = 100;
  const top = Math.max(deadlineMs * 1.5, ...periodsMs, 1);

  const points =
    periodsMs.length === 0
      ? ""
      : periodsMs
          .map((v, i) => {
            const x = (i / Math.max(periodsMs.length - 1, 1)) * w;
            const y = h - 8 - (v / top) * (h - 16);
            return `${x},${y}`;
          })
          .join(" ");

  const deadlineY = h - 8 - (deadlineMs / top) * (h - 16);

  return (
    <div className="chart-wrap">
      <svg viewBox={`0 0 ${w} ${h}`} className="period-chart" aria-label="tick period">
        <line
          x1={0}
          y1={deadlineY}
          x2={w}
          y2={deadlineY}
          stroke="#eab308"
          strokeWidth={1}
          strokeDasharray="4 3"
        />
        {points && (
          <polyline fill="none" stroke="#38bdf8" strokeWidth={2} points={points} />
        )}
      </svg>
      <p className="chart-range">dashed line is the {deadlineMs.toFixed(0)} ms period</p>
    </div>
  );
}
