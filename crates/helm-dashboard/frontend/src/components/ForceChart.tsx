interface Props {
  values: number[];
}

export function ForceChart({ values }: Props) {
  const w = 360;
  const h = 100;
  const maxAbs = Math.max(20, ...values.map((v) => Math.abs(v)), 1);

  const points =
    values.length === 0
      ? ""
      : values
          .map((v, i) => {
            const x = (i / Math.max(values.length - 1, 1)) * w;
            const y = h / 2 - (v / maxAbs) * (h / 2 - 8);
            return `${x},${y}`;
          })
          .join(" ");

  return (
    <svg viewBox={`0 0 ${w} ${h}`} className="force-chart" aria-label="force trace">
      <line x1={0} y1={h / 2} x2={w} y2={h / 2} stroke="#444" strokeWidth={1} />
      {points && (
        <polyline fill="none" stroke="#22c55e" strokeWidth={2} points={points} />
      )}
    </svg>
  );
}
