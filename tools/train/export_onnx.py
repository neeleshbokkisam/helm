#!/usr/bin/env python3
"""export a trained policy to onnx + metadata sidecar for helm."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import torch
import torch.nn as nn

from env import GRAVITY, LENGTH, MASS_CART, MASS_POLE
from train import LinearPolicy, PolicyNet, build_model


def load_checkpoint(path: Path) -> nn.Module:
    ckpt = torch.load(path, map_location="cpu", weights_only=False)
    model_type = ckpt.get("model", "mlp")
    hidden = ckpt.get("hidden", 64)
    if model_type == "linear":
        model, _ = build_model(mlp=False, hidden=hidden)
    else:
        model, _ = build_model(mlp=True, hidden=hidden)
    model.load_state_dict(ckpt["state_dict"])
    model.eval()
    return model


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("checkpoint", type=Path, help="torch checkpoint from train.py")
    parser.add_argument("output", type=Path, help="output .onnx path")
    parser.add_argument("--force-limit", type=float, default=20.0)
    args = parser.parse_args()

    model = load_checkpoint(args.checkpoint)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    dummy = torch.zeros(1, 4)
    torch.onnx.export(
        model,
        dummy,
        args.output,
        input_names=["observation"],
        output_names=["action"],
        opset_version=17,
    )

    meta = {
        "observation_name": "observation",
        "observation_shape": [1, 4],
        "action_name": "action",
        "action_shape": [1, 1],
        "force_limit": args.force_limit,
        "params": {
            "gravity": GRAVITY,
            "mass_cart": MASS_CART,
            "mass_pole": MASS_POLE,
            "length": LENGTH,
        },
        "theta_zero": "upright",
    }
    meta_path = Path(f"{args.output}.json")
    meta_path.write_text(json.dumps(meta, indent=2) + "\n")
    print(f"wrote {args.output}")
    print(f"wrote {meta_path}")


if __name__ == "__main__":
    main()
