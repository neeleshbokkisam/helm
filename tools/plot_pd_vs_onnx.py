#!/usr/bin/env python3
"""Plot theta traces from compare-controllers CSV. No third-party packages."""

from __future__ import annotations

import csv
import sys
from collections import defaultdict
from pathlib import Path


def main() -> None:
    src = Path(sys.argv[1] if len(sys.argv) > 1 else "docs/pd_vs_onnx_trace.csv")
    dst = Path(sys.argv[2] if len(sys.argv) > 2 else "docs/media/pd_vs_onnx.svg")
    series: dict[tuple[str, str], list[tuple[float, float]]] = defaultdict(list)
    with src.open() as f:
        for row in csv.DictReader(f):
            if not row["theta0"].startswith("0.200"):
                continue
            series[(row["controller"], row["theta0"])].append(
                (float(row["t"]), float(row["theta"]))
            )

    width, height = 640, 280
    left, right, top, bottom = 40, 16, 28, 32
    plot_w = width - left - right
    plot_h = height - top - bottom
    colors = {"pd": "#22c55e", "onnx": "#38bdf8"}

    def xy(t: float, theta: float) -> tuple[float, float]:
        x = left + (t / 10.0) * plot_w
        y = top + (0.5 - theta / 0.5) * plot_h
        return x, y

    lines = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">',
        '<rect width="100%" height="100%" fill="#111318"/>',
        f'<text x="{left}" y="18" fill="#e4e4e7" font-family="ui-sans-serif,sans-serif" font-size="13">theta from +0.2 rad, PD vs trained ONNX</text>',
        f'<line x1="{left}" y1="{top + plot_h / 2}" x2="{left + plot_w}" y2="{top + plot_h / 2}" stroke="#3f3f46"/>',
    ]
    for (controller, _theta0), points in series.items():
        color = colors.get(controller, "#fff")
        pts = " ".join(f"{x:.1f},{y:.1f}" for x, y in (xy(t, th) for t, th in points))
        lines.append(f'<polyline fill="none" stroke="{color}" stroke-width="2" points="{pts}"/>')
    lines.append(
        f'<text x="{left}" y="{height - 10}" fill="#22c55e" font-family="ui-sans-serif,sans-serif" font-size="12">PD</text>'
    )
    lines.append(
        f'<text x="{left + 36}" y="{height - 10}" fill="#38bdf8" font-family="ui-sans-serif,sans-serif" font-size="12">ONNX</text>'
    )
    lines.append("</svg>")
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
