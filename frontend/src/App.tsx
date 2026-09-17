import { useEffect, useMemo, useState } from "react";
import { api, connectEvents, formatClock, protocolLabel } from "./api";
import type { Device, MediaItem, PlaybackStatus } from "./types";

const THEME_KEY = "cazt-theme";

function useTheme() {
  const [dark, setDark] = useState(() => {
    const saved = localStorage.getItem(THEME_KEY);
    if (saved) return saved === "dark";
    return window.matchMedia("(prefers-color-scheme: dark)").matches;
  });

  useEffect(() => {
    document.documentElement.classList.toggle("dark", dark);
    localStorage.setItem(THEME_KEY, dark ? "dark" : "light");
  }, [dark]);

  return { dark, setDark };
}

export default function App() {
  const { dark, setDark } = useTheme();
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<MediaItem[]>([]);
  const [devices, setDevices] = useState<Device[]>([]);
  const [scanning, setScanning] = useState(true);
  const [discoveryError, setDiscoveryError] = useState<string | null>(null);
  const [selectedMedia, setSelectedMedia] = useState<MediaItem | null>(null);
  const [selectedDevice, setSelectedDevice] = useState<string | null>(null);
  const [playback, setPlayback] = useState<PlaybackStatus | null>(null);
  const [loadingSearch, setLoadingSearch] = useState(true);
  const [casting, setCasting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [directUrl, setDirectUrl] = useState("");

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [media, deviceState] = await Promise.all([api.search(""), api.devices()]);
        if (cancelled) return;
        setItems(media);
        setDevices(deviceState.devices);
        setScanning(deviceState.scanning);
        setDiscoveryError(deviceState.last_error ?? null);
      } catch (err) {
        if (!cancelled) setError(err instanceof Error ? err.message : "Could not reach Cazt.");
      } finally {
        if (!cancelled) setLoadingSearch(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    return connectEvents((event) => {
      if (event.type === "devices") {
        setDevices(event.devices);
        setScanning(event.scanning);
        setDiscoveryError(event.last_error ?? null);
      }
      if (event.type === "playback") setPlayback(event.status);
      if (event.type === "error") setError(event.message);
    });
  }, []);

  useEffect(() => {
    const handle = window.setTimeout(async () => {
      setLoadingSearch(true);
      try {
        const media = await api.search(query);
        setItems(media);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Search failed.");
      } finally {
        setLoadingSearch(false);
      }
    }, 180);
    return () => window.clearTimeout(handle);
  }, [query]);

  const selected = useMemo(
    () => devices.find((device) => device.id === selectedDevice) ?? null,
    [devices, selectedDevice],
  );

  async function refreshDevices() {
    setScanning(true);
    setError(null);
    try {
      const next = await api.refreshDevices();
      setDevices(next.devices);
      setScanning(next.scanning);
      setDiscoveryError(next.last_error ?? null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not refresh devices.");
      setScanning(false);
    }
  }

  async function castNow(media = selectedMedia) {
    if (!media || !selected) {
      setError("Choose something to watch and a screen first.");
      return;
    }
    setCasting(true);
    setError(null);
    try {
      const status = await api.cast({
        device_id: selected.id,
        provider: media.provider,
        media_id: media.id,
        url: media.provider === "direct" ? directUrl : undefined,
      });
      setPlayback(status);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Casting failed.");
    } finally {
      setCasting(false);
    }
  }

  async function castDirect() {
    const url = directUrl.trim();
    if (!url) {
      setError("Paste a public HTTP or HLS URL first.");
      return;
    }
    const item: MediaItem = {
      id: "direct",
      provider: "direct",
      title: "Direct URL",
      playable: true,
      kind: "stream",
    };
    setSelectedMedia(item);
    await castNow(item);
  }

  async function run(action: () => Promise<PlaybackStatus>) {
    try {
      setPlayback(await action());
    } catch (err) {
      setError(err instanceof Error ? err.message : "Playback control failed.");
    }
  }

  const cinejoyEmpty = query.toLowerCase().includes("cinejoy") || query.toLowerCase().includes("cine joy");

  return (
    <div className="mx-auto min-h-dvh max-w-3xl px-4 pb-36 pt-6 sm:px-6">
      <header className="mb-8 flex items-start justify-between gap-4">
        <div>
          <div className="flex items-center gap-2">
            <Logo />
            <h1 className="text-3xl font-semibold tracking-tight">Cazt</h1>
          </div>
          <p className="mt-1 text-sm text-[var(--muted)]">Choose something. Choose a screen. Cazt it.</p>
        </div>
        <button
          type="button"
          className="rounded-full border border-[var(--line)] px-3 py-1.5 text-sm text-[var(--muted)]"
          onClick={() => setDark(!dark)}
          aria-pressed={dark}
        >
          {dark ? "Light" : "Dark"}
        </button>
      </header>

      <label className="sr-only" htmlFor="search">
        Search movies or shows
      </label>
      <input
        id="search"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        placeholder="Search movies or shows…"
        className="w-full rounded-2xl border border-[var(--line)] bg-[var(--card)] px-4 py-3 outline-none ring-[var(--accent)] focus:ring-2"
      />

      {error && (
        <p className="mt-3 rounded-xl bg-[color-mix(in_srgb,var(--accent)_12%,transparent)] px-3 py-2 text-sm" role="alert">
          {error}
        </p>
      )}

      <section className="mt-8">
        <div className="mb-3 flex items-end justify-between">
          <h2 className="text-sm font-medium uppercase tracking-[0.16em] text-[var(--muted)]">Watch</h2>
          {loadingSearch && <span className="text-xs text-[var(--muted)]">Loading…</span>}
        </div>
        {cinejoyEmpty && (
          <p className="mb-4 text-sm text-[var(--muted)]">
            cinejoy.pk has no public playback API, so Cazt will not scrape it. Use the sample films
            below, paste a URL you have the right to play, or point <code>CINEJOY_CATALOG_URL</code> at
            a permitted catalog.
          </p>
        )}
        {items.length === 0 && !loadingSearch ? (
          <EmptyState title="Nothing matched." body="Try another title, or paste a public video URL below." />
        ) : (
          <ul className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            {items.map((item) => (
              <li key={`${item.provider}-${item.id}`}>
                <button
                  type="button"
                  onClick={() => setSelectedMedia(item)}
                  className={`flex w-full items-center gap-3 rounded-2xl border px-3 py-3 text-left transition ${
                    selectedMedia?.id === item.id && selectedMedia.provider === item.provider
                      ? "border-[var(--accent)] bg-[var(--card)]"
                      : "border-[var(--line)] bg-[var(--card)]"
                  }`}
                >
                  <Poster item={item} />
                  <span>
                    <span className="block font-medium">{item.title}</span>
                    <span className="block text-sm text-[var(--muted)]">
                      {[item.year, item.subtitle].filter(Boolean).join(" · ")}
                    </span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}

        <div className="mt-4 flex gap-2">
          <input
            value={directUrl}
            onChange={(event) => setDirectUrl(event.target.value)}
            placeholder="Or paste a public .mp4 / .m3u8 URL"
            className="flex-1 rounded-2xl border border-[var(--line)] bg-[var(--card)] px-4 py-3 outline-none"
          />
          <button
            type="button"
            onClick={() => void castDirect()}
            className="rounded-2xl border border-[var(--line)] px-4 py-3 text-sm"
          >
            Use URL
          </button>
        </div>
      </section>

      <section className="mt-10">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-sm font-medium uppercase tracking-[0.16em] text-[var(--muted)]">Screen</h2>
          <button type="button" onClick={() => void refreshDevices()} className="text-sm text-[var(--accent)]">
            Refresh devices
          </button>
        </div>
        <p className="mb-3 text-sm text-[var(--muted)]">
          {scanning ? "Looking on the local network…" : devices.length ? `${devices.length} ready` : "No compatible devices yet"}
        </p>
        {discoveryError && <p className="mb-3 text-sm text-[var(--muted)]">{discoveryError}</p>}
        {devices.length === 0 && !scanning ? (
          <EmptyState
            title="Cazt cannot find a TV."
            body="Multicast has to reach this machine. Same Wi-Fi, no guest network, and on Docker Desktop set CAZT_PUBLIC_HOST plus known device IPs."
          />
        ) : (
          <ul className="space-y-2">
            {devices.map((device) => (
              <li key={device.id}>
                <button
                  type="button"
                  onClick={() => setSelectedDevice(device.id)}
                  className={`flex w-full items-center justify-between rounded-2xl border px-4 py-3 text-left ${
                    selectedDevice === device.id
                      ? "border-[var(--accent)] bg-[var(--card)]"
                      : "border-[var(--line)] bg-[var(--card)]"
                  }`}
                >
                  <span>
                    <span className="block font-medium">{device.name}</span>
                    <span className="text-sm text-[var(--muted)]">
                      {protocolLabel(device.type)} · {device.model}
                    </span>
                  </span>
                  <span className="flex items-center gap-2 text-sm text-[var(--muted)]">
                    <span
                      className="h-2 w-2 rounded-full"
                      style={{ background: device.ready ? "var(--ok)" : "var(--muted)" }}
                      aria-hidden
                    />
                    {device.ready ? "Ready" : "Away"}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <div className="mt-8">
        <p className="mb-3 text-sm text-[var(--muted)]">
          Selected: {selectedMedia?.title ?? "nothing"} → {selected?.name ?? "no screen"}
        </p>
        <button
          type="button"
          onClick={() => void castNow()}
          disabled={casting || !selectedMedia || !selected}
          className="w-full rounded-full bg-[var(--accent)] px-5 py-3 font-medium text-[var(--accent-ink)] disabled:opacity-40"
        >
          {casting ? "Connecting…" : "Cazt"}
        </button>
      </div>

      <PlaybackBar playback={playback} selected={selected} onPause={() => run(api.pause)} onResume={() => run(api.resume)} onStop={() => run(api.stop)} onSeek={(seconds) => run(() => api.seek(seconds))} onVolume={(level) => run(() => api.volume(level))} />
    </div>
  );
}

function Logo() {
  return (
    <svg width="28" height="28" viewBox="0 0 32 32" aria-hidden className="mt-1">
      <circle cx="10" cy="16" r="6" fill="var(--accent)" />
      <path d="M16 10c4 2.4 4 9.6 0 12" fill="none" stroke="var(--ink)" strokeWidth="2" />
      <path d="M20 7c6 3.6 6 14.4 0 18" fill="none" stroke="var(--ink)" strokeWidth="2" />
    </svg>
  );
}

function Poster({ item }: { item: MediaItem }) {
  if (item.poster) {
    return <img src={item.poster} alt="" className="h-14 w-10 rounded-lg object-cover" />;
  }
  return (
    <span className="grid h-14 w-10 place-items-center rounded-lg bg-[var(--line)] text-xs">
      {item.title.slice(0, 1)}
    </span>
  );
}

function EmptyState({ title, body }: { title: string; body: string }) {
  return (
    <div className="rounded-2xl border border-dashed border-[var(--line)] px-4 py-6">
      <p className="font-medium">{title}</p>
      <p className="mt-1 text-sm text-[var(--muted)]">{body}</p>
    </div>
  );
}

function PlaybackBar({
  playback,
  selected,
  onPause,
  onResume,
  onStop,
  onSeek,
  onVolume,
}: {
  playback: PlaybackStatus | null;
  selected: Device | null;
  onPause: () => void;
  onResume: () => void;
  onStop: () => void;
  onSeek: (seconds: number) => void;
  onVolume: (level: number) => void;
}) {
  if (!playback?.active && playback?.state !== "error") {
    return null;
  }
  const paused = playback.state === "paused";
  const duration = playback.duration_seconds ?? 0;
  const position = playback.position_seconds ?? 0;
  const canSeek = selected?.capabilities.seek ?? true;
  const canVolume = selected?.capabilities.volume ?? true;

  return (
    <div className="fixed inset-x-0 bottom-0 border-t border-[var(--line)] bg-[var(--card)] px-4 py-3 backdrop-blur">
      <div className="mx-auto max-w-3xl">
        <p className="text-sm">
          {playback.state === "error" ? playback.message : `Casting to ${playback.device_name ?? "device"}`}
          {playback.media ? ` · ${playback.media.title}` : ""}
        </p>
        <div className="mt-3 flex items-center justify-center gap-3">
          <Control label="Pause or resume" onClick={paused ? onResume : onPause}>
            {paused ? "Play" : "Pause"}
          </Control>
          <Control label="Stop" onClick={onStop}>
            Stop
          </Control>
        </div>
        <label className="mt-3 flex items-center gap-3 text-xs text-[var(--muted)]">
          {formatClock(position)}
          <input
            type="range"
            min={0}
            max={Math.max(duration, 1)}
            value={Math.min(position, duration || position)}
            disabled={!canSeek || !duration}
            onChange={(event) => onSeek(Number(event.target.value))}
            className="w-full accent-[var(--accent)]"
            aria-label="Seek"
          />
          {formatClock(duration)}
        </label>
        <label className="mt-2 flex items-center gap-3 text-xs text-[var(--muted)]">
          Volume
          <input
            type="range"
            min={0}
            max={1}
            step={0.01}
            value={playback.volume ?? 0.5}
            disabled={!canVolume}
            onChange={(event) => onVolume(Number(event.target.value))}
            className="w-full accent-[var(--accent)]"
            aria-label="Volume"
          />
        </label>
      </div>
    </div>
  );
}

function Control({
  children,
  onClick,
  label,
}: {
  children: string;
  onClick: () => void;
  label: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      className="rounded-full border border-[var(--line)] px-4 py-2 text-sm"
    >
      {children}
    </button>
  );
}
