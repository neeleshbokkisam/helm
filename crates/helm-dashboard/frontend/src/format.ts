export function chartHalfSpan(values: number[], fallback: number): number {
  if (values.length === 0) return fallback;
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min;
  if (span > 1e-15) {
    return Math.max(span / 2, fallback * 1e-4);
  }
  const maxAbs = Math.max(...values.map((v) => Math.abs(v)), 1e-15);
  return Math.max(maxAbs, fallback * 1e-4);
}

export function formatAngleDeg(rad: number): string {
  const deg = (rad * 180) / Math.PI;
  if (Math.abs(deg) < 0.05) return `${(deg * 1000).toFixed(1)} m°`;
  return `${deg.toFixed(2)}°`;
}

export function formatCartCm(m: number): string {
  const cm = m * 100;
  if (Math.abs(cm) < 0.05) return `${(cm * 1000).toFixed(1)} mm`;
  return `${cm.toFixed(2)} cm`;
}

export function formatForce(n: number): string {
  if (Math.abs(n) < 0.05) return `${(n * 1000).toFixed(1)} mN`;
  return `${n.toFixed(2)} N`;
}

export function formatDelta(value: number, unit: string): string {
  const sign = value >= 0 ? "+" : "";
  const abs = Math.abs(value);
  if (unit === "°" && abs < 0.05) {
    return `${sign}${(value * 1000).toFixed(2)} m°`;
  }
  if (unit === " cm" && abs < 0.05) {
    return `${sign}${(value * 1000).toFixed(2)} mm`;
  }
  return `${sign}${value.toFixed(2)}${unit}`;
}
