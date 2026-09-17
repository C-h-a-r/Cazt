export type DeviceType = "chromecast" | "dlna";

export type Capabilities = {
  play: boolean;
  pause: boolean;
  seek: boolean;
  stop: boolean;
  volume: boolean;
};

export type Device = {
  id: string;
  name: string;
  type: DeviceType;
  manufacturer: string;
  model: string;
  address: string;
  port: number;
  capabilities: Capabilities;
  location?: string | null;
  ready: boolean;
};

export type MediaItem = {
  id: string;
  provider: string;
  title: string;
  subtitle?: string | null;
  year?: number | null;
  kind: "movie" | "short" | "trailer" | "stream";
  poster?: string | null;
  duration_seconds?: number | null;
  playable: boolean;
};

export type PlaybackState =
  | "idle"
  | "connecting"
  | "playing"
  | "paused"
  | "buffering"
  | "stopped"
  | "error";

export type PlaybackStatus = {
  active: boolean;
  device_id?: string | null;
  device_name?: string | null;
  media?: MediaItem | null;
  state: PlaybackState;
  position_seconds?: number | null;
  duration_seconds?: number | null;
  volume?: number | null;
  muted?: boolean | null;
  using_proxy: boolean;
  message?: string | null;
};

export type DevicesResponse = {
  devices: Device[];
  scanning: boolean;
  last_error?: string | null;
};

export type ProviderInfo = {
  id: string;
  name: string;
  description: string;
};

export type ApiError = {
  error: {
    code: string;
    message: string;
  };
};

export type ServerEvent =
  | { type: "devices"; devices: Device[]; scanning: boolean; last_error?: string | null }
  | { type: "playback"; status: PlaybackStatus }
  | { type: "error"; code: string; message: string };
