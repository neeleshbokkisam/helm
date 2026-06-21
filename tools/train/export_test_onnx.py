#!/usr/bin/env python3
"""export a tiny linear policy onnx for rust ci (matches pd stabilizer gains)."""

from __future__ import annotations

import json
from pathlib import Path

import torch
import torch.nn as nn

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / "crates/helm-modules/tests/fixtures/cartpole_test.onnx"
META = FIXTURE.with_suffix(".onnx.json")


def main() -> None:
    FIXTURE.parent.mkdir(parents=True, exist_ok=True)

    # obs order: x, x_dot, theta, theta_dot
    model = nn.Linear(4, 1)
    with torch.no_grad():
        model.weight.copy_(torch.tensor([[1.0, 2.0, 120.0, 20.0]]))
        model.bias.zero_()
    model.eval()

    dummy = torch.zeros(1, 4)
    torch.onnx.export(
        model,
        dummy,
        FIXTURE,
        input_names=["observation"],
        output_names=["action"],
        dynamic_axes=None,
        opset_version=17,
    )

    meta = {
        "observation_name": "observation",
        "observation_shape": [1, 4],
        "action_name": "action",
        "action_shape": [1, 1],
        "force_limit": 20.0,
        "params": {
            "gravity": 9.8,
            "mass_cart": 1.0,
            "mass_pole": 0.1,
            "length": 0.5,
        },
        "theta_zero": "upright",
    }
    META.write_text(json.dumps(meta, indent=2) + "\n")
    print(f"wrote {FIXTURE}")
    print(f"wrote {META}")


if __name__ == "__main__":
    main()
