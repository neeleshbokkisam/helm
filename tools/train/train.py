#!/usr/bin/env python3
"""train a cart-pole policy; stop when eval success criterion is met."""

from __future__ import annotations

import argparse
import random
from dataclasses import dataclass
from pathlib import Path

import torch
import torch.nn as nn

from env import DEFAULT_DT, State, rk4_step

EPISODE_STEPS = 500
THETA_LIMIT = 0.2
CONSECUTIVE_EVALS = 10
THETA_INIT_MIN = 0.03
THETA_INIT_MAX = 0.08
FORCE_LIMIT = 20.0
# intentionally loose: stops before coefficient convergence (biased under-strength fit is ok)
FORCE_MATCH_EPS = 0.5
FORCE_MATCH_GRID = (0.03, 0.05, 0.08)


class LinearPolicy(nn.Module):
    def __init__(self) -> None:
        super().__init__()
        self.head = nn.Linear(4, 1)

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        return self.head(x)


class PolicyNet(nn.Module):
    def __init__(self, hidden: int = 64) -> None:
        super().__init__()
        self.net = nn.Sequential(
            nn.Linear(4, hidden),
            nn.Tanh(),
            nn.Linear(hidden, hidden),
            nn.Tanh(),
            nn.Linear(hidden, 1),
        )

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        return self.net(x)


def build_model(mlp: bool, hidden: int) -> tuple[nn.Module, str]:
    if mlp:
        return PolicyNet(hidden=hidden), "mlp"
    return LinearPolicy(), "linear"


def obs_from_state(state: State) -> torch.Tensor:
    return torch.tensor(
        [[state.x, state.x_dot, state.theta, state.theta_dot]], dtype=torch.float32
    )


def pd_force_raw(state: State) -> float:
    return (
        120.0 * state.theta
        + 20.0 * state.theta_dot
        + state.x
        + 2.0 * state.x_dot
    )


def infer_raw(model: nn.Module, state: State) -> float:
    with torch.no_grad():
        return model(obs_from_state(state)).item()


def act(model: nn.Module, state: State) -> float:
    raw = infer_raw(model, state)
    return max(-FORCE_LIMIT, min(FORCE_LIMIT, raw))


@dataclass
class EvalResult:
    passed: bool
    max_abs_theta: float
    init_theta: float
    force_err: float


def eval_episode(model: nn.Module, init_theta: float) -> EvalResult:
    init = State(theta=init_theta)
    pd = pd_force_raw(init)
    force_err = abs(infer_raw(model, init) - pd)

    state = init
    max_abs = abs(state.theta)
    for _ in range(EPISODE_STEPS):
        force = act(model, state)
        state = rk4_step(state, force, DEFAULT_DT)
        max_abs = max(max_abs, abs(state.theta))
        if abs(state.theta) >= THETA_LIMIT:
            return EvalResult(False, max_abs, init_theta, force_err)
    return EvalResult(True, max_abs, init_theta, force_err)


def eval_force_grid(model: nn.Module) -> tuple[bool, list[tuple[float, float, float, float]]]:
    checks: list[tuple[float, float, float, float]] = []
    for magnitude in FORCE_MATCH_GRID:
        for sign in (-1.0, 1.0):
            theta = sign * magnitude
            state = State(theta=theta)
            pd = pd_force_raw(state)
            net = infer_raw(model, state)
            err = abs(net - pd)
            checks.append((theta, pd, net, err))
            if err >= FORCE_MATCH_EPS:
                return False, checks
    return True, checks


def eval_consecutive(
    model: nn.Module, rng: random.Random
) -> tuple[bool, list[EvalResult]]:
    results: list[EvalResult] = []
    for _ in range(CONSECUTIVE_EVALS):
        init_theta = rng.uniform(THETA_INIT_MIN, THETA_INIT_MAX) * rng.choice([-1.0, 1.0])
        result = eval_episode(model, init_theta)
        results.append(result)
        if result.force_err >= FORCE_MATCH_EPS or not result.passed:
            return False, results
    return True, results


def train_step(model: nn.Module, opt: torch.optim.Optimizer) -> float:
    """regress onto unclamped pd output; clamp only at act() like helm."""
    model.train()
    batch = []
    targets = []
    for _ in range(128):
        theta = random.uniform(THETA_INIT_MIN, THETA_INIT_MAX) * random.choice([-1.0, 1.0])
        theta_dot = random.uniform(-0.5, 0.5)
        x = random.uniform(-0.2, 0.2)
        x_dot = random.uniform(-0.5, 0.5)
        state = State(x=x, x_dot=x_dot, theta=theta, theta_dot=theta_dot)
        batch.append([x, x_dot, theta, theta_dot])
        targets.append([pd_force_raw(state)])

    x = torch.tensor(batch, dtype=torch.float32)
    y = torch.tensor(targets, dtype=torch.float32)
    pred = model(x)
    loss = nn.functional.mse_loss(pred, y)
    opt.zero_grad()
    loss.backward()
    opt.step()
    return loss.item()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--steps", type=int, default=5000, help="max training iterations")
    parser.add_argument("--lr", type=float, default=None, help="default: 0.05 linear, 1e-3 mlp")
    parser.add_argument("--hidden", type=int, default=64, help="mlp hidden width")
    parser.add_argument("--mlp", action="store_true", help="use mlp instead of linear head")
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument("--out", type=Path, default=Path("checkpoints/policy.pt"))
    args = parser.parse_args()

    random.seed(args.seed)
    torch.manual_seed(args.seed)
    rng = random.Random(args.seed + 1)

    model, model_type = build_model(args.mlp, args.hidden)
    lr = args.lr if args.lr is not None else (1e-3 if args.mlp else 0.05)
    opt = torch.optim.Adam(model.parameters(), lr=lr)

    for step in range(1, args.steps + 1):
        loss = train_step(model, opt)
        ok, results = eval_consecutive(model, rng)
        grid_ok, grid = eval_force_grid(model)
        if ok and grid_ok:
            args.out.parent.mkdir(parents=True, exist_ok=True)
            torch.save(
                {
                    "state_dict": model.state_dict(),
                    "model": model_type,
                    "hidden": args.hidden,
                    "step": step,
                },
                args.out,
            )
            print(f"success at step {step}: {CONSECUTIVE_EVALS} consecutive evals passed")
            for r in results:
                print(
                    f"  init_theta={r.init_theta:+.4f} "
                    f"max|theta|={r.max_abs_theta:.4f} "
                    f"|f_err|={r.force_err:.4f}"
                )
            print("force-match grid (|f_err| < {:.1f} N):".format(FORCE_MATCH_EPS))
            for theta, pd, net, err in grid:
                print(f"  theta={theta:+.3f}  pd={pd:+.3f}  net={net:+.3f}  |err|={err:.4f}")
            return

        if step % 100 == 0:
            last = results[-1]
            grid_err = grid[0][3] if grid else float("nan")
            print(
                f"step {step} loss={loss:.4f} "
                f"eval {len(results)}/{CONSECUTIVE_EVALS} "
                f"last init={last.init_theta:+.4f} "
                f"max|theta|={last.max_abs_theta:.4f} "
                f"|f_err|={last.force_err:.4f} "
                f"grid_ok={grid_ok}"
            )

    print("stopped without meeting success criterion")
    raise SystemExit(1)


if __name__ == "__main__":
    main()
