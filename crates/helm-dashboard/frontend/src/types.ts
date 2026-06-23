export interface CartPoleState {
  x: number;
  x_dot: number;
  theta: number;
  theta_dot: number;
}

export type SafetyFault =
  | { ForceOutOfRange: { requested_n: number; limit_n: number } }
  | { StateStale: { ticks_since_update: number } }
  | { CommandStale: { ticks_since_update: number } };

export interface SafetyStatus {
  armed: boolean;
  latched_fault: SafetyFault | null;
  tick: number;
}

export interface TickSnapshot {
  tick: number;
  dt_secs: number;
  state: CartPoleState;
  force_safe_n: number;
  safety: SafetyStatus;
}

export type ConnectionStatus = "connected" | "reconnecting" | "disconnected";

export function faultLabel(fault: SafetyFault | null): string {
  if (!fault) return "none";
  if ("ForceOutOfRange" in fault) return "force out of range";
  if ("StateStale" in fault) return "state stale";
  if ("CommandStale" in fault) return "command stale";
  return "unknown";
}
