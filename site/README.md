# Mosaïk marketing site

Next.js 16 (App Router, Turbopack), React 19, Tailwind CSS 4 and TypeScript. No animation library: the demo moves with CSS transitions.

This folder is written by `mosaik.sh` at the root of the repository, which is the source of truth. Never edit these files
by hand: change the script and run it again. The folder is written afresh each time; `public/`, the Vercel link, the
installed dependencies and the last build are kept.

```bash
./mosaik.sh --dir site             # install or update, then start the dev server
./mosaik.sh --dir site --deploy    # type-check, then deploy to production on Vercel
./mosaik.sh --help                 # every option
```

## Where things live

- `lib/data.ts`: the four demo workspaces and the desktop geometry (grid slots, the messy layout).
- `components/apps.tsx`: the pretend apps drawn inside the demo (editor, terminal, browser, chat, design, notes, reader, charts, broadcast).
- `components/desk.tsx`: the interactive desktop in the hero and the feature illustrations.
- `components/header.tsx`: the header, the one part that runs in the browser besides the demo.
- `components/ui.tsx`: footer, wordmark, icons, the download button and the platform list.
- `app/`: the pages, plus the favicon (`icon.svg`, `favicon.ico`, `apple-icon.png`), `robots.txt` and `sitemap.xml`.

## Releasing

Every Download button installs Mosaïk from the Microsoft Store (`STORE_ID` in `mosaik.sh`), so deploy a version of the site
that offers a download only once the Store listing is live. What the site says the app does must match the release: the
features on `/download` and the home page, the switcher shortcut (`Win+Alt+Space`) and `/conditions`.
