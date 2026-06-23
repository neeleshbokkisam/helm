import { useEffect, useRef, useState } from "react";
import type { ConnectionStatus, TickSnapshot } from "./types";

const MAX_FORCE_POINTS = 200;
const MAX_BACKOFF_MS = 5000;

function wsUrl(): string {
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${window.location.host}/ws`;
}

export function useDashboardSocket() {
  const [snapshot, setSnapshot] = useState<TickSnapshot | null>(null);
  const [forceHistory, setForceHistory] = useState<number[]>([]);
  const [status, setStatus] = useState<ConnectionStatus>("disconnected");
  const backoffRef = useRef(500);

  useEffect(() => {
    let ws: WebSocket | null = null;
    let reconnectTimer: number | undefined;
    let closed = false;

    const connect = () => {
      setStatus("reconnecting");
      ws = new WebSocket(wsUrl());

      ws.onopen = () => {
        backoffRef.current = 500;
        setStatus("connected");
      };

      ws.onmessage = (ev) => {
        try {
          const data = JSON.parse(ev.data as string) as TickSnapshot;
          setSnapshot(data);
          setForceHistory((prev) => {
            const next = [...prev, data.force_safe_n];
            return next.length > MAX_FORCE_POINTS
              ? next.slice(next.length - MAX_FORCE_POINTS)
              : next;
          });
        } catch {
          // ignore malformed frames
        }
      };

      ws.onclose = () => {
        if (closed) return;
        setStatus("reconnecting");
        const delay = backoffRef.current;
        backoffRef.current = Math.min(delay * 2, MAX_BACKOFF_MS);
        reconnectTimer = window.setTimeout(connect, delay);
      };

      ws.onerror = () => {
        ws?.close();
      };
    };

    connect();

    return () => {
      closed = true;
      window.clearTimeout(reconnectTimer);
      ws?.close();
    };
  }, []);

  return { snapshot, forceHistory, status };
}
