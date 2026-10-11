# 0001. One repository for the app, the site and the docs

- Status: accepted
- Date: 2026-10-11

## Context

`gimzdev/mosaik` held only the marketing site, generated at the repository root by the site
script. The app, its docs and its CI now need a home, and the site has to keep matching the app
(download page, hotkey, privacy terms).

## Decision

- The site moves to `site/`. It is still written only by `mosaik1.sh`, which now lives at the
  repository root and defaults to `site/` when run there. The move was done with `git mv`, so
  the site's history follows it.
- The repository root holds the Rust workspace (`crates/`), the Tauri app (`apps/desktop`), the
  docs (`docs/`), the installer config (`installer/`) and CI (`.github/workflows`, with path
  filters so site changes and app changes run separate checks).
- The root `README.md` describes the project; the generated `site/README.md` describes the site.

## Consequences

- Vercel must build from `site/`. Deploys made with `./mosaik1.sh --dir site --deploy` already do
  (the Vercel link lives in `site/.vercel`). If the Vercel project also builds on push, its Root
  Directory setting must be changed to `site` by the owner.
- CI checks that `site/` is exactly what `mosaik1.sh` generates, so the site cannot drift from its
  source of truth.
