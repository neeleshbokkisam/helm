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

export interface LoopStats {
  tick: number;
  period_us: number;
  jitter_us: number;
  jitter_p50_us: number;
  jitter_p99_us: number;
  jitter_max_us: number;
  compute_us: number;
  miss: boolean;
  miss_count: number;
  skip_count?: number;
  hz: number;
  stress_threads: number;
  core_count: number;
}

export interface TickSnapshot {
  tick: number;
  dt_secs: number;
  state: CartPoleState;
  force_cmd_n?: number;
  force_safe_n: number;
  safety: SafetyStatus;
  loop_stats?: LoopStats;
}

export interface HelloMessage {
  type: "hello";
  mode: string;
  backend: string;
  dt_secs: number;
  initial_theta_rad: number;
  loops: boolean;
}

export interface HistoryMessage {
  type: "history";
  snapshots: TickSnapshot[];
}

export interface EndedMessage {
  type: "ended";
  final_tick: number;
  reason: string;
}

export type DemoPhase =
  | "connecting"
  | "starting"
  | "stabilizing"
  | "balanced"
  | "ended";

export type ConnectionStatus = "connected" | "reconnecting" | "disconnected";

export function faultLabel(fault: SafetyFault | null): string {
  if (!fault) return "none";
  if ("ForceOutOfRange" in fault) return "force out of range";
  if ("StateStale" in fault) return "state stale";
  if ("CommandStale" in fault) return "command stale";
  return "unknown";
}

export function radToDeg(rad: number): number {
  return (rad * 180) / Math.PI;
}

export function derivePhase(
  snapshot: TickSnapshot | null,
  connection: ConnectionStatus,
  runEnded: boolean,
): DemoPhase {
  if (runEnded) return "ended";
  if (connection !== "connected") return "connecting";
  if (!snapshot) return "connecting";

  const theta = Math.abs(snapshot.state.theta);
  const force = Math.abs(snapshot.force_safe_n);

  if (theta > 0.05) return "starting";
  if (theta > 0.005 || force > 0.1) return "stabilizing";
  return "balanced";
}

export function phaseMessage(
  phase: DemoPhase,
  snapshot: TickSnapshot | null,
  hello: HelloMessage | null,
): string {
  switch (phase) {
    case "connecting":
      return "Connecting to control loop…";
    case "starting": {
      const deg = radToDeg(snapshot?.state.theta ?? hello?.initial_theta_rad ?? 0.3);
      return `Pole tilted ~${Math.abs(deg).toFixed(0)}° — watch the controller catch it.`;
    }
    case "stabilizing":
      return "Stabilizing — controller applying force to balance the pole.";
    case "balanced":
      return "Balanced — motion is tiny at equilibrium. Demo mode resets every ~15 s so you can replay tilt → settle.";
    case "ended":
      return "Run finished. Restart with: cargo run -p helm-cli --features dashboard -- --demo";
  }
}
