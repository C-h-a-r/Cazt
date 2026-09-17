import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import type { Device, MediaItem } from "./types";

const media: MediaItem[] = [
  {
    id: "bbb",
    provider: "samples",
    title: "Big Buck Bunny",
    subtitle: "Blender Foundation",
    year: 2008,
    kind: "movie",
    playable: true,
  },
];

const devices: Device[] = [
  {
    id: "cast-1",
    name: "Living Room TV",
    type: "chromecast",
    manufacturer: "Google",
    model: "Chromecast",
    address: "192.168.1.50",
    port: 8009,
    capabilities: { play: true, pause: true, seek: true, stop: true, volume: true },
    ready: true,
  },
];

beforeEach(() => {
  vi.stubGlobal(
    "matchMedia",
    vi.fn().mockReturnValue({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() }),
  );
  vi.stubGlobal(
    "WebSocket",
    class {
      close() {}
      addEventListener() {}
      removeEventListener() {}
      set onmessage(_value: unknown) {}
    },
  );
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo) => {
      const url = String(input);
      if (url.includes("/api/media/search")) {
        return json(media);
      }
      if (url.includes("/api/devices")) {
        return json({ devices, scanning: false, last_error: null });
      }
      if (url.includes("/api/cast")) {
        return json({
          active: true,
          device_id: "cast-1",
          device_name: "Living Room TV",
          media: media[0],
          state: "playing",
          using_proxy: false,
        });
      }
      return json({});
    }),
  );
});

afterEach(() => {
  vi.unstubAllGlobals();
});

function json(body: unknown) {
  return {
    ok: true,
    status: 200,
    json: async () => body,
  };
}

it("lets a user pick media, pick a device, and cast", async () => {
  const user = userEvent.setup();
  render(<App />);

  expect(await screen.findByText("Big Buck Bunny")).toBeInTheDocument();
  await user.click(screen.getByText("Big Buck Bunny"));
  await user.click(screen.getByText("Living Room TV"));
  await user.click(screen.getByRole("button", { name: "Cazt" }));

  await waitFor(() => {
    expect(fetch).toHaveBeenCalledWith(
      "/api/cast",
      expect.objectContaining({
        method: "POST",
      }),
    );
  });
});
