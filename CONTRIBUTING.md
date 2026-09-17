# Contributing to Cazt

Thanks for helping. Cazt is intentionally small: pick media, pick a screen, cast it.

## Development

```bash
cp .env.example .env
make dev
```

Or run the two processes separately:

```bash
make server
make web
```

## Checks

```bash
make lint
make test
make build
```

CI runs the same commands and does not require a physical Chromecast or TV.
Protocol code is covered with mocks and unit tests around normalization,
deduplication, media sessions, and SSRF.

## Adding a device protocol

1. Implement `DiscoveryProvider` in `server/src/discovery`.
2. Implement `CastProvider` in `server/src/cast`.
3. Only list a device when Cazt can actually control it.
4. Register both in the default provider lists.
5. Add tests that do not need hardware.

## Adding a media source

Implement `MediaProvider`. Do not scrape sites, bypass DRM, or fetch
arbitrary private URLs. Playable URLs must pass the SSRF checks.

## Pull requests

- Keep the UI a consumer utility, not an admin dashboard.
- Prefer small, documented network/protocol changes.
- Update the README only for behavior that actually shipped.
