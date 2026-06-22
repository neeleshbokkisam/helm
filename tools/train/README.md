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

Use `--mlp` for a tanh MLP instead of the default linear head (not recommended for the PD regression target).

Regenerate the bundled CI fixture (linear PD stand-in, not a trained policy):

```bash
python export_test_onnx.py
```

## stopping condition

Training is done when **all** of the following hold on the same checkpoint:

1. **Episode length**: 500 simulation steps at `dt=0.01` (same as the v0 demo / `CONTRACT_STEPS`).
2. **Stability**: `|theta| < 0.2` for every step of the episode (initial state included).
3. **Consecutive evals**: criteria 1–2 pass for **10 consecutive** evaluation episodes.
4. **Initial conditions**: each eval samples `theta` uniformly from `[0.03, 0.08]` rad, with sign chosen randomly (not the fixed `0.05` demo seed). Other state components start at zero.
5. **Force match**: at each eval init state `(x, x_dot, theta, theta_dot) = (0, 0, theta_0, 0)`, the network's raw output must satisfy `|f_net - f_pd| < 0.5` N, where `f_pd = 120*theta + 20*theta_dot + x + 2*x_dot` (PD forces in this range are 3.6–9.6 N). The same bound must hold on a fixed grid at `theta = ±{0.03, 0.05, 0.08}`.

`train.py` exits 0 only when both the behavioral bound and force match pass together. A behavioral threshold alone is not sufficient: a weak controller can keep small initial perturbations under `|theta| < 0.2` without learning the target policy (Goodhart's law). Force match checks the learned mapping directly against ground-truth PD output.

Default architecture is a **linear head** (`4 → 1`), matching the provably linear PD target; default Adam lr is `0.05` (use `--mlp` for tanh MLP with lr `1e-3`).

## helm

```bash
cargo run -p helm-cli --features onnx -- \
  --controller policy --model models/cartpole.onnx --seconds 5
```
