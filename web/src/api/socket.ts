import type { ServerEvent } from "./types";

/**
 * Live event stream from the backend (`/ws`). Reconnects automatically.
 * Returns a function that closes the connection for good.
 */
export function connectSocket(onEvent: (e: ServerEvent) => void, onStatus: (connected: boolean) => void): () => void {
  let ws: WebSocket | null = null;
  let retry: number | undefined;
  let closed = false;

  const connect = () => {
    const proto = location.protocol === "https:" ? "wss" : "ws";
    ws = new WebSocket(`${proto}://${location.host}/ws`);
    ws.onopen = () => onStatus(true);
    ws.onmessage = (m) => {
      let e: ServerEvent;
      try {
        e = JSON.parse(m.data) as ServerEvent;
      } catch {
        return;
      }
      onEvent(e);
    };
    ws.onclose = () => {
      onStatus(false);
      if (!closed) retry = window.setTimeout(connect, 1500);
    };
  };

  connect();
  return () => {
    closed = true;
    window.clearTimeout(retry);
    ws?.close();
  };
}
