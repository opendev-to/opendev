import type { WSMessage } from '../types';

export type WSEventHandler = (message: WSMessage) => void;

class WebSocketClient {
  private ws: WebSocket | null = null;
  private handlers: Map<string, Set<WSEventHandler>> = new Map();
  private reconnectTimer: number | null = null;
  private reconnectAttempts = 0;
  private reconnectDelay = 1000;
  private maxReconnectDelay = 30000;
  private intentionalClose = false;
  private pingInterval: number | null = null;
  private visibilityListenerAdded = false;
  private lastSeq: number = 0;
  private seenSeqs: Set<number> = new Set();

  connect() {
    // Prevent multiple connections
    if (this.ws && (this.ws.readyState === WebSocket.CONNECTING || this.ws.readyState === WebSocket.OPEN)) {
      console.log('WebSocket already connecting or connected');
      return;
    }

    this.intentionalClose = false;

    // Use proxy in development, or direct connection in production
    const isDev = import.meta.env.DEV;
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';

    // In dev, connect directly to backend to avoid Vite HMR WebSocket conflicts
    // In prod, use the same host (since static files are served by backend)
    const wsUrl = isDev
      ? 'ws://localhost:8080/ws'  // Direct connection in dev
      : `${protocol}//${window.location.host}/ws`;  // Relative in prod

    console.log('Connecting to WebSocket:', wsUrl, `(dev mode: ${isDev})`);

    try {
      this.ws = new WebSocket(wsUrl);

      this.ws.onopen = () => {
        console.log('WebSocket connected successfully');
        this.reconnectAttempts = 0;
        this.emit({ type: 'connected', data: {} });
        this.startHeartbeat();

        // Request catch-up for messages missed during disconnection
        if (this.lastSeq > 0) {
          console.log(`Requesting catch-up from seq ${this.lastSeq}`);
          this.send({ type: 'sync', data: { last_seq: this.lastSeq } });
        }
      };

      this.ws.onmessage = (event) => {
        try {
          const message: WSMessage = JSON.parse(event.data);

          // Track sequence numbers for catch-up on reconnect
          const seq = (message as any).seq;
          if (typeof seq === 'number') {
            // Deduplicate: skip messages we have already processed
            if (this.seenSeqs.has(seq)) {
              return;
            }
            this.seenSeqs.add(seq);
            if (seq > this.lastSeq) {
              this.lastSeq = seq;
            }
            // Prune old entries to avoid unbounded growth
            if (this.seenSeqs.size > 5000) {
              const cutoff = this.lastSeq - 2500;
              const pruned = new Set<number>();
              for (const s of this.seenSeqs) {
                if (s >= cutoff) pruned.add(s);
              }
              this.seenSeqs = pruned;
            }
          }

          this.emit(message);
        } catch (error) {
          console.error('Failed to parse WebSocket message:', error);
        }
      };

      this.ws.onerror = (error) => {
        console.error('WebSocket error:', error);
      };

      this.ws.onclose = (event) => {
        console.log('WebSocket disconnected:', event.code, event.reason);
        this.ws = null;
        this.stopHeartbeat();
        this.emit({ type: 'disconnected', data: {} });

        if (!this.intentionalClose) {
          this.attemptReconnect();
        }
      };
    } catch (error) {
      console.error('Failed to create WebSocket:', error);
      this.attemptReconnect();
    }

    // Register visibility change listener once
    if (!this.visibilityListenerAdded) {
      this.visibilityListenerAdded = true;
      document.addEventListener('visibilitychange', () => {
        if (document.visibilityState === 'visible' && !this.intentionalClose) {
          if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
            console.log('Tab became visible, reconnecting WebSocket...');
            this.connect();
          }
        }
      });
    }
  }

  disconnect() {
    this.intentionalClose = true;
    this.stopHeartbeat();
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }

  send(message: any) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(message));
    } else {
      console.warn('WebSocket is not connected');
    }
  }

  ping() {
    this.send({ type: 'ping', data: { timestamp: Date.now() } });
  }

  on(eventType: string, handler: WSEventHandler) {
    if (!this.handlers.has(eventType)) {
      this.handlers.set(eventType, new Set());
    }
    this.handlers.get(eventType)!.add(handler);

    // Return unsubscribe function
    return () => {
      const handlers = this.handlers.get(eventType);
      if (handlers) {
        handlers.delete(handler);
      }
    };
  }

  private emit(message: WSMessage) {
    // Emit to specific type handlers
    const typeHandlers = this.handlers.get(message.type);
    if (typeHandlers) {
      typeHandlers.forEach(handler => handler(message));
    }

    // Emit to wildcard handlers
    const wildcardHandlers = this.handlers.get('*');
    if (wildcardHandlers) {
      wildcardHandlers.forEach(handler => handler(message));
    }
  }

  private startHeartbeat() {
    this.stopHeartbeat();
    this.pingInterval = window.setInterval(() => {
      this.ping();
    }, 30000);
  }

  private stopHeartbeat() {
    if (this.pingInterval) {
      clearInterval(this.pingInterval);
      this.pingInterval = null;
    }
  }

  private attemptReconnect() {
    // Clear any existing reconnect timer
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }

    this.reconnectAttempts++;
    const delay = Math.min(
      this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1),
      this.maxReconnectDelay
    );

    console.log(`Attempting to reconnect in ${delay}ms (attempt ${this.reconnectAttempts})`);

    this.reconnectTimer = window.setTimeout(() => {
      this.connect();
    }, delay);
  }
}

export const wsClient = new WebSocketClient();
