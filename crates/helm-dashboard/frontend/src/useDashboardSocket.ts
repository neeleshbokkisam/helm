import { useEffect, useRef, useState } from "react";
import type { HealthInfo } from "./components/DebugPanel";
import { appendStateWindow } from "./poleLayout";
import type {
  CartPoleState,
  ConnectionStatus,
  EndedMessage,
  HelloMessage,
  HistoryMessage,
  TickSnapshot,
} from "./types";

const MAX_POINTS = 200;
const MAX_BACKOFF_MS = 5000;

function wsUrl(): string {
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${window.location.host}/ws`;
}

function isTickSnapshot(data: unknown): data is TickSnapshot {
  if (!data || typeof data !== "object") return false;
  const d = data as Record<string, unknown>;
  return typeof d.tick === "number" && typeof d.state === "object";
}

function appendClamped(values: number[], next: number): number[] {
  const out = [...values, next];
  return out.length > MAX_POINTS ? out.slice(out.length - MAX_POINTS) : out;
}

function seedHistories(snapshots: TickSnapshot[]) {
  return snapshots.reduce(
    (acc, s) => ({
      force: appendClamped(acc.force, s.force_safe_n),
      theta: appendClamped(acc.theta, s.state.theta),
      x: appendClamped(acc.x, s.state.x),
      states: appendStateWindow(acc.states, s.state),
    }),
    {
      force: [] as number[],
      theta: [] as number[],
      x: [] as number[],
      states: [] as CartPoleState[],
    },
  );
}

async function fetchHealth(): Promise<HealthInfo> {
  const resp = await fetch("/health");
  if (!resp.ok) {
    throw new Error(`HTTP ${resp.status}`);
  }
  return resp.json() as Promise<HealthInfo>;
}

export function useDashboardSocket() {
  const [snapshot, setSnapshot] = useState<TickSnapshot | null>(null);
  const [hello, setHello] = useState<HelloMessage | null>(null);
  const [forceHistory, setForceHistory] = useState<number[]>([]);
  const [thetaHistory, setThetaHistory] = useState<number[]>([]);
  const [xHistory, setXHistory] = useState<number[]>([]);
  const [stateWindow, setStateWindow] = useState<CartPoleState[]>([]);
  const [status, setStatus] = useState<ConnectionStatus>("disconnected");
  const [runEnded, setRunEnded] = useState(false);
  const [endedReason, setEndedReason] = useState<string | null>(null);
  const [connectAttempts, setConnectAttempts] = useState(0);
  const [ticksReceived, setTicksReceived] = useState(0);
  const [lastEvent, setLastEvent] = useState<string | null>(null);
  const [lastError, setLastError] = useState<string | null>(null);
  const [health, setHealth] = useState<HealthInfo | null>(null);
  const [healthError, setHealthError] = useState<string | null>(null);
  const backoffRef = useRef(500);
  const url = wsUrl();

  useEffect(() => {
    let cancelled = false;
    fetchHealth()
      .then((h) => {
        if (!cancelled) {
          setHealth(h);
          setHealthError(null);
        }
      })
      .catch((e: unknown) => {
        if (!cancelled) {
          setHealth(null);
          setHealthError(e instanceof Error ? e.message : "unreachable");
        }
      });
    return () => {
      cancelled = true;
    };
  }, [status, connectAttempts]);

  useEffect(() => {
    let ws: WebSocket | null = null;
    let reconnectTimer: number | undefined;
    let closed = false;
    let ended = false;

    const connect = () => {
      if (ended) return;
      setConnectAttempts((n) => n + 1);
      setStatus("reconnecting");
      setLastError(null);
      ws = new WebSocket(url);

      ws.onopen = () => {
        backoffRef.current = 500;
        setStatus("connected");
        setLastEvent("ws open");
      };

      ws.onmessage = (ev) => {
        try {
          const data = JSON.parse(ev.data as string) as Record<string, unknown>;
          const kind = data.type;

          if (kind === "hello") {
            setHello(data as unknown as HelloMessage);
            setLastEvent("hello");
            return;
          }

          if (kind === "history") {
            const hist = data as unknown as HistoryMessage;
            const seeded = seedHistories(hist.snapshots);
            setForceHistory(seeded.force);
            setThetaHistory(seeded.theta);
            setXHistory(seeded.x);
            setStateWindow(seeded.states);
            const last = hist.snapshots[hist.snapshots.length - 1];
            if (last) setSnapshot(last);
            setLastEvent(`history (${hist.snapshots.length} snapshots)`);
            return;
          }

          if (kind === "ended") {
            const msg = data as unknown as EndedMessage;
            ended = true;
            setRunEnded(true);
            setEndedReason(msg.reason);
            setStatus("disconnected");
            setLastEvent(`ended (${msg.reason})`);
            ws?.close();
            return;
          }

          if (kind === "tick" && isTickSnapshot(data)) {
            setSnapshot(data);
            setForceHistory((prev) => appendClamped(prev, data.force_safe_n));
            setThetaHistory((prev) => appendClamped(prev, data.state.theta));
            setXHistory((prev) => appendClamped(prev, data.state.x));
            setStateWindow((prev) => appendStateWindow(prev, data.state));
            setTicksReceived((n) => n + 1);
            setLastEvent(`tick ${data.tick}`);
            return;
          }

          if (isTickSnapshot(data)) {
            setSnapshot(data);
            setForceHistory((prev) => appendClamped(prev, data.force_safe_n));
            setThetaHistory((prev) => appendClamped(prev, data.state.theta));
            setXHistory((prev) => appendClamped(prev, data.state.x));
            setStateWindow((prev) => appendStateWindow(prev, data.state));
            setTicksReceived((n) => n + 1);
            setLastEvent(`tick ${data.tick} (legacy)`);
            return;
          }

          setLastError(`unknown message type: ${String(kind)}`);
        } catch (e) {
          const msg = e instanceof Error ? e.message : "parse error";
          setLastError(`JSON parse failed: ${msg}`);
          console.warn("dashboard: malformed ws frame", ev.data, e);
        }
      };

      ws.onclose = (ev) => {
        if (closed || ended) return;
        setLastEvent(`ws closed (code ${ev.code})`);
        setStatus((prev) => {
          if (prev === "disconnected") return prev;
          return "reconnecting";
        });
        if (ev.code === 1006) {
          setLastError("connection refused or server stopped");
        }
        const delay = backoffRef.current;
        backoffRef.current = Math.min(delay * 2, MAX_BACKOFF_MS);
        reconnectTimer = window.setTimeout(connect, delay);
      };

      ws.onerror = () => {
        setLastError("websocket error (see server terminal)");
        ws?.close();
      };
    };

    connect();

    return () => {
      closed = true;
      window.clearTimeout(reconnectTimer);
      ws?.close();
    };
  }, [url]);

  return {
    snapshot,
    hello,
    forceHistory,
    thetaHistory,
    xHistory,
    stateWindow,
    status,
    runEnded,
    endedReason,
    debug: {
      wsUrl: url,
      connectAttempts,
      ticksReceived,
      lastEvent,
      lastError,
      health,
      healthError,
    },
  };
}
