#!/usr/bin/env python3
"""PTY RTT: spawned child (not fork), 32-byte payload, tiny child-side work."""

import os
import pty
import statistics
import subprocess
import sys
import time

CHILD = r"""
import sys
buf = sys.stdin.buffer
out = sys.stdout.buffer
while True:
    data = buf.read(32)
    if not data:
        break
    acc = 0
    for b in data:
        acc = (acc * 31 + b) & 0xFFFFFFFFFFFFFFFF
    out.write(data)
    out.flush()
"""

PAYLOAD = b"\xa5" * 32
ITERATIONS = 1000


def percentile(sorted_vals: list[float], p: float) -> float:
    k = (len(sorted_vals) - 1) * (p / 100.0)
    f = int(k)
    c = min(f + 1, len(sorted_vals) - 1)
    if f == c:
        return sorted_vals[f]
    return sorted_vals[f] + (sorted_vals[c] - sorted_vals[f]) * (k - f)


def run_once(label: str) -> None:
    master_fd, slave_fd = pty.openpty()
    proc = subprocess.Popen(
        [sys.executable, "-c", CHILD],
        stdin=slave_fd,
        stdout=slave_fd,
        stderr=subprocess.DEVNULL,
        close_fds=True,
    )
    os.close(slave_fd)
    master = os.fdopen(master_fd, "rb+", buffering=0)

    time.sleep(0.05)
    for _ in range(20):
        master.write(PAYLOAD)
        master.read(len(PAYLOAD))

    rtts_ms: list[float] = []
    for _ in range(ITERATIONS):
        t0 = time.perf_counter_ns()
        master.write(PAYLOAD)
        got = master.read(len(PAYLOAD))
        t1 = time.perf_counter_ns()
        if len(got) != len(PAYLOAD):
            break
        rtts_ms.append((t1 - t0) / 1_000_000.0)

    proc.kill()
    proc.wait()
    master.close()

    rtts_ms.sort()
    print(f"=== {label} ===")
    print(f"iterations: {len(rtts_ms)}")
    print(f"median_ms: {statistics.median(rtts_ms):.4f}")
    print(f"p99_ms: {percentile(rtts_ms, 99):.4f}")
    print(f"max_ms: {rtts_ms[-1]:.4f}")
    print(f"min_ms: {rtts_ms[0]:.4f}")
    print(f"mean_ms: {statistics.mean(rtts_ms):.4f}")


if __name__ == "__main__":
    run_once("spawned child, 32-byte echo + tiny work")
    run_once("run 2")
    run_once("run 3")
