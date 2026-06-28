#!/usr/bin/env python3
"""Bare PTY round-trip latency: forked child echo loop, no protocol."""

import os
import pty
import select
import statistics
import sys
import time


def percentile(sorted_vals: list[float], p: float) -> float:
    if not sorted_vals:
        return 0.0
    k = (len(sorted_vals) - 1) * (p / 100.0)
    f = int(k)
    c = min(f + 1, len(sorted_vals) - 1)
    if f == c:
        return sorted_vals[f]
    return sorted_vals[f] + (sorted_vals[c] - sorted_vals[f]) * (k - f)


def main() -> int:
    iterations = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
    payload = b"x" * 16

    master_fd, slave_fd = pty.openpty()
    pid = os.fork()

    if pid == 0:
        os.close(master_fd)
        os.dup2(slave_fd, 0)
        os.dup2(slave_fd, 1)
        os.close(slave_fd)
        while True:
            try:
                data = os.read(0, 64)
            except OSError:
                break
            if not data:
                break
            os.write(1, data)
        os._exit(0)

    os.close(slave_fd)
    master = os.fdopen(master_fd, "rb+", buffering=0)

    # Warmup
    for _ in range(20):
        master.write(payload)
        master.read(len(payload))

    rtts_ms: list[float] = []
    for _ in range(iterations):
        t0 = time.perf_counter_ns()
        master.write(payload)
        got = master.read(len(payload))
        t1 = time.perf_counter_ns()
        if len(got) != len(payload):
            print(f"short read: {len(got)}", file=sys.stderr)
            break
        rtts_ms.append((t1 - t0) / 1_000_000.0)

    os.kill(pid, 9)
    os.waitpid(pid, 0)
    master.close()

    rtts_ms.sort()
    print(f"iterations: {len(rtts_ms)}")
    print(f"payload_bytes: {len(payload)}")
    print(f"median_ms: {statistics.median(rtts_ms):.4f}")
    print(f"p99_ms: {percentile(rtts_ms, 99):.4f}")
    print(f"max_ms: {rtts_ms[-1]:.4f}")
    print(f"min_ms: {rtts_ms[0]:.4f}")
    print(f"mean_ms: {statistics.mean(rtts_ms):.4f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
