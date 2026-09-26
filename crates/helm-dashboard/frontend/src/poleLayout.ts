import type { CartPoleState } from "./types";

export const TRACK_W = 320;
export const TRACK_LEFT = 20;
export const POLE_L = 100;
export const STATE_WINDOW_SIZE = 200;

/**
 * Visual gain: multiply real angles/positions so they're clearly visible.
 * 0.3 rad (~17°) at startup maps to ~54° on screen — very obvious.
 */
const THETA_GAIN = Math.PI;
const X_GAIN = 300;

export interface PoleLayout {
  cartX: number;
  tipX: number;
  tipY: number;
  uprightTipX: number;
  uprightTipY: number;
  pivotY: number;
}

const DEFAULT_STATE: CartPoleState = {
  x: 0,
  x_dot: 0,
  theta: 0.3,
  theta_dot: 0,
};

export function appendStateWindow(
  window: CartPoleState[],
  state: CartPoleState,
): CartPoleState[] {
  const out = [...window, state];
  return out.length > STATE_WINDOW_SIZE
    ? out.slice(out.length - STATE_WINDOW_SIZE)
    : out;
}

export function computePoleLayout(
  state: CartPoleState | null,
): PoleLayout {
  const pivotY = 130;
  const trackCenter = TRACK_LEFT + TRACK_W / 2;
  const current = state ?? DEFAULT_STATE;

  const cartX = Math.max(
    TRACK_LEFT + 25,
    Math.min(TRACK_LEFT + TRACK_W - 25, trackCenter + current.x * X_GAIN),
  );

  const displayTheta = current.theta * THETA_GAIN;

  const uprightTipX = cartX;
  const uprightTipY = pivotY - POLE_L;
  const tipX = cartX + Math.sin(displayTheta) * POLE_L;
  const tipY = pivotY - Math.cos(displayTheta) * POLE_L;

  return { cartX, tipX, tipY, uprightTipX, uprightTipY, pivotY };
}

export function deltaOverWindow(values: number[], samples: number): number | null {
  if (values.length < 2) return null;
  const n = Math.min(samples, values.length - 1);
  return values[values.length - 1] - values[values.length - 1 - n];
}
