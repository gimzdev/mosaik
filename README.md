# Mosaïk marketing site

Next.js 16 (App Router, Turbopack), React 19, Tailwind CSS 4 and TypeScript. No animation library: the demo moves with CSS transitions.

This folder is written by `v1.sh`, which is the source of truth. Change the settings at the top of that script and run it
again; the site updates in place and only changed files are touched.

```bash
./v1.sh              # install or update, then start the dev server
./v1.sh --deploy     # type-check, then deploy to production on Vercel
./v1.sh --help       # every option
```

## Where things live

- `lib/data.ts`: the four demo workspaces and the desktop geometry (grid slots, the messy layout).
- `components/apps.tsx`: the pretend apps drawn inside the demo (editor, terminal, browser, chat, design, notes, reader, charts, broadcast).
- `components/desk.tsx`: the interactive desktop in the hero, the feature illustrations and the workspace thumbnails.
- `components/ui.tsx`: header, footer, wordmark, icons and the platform list.
- `app/`: the pages, plus the favicon (`icon.svg`, `favicon.ico`, `apple-icon.png`), `robots.txt` and `sitemap.xml`.

## Before launch

- Set `SITE_URL` in `v1.sh` to the real domain.
- Have the copy on `/conditions` reviewed before the app collects any data.
