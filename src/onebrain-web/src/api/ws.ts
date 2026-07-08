import type { WsEvent } from './types';

type EventHandler = (event: WsEvent) => void;

export class OneBrainWs {
  private ws: WebSocket | null = null;
  private handlers: Map<string, Set<EventHandler>> = new Map();
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectDelay = 2000;

  connect(token: string) {
    if (this.ws?.readyState === WebSocket.OPEN) return;
    try {
      this.ws = new WebSocket(`ws://127.0.0.1:4280/ws/events?token=${encodeURIComponent(token)}`);
      this.ws.onmessage = (e) => {
        try {
          const event: WsEvent = JSON.parse(e.data);
          this.emit(event.event_type, event);
          this.emit('*', event); // wildcard listeners
        } catch { /* ignore parse errors */ }
      };
      this.ws.onclose = () => {
        this.scheduleReconnect(token);
      };
      this.ws.onerror = () => {
        this.ws?.close();
      };
    } catch { /* ignore connection errors */ }
  }

  disconnect() {
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.ws?.close();
    this.ws = null;
  }

  on(eventType: string, handler: EventHandler) {
    if (!this.handlers.has(eventType)) {
      this.handlers.set(eventType, new Set());
    }
    this.handlers.get(eventType)!.add(handler);
    return () => { this.handlers.get(eventType)?.delete(handler); };
  }

  private emit(eventType: string, event: WsEvent) {
    this.handlers.get(eventType)?.forEach(h => h(event));
  }

  private scheduleReconnect(token: string) {
    if (this.reconnectTimer) return;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.connect(token);
    }, this.reconnectDelay);
  }
}

export const ws = new OneBrainWs();
