import type {
  ApiError,
  DevicesResponse,
  MediaItem,
  PlaybackStatus,
  ProviderInfo,
  ServerEvent,
} from "./types";

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    ...init,
    headers: {
      "content-type": "application/json",
      ...(init?.headers ?? {}),
    },
  });
  if (!response.ok) {
    let message = `Request failed (${response.status})`;
    try {
      const body = (await response.json()) as ApiError;
      message = body.error?.message || message;
    } catch {
      /* ignore non-json */
    }
    throw new Error(message);
  }
  if (response.status === 204) {
    return undefined as T;
  }
  return (await response.json()) as T;
}

export const api = {
  health: () => request<{ status: string }>("/api/health"),
  devices: () => request<DevicesResponse>("/api/devices"),
  refreshDevices: () => request<DevicesResponse>("/api/devices/refresh", { method: "POST" }),
  providers: () => request<ProviderInfo[]>("/api/media/providers"),
  search: (q: string, provider?: string) => {
    const params = new URLSearchParams();
    if (q) params.set("q", q);
    if (provider) params.set("provider", provider);
    return request<MediaItem[]>(`/api/media/search?${params.toString()}`);
  },
  cast: (body: {
    device_id: string;
    provider: string;
    media_id: string;
    url?: string;
    proxy?: boolean;
  }) => request<PlaybackStatus>("/api/cast", { method: "POST", body: JSON.stringify(body) }),
  pause: () => request<PlaybackStatus>("/api/playback/pause", { method: "POST" }),
  resume: () => request<PlaybackStatus>("/api/playback/resume", { method: "POST" }),
  stop: () => request<PlaybackStatus>("/api/playback/stop", { method: "POST" }),
  seek: (seconds: number) =>
    request<PlaybackStatus>("/api/playback/seek", {
      method: "POST",
      body: JSON.stringify({ seconds }),
    }),
  volume: (level: number) =>
    request<PlaybackStatus>("/api/playback/volume", {
      method: "POST",
      body: JSON.stringify({ level }),
    }),
  status: () => request<PlaybackStatus>("/api/playback/status"),
};

export function connectEvents(onEvent: (event: ServerEvent) => void): () => void {
  const protocol = window.location.protocol === "https:" ? "wss" : "ws";
  const socket = new WebSocket(`${protocol}://${window.location.host}/api/events`);
  socket.onmessage = (message) => {
    try {
      onEvent(JSON.parse(message.data) as ServerEvent);
    } catch {
      /* ignore */
    }
  };
  return () => socket.close();
}

export function formatClock(total?: number | null): string {
  if (total == null || Number.isNaN(total)) return "0:00";
  const seconds = Math.max(0, Math.floor(total));
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return `${m}:${String(s).padStart(2, "0")}`;
}

export function protocolLabel(type: string): string {
  return type === "chromecast" ? "Chromecast" : "DLNA";
}
