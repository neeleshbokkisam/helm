"""cart-pole env matching helm-sim/src/cart_pole.rs (uniform rod, rk4)."""

from __future__ import annotations

import json
import math
import sys
from dataclasses import dataclass


GRAVITY = 9.8
MASS_CART = 1.0
MASS_POLE = 0.1
LENGTH = 0.5
DEFAULT_DT = 0.01


@dataclass
class State:
    x: float = 0.0
    x_dot: float = 0.0
    theta: float = 0.05
    theta_dot: float = 0.0

    def as_list(self) -> list[float]:
        return [self.x, self.x_dot, self.theta, self.theta_dot]


def derivatives(state: State, force: float) -> tuple[float, float, float, float]:
    g = GRAVITY
    m = MASS_POLE
    m_c = MASS_CART
    l = LENGTH
    total = m + m_c

    sin_t = math.sin(state.theta)
    cos_t = math.cos(state.theta)

    temp = (force + m * l * state.theta_dot ** 2 * sin_t) / total
    theta_acc = (g * sin_t - cos_t * temp) / (
        l * (4.0 / 3.0 - m * cos_t * cos_t / total)
    )
    x_acc = temp - m * l * theta_acc * cos_t / total

    return state.x_dot, x_acc, state.theta_dot, theta_acc


def rk4_step(state: State, force: float, dt: float) -> State:
    k1 = derivatives(state, force)

    s2 = State(
        state.x + k1[0] * dt * 0.5,
        state.x_dot + k1[1] * dt * 0.5,
        state.theta + k1[2] * dt * 0.5,
        state.theta_dot + k1[3] * dt * 0.5,
    )
    k2 = derivatives(s2, force)

    s3 = State(
        state.x + k2[0] * dt * 0.5,
        state.x_dot + k2[1] * dt * 0.5,
        state.theta + k2[2] * dt * 0.5,
        state.theta_dot + k2[3] * dt * 0.5,
    )
    k3 = derivatives(s3, force)

    s4 = State(
        state.x + k3[0] * dt,
        state.x_dot + k3[1] * dt,
        state.theta + k3[2] * dt,
        state.theta_dot + k3[3] * dt,
    )
    k4 = derivatives(s4, force)

    return State(
        state.x + (dt / 6.0) * (k1[0] + 2 * k2[0] + 2 * k3[0] + k4[0]),
        state.x_dot + (dt / 6.0) * (k1[1] + 2 * k2[1] + 2 * k3[1] + k4[1]),
        state.theta + (dt / 6.0) * (k1[2] + 2 * k2[2] + 2 * k3[2] + k4[2]),
        state.theta_dot + (dt / 6.0) * (k1[3] + 2 * k2[3] + 2 * k3[3] + k4[3]),
    )


def simulate_zero_force(steps: int, dt: float, initial: State) -> list[list[float]]:
    state = initial
    traj = [state.as_list()]
    for _ in range(steps):
        state = rk4_step(state, 0.0, dt)
        traj.append(state.as_list())
    return traj


def main() -> None:
    if len(sys.argv) < 2 or sys.argv[1] != "--dump-trajectory":
        print("usage: env.py --dump-trajectory", file=sys.stderr)
        sys.exit(1)

    traj = simulate_zero_force(500, DEFAULT_DT, State())
    json.dump(traj, sys.stdout)


if __name__ == "__main__":
    main()
