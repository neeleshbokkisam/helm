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


def act(model: PolicyNet, state: State) -> float:
    with torch.no_grad():
        raw = model(obs_from_state(state)).item()
    return max(-FORCE_LIMIT, min(FORCE_LIMIT, raw))


@dataclass
class EvalResult:
    passed: bool
    max_abs_theta: float
    init_theta: float


def eval_episode(model: PolicyNet, init_theta: float) -> EvalResult:
    state = State(theta=init_theta)
    max_abs = abs(state.theta)
    for _ in range(EPISODE_STEPS):
        force = act(model, state)
        state = rk4_step(state, force, DEFAULT_DT)
        max_abs = max(max_abs, abs(state.theta))
        if abs(state.theta) >= THETA_LIMIT:
            return EvalResult(False, max_abs, init_theta)
    return EvalResult(True, max_abs, init_theta)


def eval_consecutive(model: PolicyNet, rng: random.Random) -> tuple[bool, list[EvalResult]]:
    results: list[EvalResult] = []
    for _ in range(CONSECUTIVE_EVALS):
        init_theta = rng.uniform(THETA_INIT_MIN, THETA_INIT_MAX) * rng.choice([-1.0, 1.0])
        result = eval_episode(model, init_theta)
        results.append(result)
        if not result.passed:
            return False, results
    return True, results


def train_step(model: PolicyNet, opt: torch.optim.Optimizer) -> float:
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
    parser.add_argument("--lr", type=float, default=1e-3)
    parser.add_argument("--hidden", type=int, default=64)
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument("--out", type=Path, default=Path("checkpoints/policy.pt"))
    args = parser.parse_args()

    random.seed(args.seed)
    torch.manual_seed(args.seed)
    rng = random.Random(args.seed + 1)

    model = PolicyNet(hidden=args.hidden)
    opt = torch.optim.Adam(model.parameters(), lr=args.lr)

    for step in range(1, args.steps + 1):
        loss = train_step(model, opt)
        ok, results = eval_consecutive(model, rng)
        if ok:
            args.out.parent.mkdir(parents=True, exist_ok=True)
            torch.save(
                {"state_dict": model.state_dict(), "hidden": args.hidden, "step": step},
                args.out,
            )
            print(f"success at step {step}: {CONSECUTIVE_EVALS} consecutive evals passed")
            for r in results:
                print(
                    f"  init_theta={r.init_theta:+.4f} max|theta|={r.max_abs_theta:.4f}"
                )
            return

        if step % 100 == 0:
            last = results[-1]
            print(
                f"step {step} loss={loss:.4f} "
                f"eval {len(results)}/{CONSECUTIVE_EVALS} "
                f"last init={last.init_theta:+.4f} max|theta|={last.max_abs_theta:.4f}"
            )

    print("stopped without meeting success criterion")
    raise SystemExit(1)


if __name__ == "__main__":
    main()
