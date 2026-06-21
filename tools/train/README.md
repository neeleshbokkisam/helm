# cart-pole policy training

Python env mirrors `helm-sim` RK4 cart-pole (`env.py`). Physics contract test in `crates/helm-sim/tests/physics_contract.rs` must pass before trusting training output.

## setup

```bash
pip install torch
```

## train

```bash
python train.py
python export_onnx.py checkpoints/policy.pt models/cartpole.onnx
```

Regenerate the bundled CI fixture (linear PD stand-in, not a trained policy):

```bash
python export_test_onnx.py
```

## stopping condition

Training is done when **all** of the following hold on the same checkpoint:

1. **Episode length**: 500 simulation steps at `dt=0.01` (same as the v0 demo / `CONTRACT_STEPS`).
2. **Stability**: `|theta| < 0.2` for every step of the episode (initial state included).
3. **Consecutive evals**: the criterion above passes for **10 consecutive** evaluation episodes.
4. **Initial conditions**: each eval samples `theta` uniformly from `[0.03, 0.08]` rad, with sign chosen randomly (not the fixed `0.05` demo seed). Other state components start at zero.

`train.py` implements this check and exits 0 only when it passes. If Rust policy tests pass but this criterion fails, suspect the training run—not the physics contract or ONNX wiring.

## helm

```bash
cargo run -p helm-cli --features onnx -- \
  --controller policy --model models/cartpole.onnx --seconds 5
```
