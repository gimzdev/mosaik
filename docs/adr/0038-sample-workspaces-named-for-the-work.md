# 0038. Sample workspaces are named for the work

- Status: accepted
- Date: 2026-10-17

## Context

The owner, on 2026-10-17 at 00:51, of the screenshots for GitHub and the portfolio, which showed
the stand-in engine's workspace "Website": "the workspace 'website' doesnt make any fucking sense
we need generic but not that generic ... a website and a workspace are not the same thing".
Beside the site's own picture, captioned "Website", it read as the same thing. ADR 0031 had named
the samples as people name things: Website, Backend and Portfolio in the stand-in, Website in the
UI tour, "Client website" on the site.

## Decision

Sample workspaces are named for the work done in them, generic but not vague: in the stand-in
engine "Web development", "Side project" and "Brand design"; in the UI tour's profile "Web
development"; on the site "Web development" (it was "Client website"), beside "Brand refresh",
"Thesis, chapter 3" and "Friday stream". Pictures of the site are captioned "mosaïk.com", never
"Website". The window's own link to the site, in About, keeps its name.

## Consequences

- `apps/desktop/src/mock.ts`, the browser tests and `ZoneEditor.test.tsx` use the new names; the
  tour (`crates/mosaik-probe/src/ui_tour.rs`) prepares "Web development".
- The screenshots in `docs/screenshots`, `gimz/screenshots` and the portfolio were taken again with
  these names, the site's first.
