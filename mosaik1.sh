#!/usr/bin/env bash
if grep -q $'\r' "$0" 2>/dev/null; then MK_SCRIPT="$0" MK_COPY="${TMPDIR:-/tmp}/mosaik-site.$$.sh"; export MK_SCRIPT MK_COPY; tr -d '\r' < "$0" > "$MK_COPY" && exec bash "$MK_COPY" "$@"; fi # Windows line endings: run a copy without them
# ════════════════════════════════════════════════════════════════════════
#  Mosaïk marketing site · mosaik1.sh
#
#  One file that writes the whole site, installs it and runs it:
#    Next.js 16 (Turbopack) · React 19 · Tailwind CSS 4 · TypeScript
#
#  Usage
#    ./mosaik1.sh             install or update, then start the dev server
#    ./mosaik1.sh --deploy    type-check, then deploy to production on Vercel
#    ./mosaik1.sh --preview   same, but to a shareable preview URL
#    ./mosaik1.sh --prod      build and serve the production build locally
#    ./mosaik1.sh --build     install and build only (CI)
#    ./mosaik1.sh --no-start  install, don't start anything
#    ./mosaik1.sh --fresh     reinstall dependencies, rebuild from scratch
#    ./mosaik1.sh --port 4000 another port (default 3000, or the next free one)
#    ./mosaik1.sh --dir ~/m   install somewhere else (default ./mosaik_site)
#    ./mosaik1.sh --deploy --app   also send the app to the Microsoft Store
#    ./mosaik1.sh --app       only send the app (Windows: it is tested, built
#                             and packaged first)
#
#  Updating
#    This file is the source of truth. Change the settings below and run it
#    again: the site's files are written afresh (any it no longer makes are
#    gone), while public/, your Vercel link, the installed dependencies and
#    the last build are kept, so only what changed is installed or built.
#
#  Deploying
#    The first --deploy asks you to sign in to Vercel (or export
#    VERCEL_TOKEN, plus VERCEL_SCOPE for a team), creates the project named
#    below and ships it to production. Point your domain at it once with:
#      npx vercel domains add <your-domain>
#
#  The app (--app)
#    Runs the app's own mosaik.ps1 publish: the repository's (./mosaik/) when
#    it is beside this file, else the one-file mosaik.ps1 beside it. It tests,
#    builds and packages the app and sends it to the Microsoft Store, which
#    checks it (usually within a day or two) before everyone gets it. The
#    first time, it asks for the Store's key (the app's docs/RELEASE.md says
#    where Partner Center makes one) and keeps it, encrypted; after that
#    there is nothing to answer.
# ════════════════════════════════════════════════════════════════════════
set -Eeuo pipefail
# The copy made for Windows line endings (line 2) goes when the script ends.
cleanup() { [ "${MK_COPY:-}" != "$0" ] || rm -f "$0"; }
trap cleanup EXIT

# ── Your settings ────────────────────────────────────────────────────────
SITE_NAME="Mosaïk"
SITE_URL="https://xn--mosak-fta.com"     # your real domain: metadata, robots.txt and the sitemap use it
CONTACT_EMAIL="contact@gimzdev.com"
REPO_URL="https://github.com/gimzdev/mosaik"
PATREON_URL="https://www.patreon.com/gimzdev"
STORE_ID="9PHR0L7S1QVD"                  # Mosaïk in the Microsoft Store: every Download button installs it
VERCEL_PROJECT="mosaik"

MK_VERSION="1.10.1"

# ── Output ───────────────────────────────────────────────────────────────
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  C0=$'\033[0m'; CD=$'\033[2m'; CB=$'\033[1m'; CA=$'\033[38;5;63m'; CY=$'\033[38;5;179m'; CR=$'\033[38;5;167m'; CG=$'\033[38;5;72m'
else
  C0=''; CD=''; CB=''; CA=''; CY=''; CR=''; CG=''
fi
step() { printf '%s▸%s %s\n' "$CA" "$C0" "$*"; }
ok()   { printf '%s✔%s %s\n' "$CG" "$C0" "$*"; }
note() { printf '%s  %s%s\n' "$CD" "$*" "$C0"; }
warn() { printf '%s!%s %s\n' "$CY" "$C0" "$*" >&2; }
die()  { printf '%s✖ %s%s\n' "$CR" "$*" "$C0" >&2; exit 1; }
trap 'printf "%s✖ Stopped at line %s: %s%s\n" "$CR" "$LINENO" "$BASH_COMMAND" "$C0" >&2' ERR

SCRIPT_PATH="${MK_SCRIPT:-${BASH_SOURCE[0]:-$0}}"
SCRIPT_NAME="${SCRIPT_PATH##*/}"
usage() { awk '/^#  Usage$/ { on = 1 } on && /^#$/ { exit } on { sub(/^#  ?/, ""); print }' "$0" | sed "s#\./mosaik1\.sh#./$SCRIPT_NAME#"; exit 0; }

# ── Options ──────────────────────────────────────────────────────────────
APP_DIR=""; PORT="${PORT:-3000}"; PORT_SET=0; MODE="dev"; START=1; FRESH=0; DEPLOY=0; PREVIEW=0
APP=0; SITE=0   # --app: send the app to the Store; SITE: an option about the site was given too
while [ $# -gt 0 ]; do
  case "$1" in
    --dir) [ $# -ge 2 ] || die "--dir needs a path"; APP_DIR="$2"; SITE=1; shift 2 ;;
    --dir=*) APP_DIR="${1#*=}"; SITE=1; shift ;;
    --port) [ $# -ge 2 ] || die "--port needs a number"; PORT="$2"; PORT_SET=1; SITE=1; shift 2 ;;
    --port=*) PORT="${1#*=}"; PORT_SET=1; SITE=1; shift ;;
    --prod) MODE="prod"; SITE=1; shift ;;
    --build) MODE="prod"; START=0; SITE=1; shift ;;
    --no-start) START=0; SITE=1; shift ;;
    --fresh) FRESH=1; SITE=1; shift ;;
    --deploy) DEPLOY=1; SITE=1; shift ;;
    --preview) DEPLOY=1; PREVIEW=1; SITE=1; shift ;;
    --app) APP=1; shift ;;
    -h|--help) usage ;;
    *) die "Unknown option: $1 (try --help)" ;;
  esac
done
case "$PORT" in ''|*[!0-9]*) die "Port must be a number, got \"$PORT\"" ;; esac
[ "$PORT" -ge 1 ] && [ "$PORT" -le 65535 ] || die "Port must be between 1 and 65535"
case "$SITE_URL" in https://*) SITE_URL="${SITE_URL%/}" ;; *) die "SITE_URL must start with https:// (got \"$SITE_URL\")" ;; esac
case "$PATREON_URL" in https://*) ;; *) die "PATREON_URL must start with https:// (got \"$PATREON_URL\")" ;; esac
case "$STORE_ID" in ????????????) ;; *) die "STORE_ID is the 12-character Store ID from Partner Center (got \"$STORE_ID\")" ;; esac
case "$STORE_ID" in *[!0-9A-Z]*) die "STORE_ID has only capital letters and digits (got \"$STORE_ID\")" ;; esac
case "$VERCEL_PROJECT" in ''|*[!a-z0-9._-]*) die "VERCEL_PROJECT may only use lowercase letters, digits, '.', '_' and '-'" ;; esac

printf '\n%s%s  %s site%s  %s%s %s%s\n\n' "$CB" "$CA" "$SITE_NAME" "$C0" "$CD" "$SCRIPT_NAME" "$MK_VERSION" "$C0"

# ── The app (--app) ──────────────────────────────────────────────────────
# Tested, built and packaged by the app's own mosaik.ps1, then sent to the Microsoft Store (its
# "publish"). Before the site: when the app is not sent, nothing else is done.
if [ "$APP" = 1 ]; then
  command -v powershell.exe >/dev/null 2>&1 ||
    die "--app builds Mosaïk and sends it to the Microsoft Store: run it on Windows (Git Bash), where Mosaïk builds."
  HERE="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
  APP_PS1=""
  if [ -f "$HERE/mosaik/Cargo.toml" ] && [ -f "$HERE/mosaik/mosaik.ps1" ]; then APP_PS1="$HERE/mosaik/mosaik.ps1"
  elif [ -f "$HERE/mosaik.ps1" ]; then APP_PS1="$HERE/mosaik.ps1"
  else die "--app needs the app beside this file: its repository (mosaik/) or the one-file mosaik.ps1."; fi
  step "Sending Mosaïk to the Microsoft Store with $APP_PS1"
  note "Its tests, build and package first (a few minutes), then the Store."
  PUBLISH=(powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(cygpath -w "$APP_PS1" 2>/dev/null || printf '%s' "$APP_PS1")" publish)
  # mintty (Git Bash's own window) is no Windows console: winpty gives PowerShell one, so it can ask.
  if [ "${TERM_PROGRAM:-}" = "mintty" ] && command -v winpty >/dev/null 2>&1; then PUBLISH=(winpty "${PUBLISH[@]}"); fi
  "${PUBLISH[@]}" || die "Mosaïk was not sent to the Store; the lines above say why. Nothing else was done."
  ok "The app's part is done"
  [ "$SITE" = 1 ] || exit 0
fi

# ── Requirements ─────────────────────────────────────────────────────────
command -v node >/dev/null 2>&1 || die "Node.js 20.9 or newer is required (https://nodejs.org)."
command -v npm >/dev/null 2>&1 || die "npm is required (it ships with Node.js)."
NODE_VERSION="$(node -p 'process.versions.node')"
node -e 'const [a, b] = process.versions.node.split(".").map(Number); process.exit(a > 20 || (a === 20 && b >= 9) ? 0 : 1)' ||
  die "Node.js $NODE_VERSION is too old. Next.js 16 needs 20.9 or newer."
ok "Node.js $NODE_VERSION, npm $(npm -v)"

# ── Where to install ─────────────────────────────────────────────────────
MARKER=".mosaik-site"
if [ -z "$APP_DIR" ]; then
  if [ -f "$MARKER" ]; then APP_DIR="."
  else APP_DIR="./mosaik_site"; fi
fi
mkdir -p "$(dirname "$APP_DIR")"
APP_DIR="$(cd "$(dirname "$APP_DIR")" && pwd -P)/$(basename "$APP_DIR")"
case "$APP_DIR" in */.) APP_DIR="${APP_DIR%/.}" ;; esac
case "$APP_DIR" in ""|/|"$HOME") die "Refusing to install into '$APP_DIR'." ;; esac

# An existing site is replaced: its files are written afresh, and any it no longer makes are gone.
# Kept: public/ and the Vercel link, and (unless --fresh) the installed dependencies and the last
# build and type-check, so a rerun only installs, builds or checks what changed.
INSTALL="fresh"
if [ -d "$APP_DIR" ] && [ -n "$(ls -A "$APP_DIR")" ]; then
  # Only a folder this script wrote is emptied: never one of yours, nor the app's source code.
  [ -f "$APP_DIR/$MARKER" ] ||
    die "$APP_DIR has files this script did not write: nothing was removed. Use --dir with a new or empty folder."
  for entry in "$APP_DIR"/* "$APP_DIR"/.[!.]* "$APP_DIR"/..?*; do
    [ -e "$entry" ] || [ -L "$entry" ] || continue
    case "${entry##*/}" in
      public|.vercel|"$MARKER") continue ;;
      node_modules|package-lock.json|.next|next-env.d.ts|*.tsbuildinfo) [ "$FRESH" = 1 ] || continue ;;
    esac
    rm -rf "$entry" || die "Could not remove $entry (a program has files open in it: close it and run again)."
  done
  INSTALL="replaced the old one"
fi
mkdir -p "$APP_DIR"
cd "$APP_DIR"
step "Writing the site into $APP_DIR ($INSTALL)"

rmdir public 2>/dev/null || true

FILES=0
# put FILE < content: writes FILE.
put() {
  case "$1" in */*) mkdir -p "${1%/*}" ;; esac
  cat > "$1"
  FILES=$((FILES + 1))
}
# put_b64 FILE < base64: the same for binary files (the favicon), decoded where it is written.
put_b64() {
  put "$1"
  node -e 'const fs = require("fs"); fs.writeFileSync(process.argv[1], Buffer.from(fs.readFileSync(process.argv[1], "utf8"), "base64"))' "$1"
}

# ── Project files ────────────────────────────────────────────────────────
# shellcheck disable=SC2016 # JavaScript template literals, not shell expansions
node -e '
const [name, url, email, repo, patreon, store, script] = process.argv.slice(1);
// One click downloads the Store web installer, which installs Mosaïk and starts it.
const download = `https://get.microsoft.com/installer/download/${store}?referrer=appbadge&source=${new URL(url).hostname}`;
const storePage = `https://apps.microsoft.com/detail/${store}`;
const site = { name, url, email, repo, patreon, download, storePage };
process.stdout.write(`// Generated from the settings at the top of ${script}. Edit them there.\nexport const site = ${JSON.stringify(site, null, 2)} as const;\n`);
' "$SITE_NAME" "$SITE_URL" "$CONTACT_EMAIL" "$REPO_URL" "$PATREON_URL" "$STORE_ID" "$SCRIPT_NAME" > lib.site.mk-gen
put lib/site.ts < lib.site.mk-gen
rm -f lib.site.mk-gen

put .gitignore <<'__MK_EOF__'
node_modules
.next
.vercel
.env*.local
*.tsbuildinfo
next-env.d.ts
.DS_Store
__MK_EOF__
put .vercelignore <<'__MK_EOF__'
node_modules
.next
.env*
*.tsbuildinfo
__MK_EOF__
put README.md <<'__MK_EOF__'
# Mosaïk marketing site

Next.js 16 (App Router, Turbopack), React 19, Tailwind CSS 4 and TypeScript. No animation library: the demo moves with CSS transitions.

This folder is written by `mosaik1.sh` at the root of the repository, which is the source of truth. Never edit these files
by hand: change the script and run it again. The folder is written afresh each time; `public/`, the Vercel link, the
installed dependencies and the last build are kept.

```bash
./mosaik1.sh --dir site             # install or update, then start the dev server
./mosaik1.sh --dir site --deploy    # type-check, then deploy to production on Vercel
./mosaik1.sh --help                 # every option
```

## Where things live

- `lib/data.ts`: the four demo workspaces and the desktop geometry (grid slots, the messy layout).
- `components/apps.tsx`: the pretend apps drawn inside the demo (editor, terminal, browser, chat, design, notes, reader, charts, broadcast).
- `components/desk.tsx`: the interactive desktop in the hero, the feature illustrations and the workspace thumbnails.
- `components/header.tsx`: the header, the one part that runs in the browser besides the demo.
- `components/ui.tsx`: footer, wordmark, icons, the download button and the platform list.
- `app/`: the pages, plus the favicon (`icon.svg`, `favicon.ico`, `apple-icon.png`), `robots.txt` and `sitemap.xml`.

## Releasing

Every Download button installs Mosaïk from the Microsoft Store (`STORE_ID` in `mosaik1.sh`), so deploy a version of the site
that offers a download only once the Store listing is live. What the site says the app does must match the release: the
features on `/download` and the home page, the switcher shortcut (`Win+Alt+Space`) and `/conditions`.
__MK_EOF__
put app/apple-icon.tsx <<'__MK_EOF__'
import { ImageResponse } from 'next/og';

export const size = { width: 180, height: 180 };
export const contentType = 'image/png';

/** The favicon's four tiles on the site background, for home screens (iOS fills transparency with black). */
export default function AppleIcon() {
  const tile = (left: number, top: number, w: number, h: number, background: string) => (
    <div style={{ position: 'absolute', left: 30 + left * 3.75, top: 30 + top * 3.75, width: w * 3.75, height: h * 3.75, borderRadius: 11.25, background }} />
  );
  return new ImageResponse(
    (
      <div style={{ display: 'flex', width: '100%', height: '100%', background: '#EDEFEA' }}>
        {tile(2, 2, 16, 16, '#2B44D9')}
        {tile(21, 2, 9, 9, '#F4B13A')}
        {tile(21, 14, 9, 16, '#12786A')}
        {tile(2, 21, 16, 9, '#C93450')}
      </div>
    ),
    size,
  );
}
__MK_EOF__
put app/conditions/page.tsx <<'__MK_EOF__'
import type { Metadata } from 'next';
import { site } from '@/lib/site';

export const metadata: Metadata = {
  title: 'Privacy and terms',
  description: 'Mosaïk sends nothing anywhere. How the app, its updates and this website handle your data, and the terms it is released under.',
};

export default function ConditionsPage() {
  const mail = <a href={`mailto:${site.email}`}>{site.email}</a>;
  const ext = (href: string, label: string) => (
    <a href={href} rel="noopener noreferrer">
      {label}
    </a>
  );
  return (
    <div className="page-x pb-24 pt-32 sm:pt-40">
      <h1 className="h1">Privacy and terms</h1>
      <p className="mt-5 text-slate">Last updated October 12, 2026.</p>
      <div className="prose-page mt-8 max-w-2xl">
        <h2>The app</h2>
        <p>
          Mosaïk makes no network requests. It has no telemetry, no account and no cloud: your workspaces, layouts and settings stay on your
          computer, in Mosaïk’s own folder.
        </p>
        <p>
          It doesn’t check for updates itself. The Microsoft Store installs and updates it, as it does every Store app, and the Download
          buttons on this site fetch Microsoft’s installer from get.microsoft.com. Microsoft’s{' '}
          {ext('https://privacy.microsoft.com/privacystatement', 'privacy statement')} covers both.
        </p>
        <p>
          The Support on Patreon links, here and in Mosaïk’s About page and tray menu, open {ext(site.patreon, 'Patreon')} in your browser. The
          app itself sends nothing to Patreon.
        </p>
        <p>If a release ever needs to send something, this page will say exactly what and why before that release ships.</p>
        <h2>This website</h2>
        <p>It sets no cookies and runs no analytics or advertising trackers.</p>
        <p>If you email {mail}, we receive your address and your message, and use them only to reply. We don’t sell or share them.</p>
        <h2>Licence</h2>
        <p>
          Mosaïk is released under CC0 1.0, a public-domain dedication. You can use, copy, change and distribute it for any purpose, with no
          attribution required. The open-source components it is built with keep their own licences, listed in the app’s About page.
        </p>
        <h2>Warranty</h2>
        <p>Mosaïk is provided as is, without warranty of any kind. Use it at your own risk.</p>
        <h2>Using it well</h2>
        <ul>
          <li>Don’t use Mosaïk for anything illegal.</li>
          <li>Be kind in issues, discussions and pull requests.</li>
        </ul>
        <h2>Questions</h2>
        <p>
          Write to {mail} or open an issue on{' '}
          <a href={site.repo} rel="noopener noreferrer">
            GitHub
          </a>
          .
        </p>
      </div>
    </div>
  );
}
__MK_EOF__
put app/download/page.tsx <<'__MK_EOF__'
import type { Metadata } from 'next';
import { DownloadButton } from '@/components/ui';
import { site } from '@/lib/site';

export const metadata: Metadata = {
  title: 'Download for Windows',
  description: 'Download Mosaïk for Windows 11 and Windows 10: free, installed and kept up to date by the Microsoft Store.',
};

const NOTES: [string, React.ReactNode][] = [
  ['Requirements', 'Windows 11, or Windows 10 version 22H2, on a 64-bit (x64) PC.'],
  ['Privacy', 'Mosaïk never connects to the internet. Your layouts stay on your PC.'],
  ['Uninstall', 'Settings, Apps, Installed apps, then Uninstall next to Mosaïk.'],
  [
    'Source',
    <>
      On{' '}
      <a href={site.repo} rel="noopener noreferrer" className="font-semibold text-lapis underline underline-offset-4">
        GitHub
      </a>
      , in the public domain (CC0).
    </>,
  ],
];

export default function DownloadPage() {
  return (
    <div className="page-x pb-24 pt-32 sm:pt-40">
      <h1 className="h1">Download Mosaïk for Windows.</h1>
      <p className="mt-5 max-w-xl text-lg text-slate">
        Free. The download is Microsoft’s small Store installer: it installs Mosaïk from the Microsoft Store, which keeps it up to date.
      </p>
      <div className="mt-8 flex flex-wrap gap-3">
        <DownloadButton className="btn btn-lapis" />
        <a href={site.storePage} rel="noopener noreferrer" className="btn btn-line">
          See it in the Microsoft Store
        </a>
      </div>
      <dl className="mt-16 grid max-w-3xl gap-x-10 gap-y-5 border-t border-grout pt-10 sm:grid-cols-[9rem_1fr]">
        {NOTES.map(([t, d]) => (
          <div key={t} className="contents">
            <dt className="font-semibold">{t}</dt>
            <dd className="-mt-4 text-[16px] leading-relaxed text-slate sm:mt-0">{d}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
__MK_EOF__
put_b64 app/favicon.ico <<'__MK_EOF__'
AAABAAMAEBAAAAAAIAAlAQAANgAAACAgAAAAACAAlAEAAFsBAAAwMAAAAAAgAAsDAADvAgAAiVBO
Rw0KGgoAAAANSUhEUgAAABAAAAAQCAYAAAAf8/9hAAAA7ElEQVR4nK2SoU4DQRRFz5uuI7hii6zA
LYji0RvSP8Ayf8AX9Ac6n0Gb1dVgAFeBrSVVDXLfQ7SzaTadzRC48r53biZzH/xRAnB19zkHHhM7
Yb0a++96Upq5BTACECGcVS/eHZZScDvTxlURBjDb+y4BZSs/QOyr42wAilz+/P51vlvegskFgBto
/auAGNL1sgOGT75U1QXIvgUI21loWwg9bABQtSrCAHZopwBYr8Ye8LmvOVYB8HY9LRvR6tTCwFx9
8/780RvQiLYX1lUj+gBcpgLiH5yEM2b/dIli6RbizDmpwTat399cvn4AiJRBGV+LQGwAAAAASUVO
RK5CYIKJUE5HDQoaCgAAAA1JSERSAAAAIAAAACAIBgAAAHN6evQAAAFbSURBVHic7ZaxcQIxEEX/
Ms4YIoogICTxOHYB1wANiAYogQZ0DVwDFEDMkDgkcBGONMTrRNZIJ6GVD+TzYL/sbrW7/3Ra7QJ/
HfIflq/vGsAawKzQ3wDozofFpm+47F80UxTLEKObNke33gmwydU3xPu0vgib/GosYrRfIibe+/XA
5JGv/fKr+HZfQOm2p+j7SrGcfZJb9RM8rABTaq8igBhdqb2KgGlz3BCjRbwTxi9BILwH+Jak58OC
5FUxD3sIi3mqFXi+VZrjvmII6D52rTsD/g5IpZMj8LXJFRI3JANqvlU6JSBbOgKBLwt9xbc7Abab
pUonh0GvE1qKe0FwBmygqLfXZPQqGF1A8AtOq0ZTPEblMMzont/2g3+b2wGbPFU6OWZEUKdVo+Wl
ggASxqgct/jWGskGCRiFXyXgbr2gIFY8krEwRuXo+5LQV3x7MMXc8x4obcf/fAIVc3L9/IeUbAAA
AABJRU5ErkJggolQTkcNChoKAAAADUlIRFIAAAAwAAAAMAgGAAAAVwL5hwAAAtJJREFUeJztmb9P
E2Ecxp/nrURI0IHGSRPGDrghNIQAMYGkLp51I644iHFhsfwFnIsLCQ6wd7MQB00gIa2pTRvcZOh4
CSsMSJSEcF8HMPR+ld4vT819tvfee5/7Pve+d7nnPSAlJRR0Ozgy214BZB7gcDh5MQCW93dyy93O
OtmaKABcAjAFoN8pAwNgefBJ3aHjMHB/rl0VwXSIqh2QqH3bzs249R1vTi4oynqPUrVB7YtFR3U2
RmbbK1EXDwAimL6YVSvH78ezirLqQ2r6ZHPSoqOs/TIfqMKecNFWfUW4LRkfOjYDYdd8N5zaGcpd
/zKw6Civ8/4EpsdLxA+JGoiC1EDSJGqAlNMAwyxjEjWQEdUMMOxzZyNRAwNafZfkB3+j5G1nK/Fn
4IfgWY8mTk3h80Gt8anz4I2Y6uqZO1r9O4DHP7cmH57TzFN4s7PfBCjCA5hnldtPW4f28Ykb+M2A
Vt8FsOt3XOJLKCypgaT5K56BodJiAYIl0CORAQYF5cM3a45ElvgMDJVeLAD4CGIO3tlgWIjSUGmx
au+wBxoj8gq7aN9afpkF6CuRZV8vdktkLAeqrSec2n1i+k5kQngnsv2d3DKJWqD6ukCi5r4zQf+J
DNckssvdAz2a5SQGAN1rRyIKXN9Cl3er615ORKSRMjWQNK7PQHO8mDXPzaIi710ekgDaNEUOVEZV
8q2K4zM4KhwGGqPaAkysKtLnjpkTRQImVhuj2quJr1sbYfVcr9HZaI0VC4pcByR08VdIvyLXW2PF
QnSaV1gMmCJLcVwkTm2LAYJTcVwkTm37x1yES8dOPNr//Gs0NZA0/5cBAWJLZO7achBAyqJjnQGR
+BKZi/YZVQW23ebroMCi4/gebz4oVgGJ+E8la/m9imuouQj17Pk365G+5v2bFQDye5UZEdGjWE4C
GCKiexUPAEf6uw0AjyDYhvdsGBTo9uJTUiLgF9LD24zDIXOFAAAAAElFTkSuQmCC
__MK_EOF__
put app/globals.css <<'__MK_EOF__'
@import 'tailwindcss';

@theme inline {
  --font-display: var(--ff-display), system-ui, sans-serif;
  --font-sans: var(--ff-sans), system-ui, sans-serif;
  --font-mono: var(--ff-mono), ui-monospace, monospace;
}
@theme {
  --color-plaster: #edefea;
  --color-paper: #f7f8f4;
  --color-grout: #d3d8cf;
  --color-ink: #161b25;
  --color-slate: #556070;
  --color-lapis: #2b44d9;
  --color-verdigris: #12786a;
  --color-saffron: #f4b13a;
  --color-madder: #c93450;
  --container-page: 1240px;
}

@layer base {
  html { background: var(--color-plaster); color: var(--color-ink); scroll-behavior: smooth; -webkit-font-smoothing: antialiased; -moz-osx-font-smoothing: grayscale; text-rendering: optimizeLegibility; }
  body { font-family: var(--font-sans); line-height: 1.625; overflow-x: hidden; }
  ::selection { background: var(--color-lapis); color: #fff; }
  :focus-visible { outline: 2px solid var(--color-lapis); outline-offset: 3px; border-radius: 4px; }
  .on-ink :focus-visible { outline-color: var(--color-saffron); }
}

@layer components {
  .page-x { margin-inline: auto; width: 100%; max-width: var(--container-page); padding-inline: 1.25rem; @media (width >= 40rem) { padding-inline: 2rem; } }
  .h1, .h2 { font-family: var(--font-display); font-weight: 700; text-wrap: balance; }
  .h1 { font-size: clamp(2.3rem, 4.4vw, 3.6rem); letter-spacing: -0.035em; line-height: 1.04; }
  .h2 { font-size: clamp(1.8rem, 3.1vw, 2.6rem); letter-spacing: -0.03em; line-height: 1.08; }

  .btn { display: inline-flex; min-height: 48px; align-items: center; justify-content: center; border-radius: 9999px; padding-inline: 1.5rem; font-size: 15px; font-weight: 600; transition: color 150ms, background-color 150ms, border-color 150ms; transition-timing-function: cubic-bezier(0.4, 0, 0.2, 1); }
  .btn-lapis { background: var(--color-lapis); color: #fff; &:hover { background: #2237b8; } }
  .btn-saffron { background: var(--color-saffron); color: var(--color-ink); &:hover { background: #ffc255; } }
  .btn-line { border: 1px solid rgb(22 27 37 / 0.25); color: var(--color-ink); &:hover { border-color: var(--color-ink); background: var(--color-ink); color: var(--color-paper); } }
  .btn-line-light { border: 1px solid rgb(255 255 255 / 0.3); color: #fff; &:hover { border-color: #fff; background: #fff; color: var(--color-ink); } }

  .prose-page {
    h2 { margin: 2.5rem 0 0.75rem; font: 700 1.5rem / 2rem var(--font-display); letter-spacing: -0.025em; }
    p, li { color: var(--color-slate); }
    ul { margin-top: 0.5rem; list-style: disc; padding-left: 1.25rem; & > li + li { margin-top: 0.25rem; } }
    a { font-weight: 600; color: var(--color-lapis); text-decoration: underline; text-underline-offset: 4px; }
  }

  /* Demo desktops: a 1280 px canvas scaled to its box with CSS alone (tan(atan2(w, 1280px)) = w / 1280). */
  .desk { position: relative; width: 100%; overflow: hidden; aspect-ratio: 1280 / 800; container-type: inline-size; }
  .desk-in { position: absolute; inset: 0 auto auto 0; width: 1280px; height: 800px; transform-origin: 0 0; transform: scale(tan(atan2(100cqw, 1280px))); }
  .win { position: absolute; left: var(--l); top: var(--t); width: var(--w); height: var(--h); transform-origin: 50% 50%; transform: translate(var(--x), var(--y)) scale(var(--sx), var(--sy)); will-change: transform; contain: layout style; }
  @media (width <= 640px) {
    /* phones get a taller desktop */
    .desk-tall { aspect-ratio: 1280 / 960; .desk-in { height: 960px; } }
    .desk-tall .win { top: var(--tt); height: var(--ht); transform: translate(var(--x), var(--yt)) scale(var(--sx), var(--syt)); }
  }
  .dim { position: absolute; pointer-events: none; background: #0a0d14; transition: opacity 0.5s; }
  .stage-shadow { box-shadow: 0 30px 80px -30px rgb(22 27 37 / 0.7); }
  .wallpaper {
    background: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='40' height='40'%3E%3Crect x='2' y='2' width='36' height='36' rx='7' fill='none' stroke='%23fff' stroke-opacity='.045'/%3E%3C/svg%3E"),
      radial-gradient(700px 420px at 12% 8%, rgb(43 68 217 / 0.22), transparent 70%), radial-gradient(640px 400px at 92% 96%, rgb(18 120 106 / 0.2), transparent 70%), #12151b;
  }
  .switcher-bg { background-image: radial-gradient(520px 320px at 15% 0%, rgb(43 68 217 / 0.28), transparent 70%), radial-gradient(480px 300px at 100% 100%, rgb(18 120 106 / 0.26), transparent 70%); }
  .chat-fade { mask-image: linear-gradient(to bottom, transparent 0, #000 28px); } /* older messages fade out under the header */
  .ic { flex-shrink: 0; fill: none; stroke: currentColor; stroke-linecap: round; stroke-linejoin: round; }
  /* syntax colours in the code views */
  .sk { color: #f78fa7; } .sf { color: #7c93ff; } .ss { color: #5fd3b6; } .sn { color: #f4b13a; } .sc { color: #5c6578; } .sv { color: #c792ea; } .sm { color: #6b7487; }
  .blink { animation: blink 1.1s steps(1) infinite; }
}

@keyframes blink { 50% { opacity: 0; } }

@media (prefers-reduced-motion: reduce) {
  html { scroll-behavior: auto; }
  .blink { animation: none; }
  .win, .dim { transition: none !important; }
}
__MK_EOF__
put app/icon.svg <<'__MK_EOF__'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">
  <rect x="2" y="2" width="16" height="16" rx="3" fill="#2B44D9"/>
  <rect x="21" y="2" width="9" height="9" rx="3" fill="#F4B13A"/>
  <rect x="21" y="14" width="9" height="16" rx="3" fill="#12786A"/>
  <rect x="2" y="21" width="16" height="9" rx="3" fill="#C93450"/>
</svg>
__MK_EOF__
put app/layout.tsx <<'__MK_EOF__'
import type { Metadata, Viewport } from 'next';
import localFont from 'next/font/local';
import { Header } from '@/components/header';
import { Footer, Sprite } from '@/components/ui';
import { site } from '@/lib/site';
import './globals.css';

// Self-hosted variable fonts (latin), preloaded with metric-matched fallbacks so nothing shifts while they load.
const display = localFont({ src: '../node_modules/@fontsource-variable/bricolage-grotesque/files/bricolage-grotesque-latin-wght-normal.woff2', weight: '200 800', variable: '--ff-display' });
const sans = localFont({ src: '../node_modules/@fontsource-variable/instrument-sans/files/instrument-sans-latin-wght-normal.woff2', weight: '400 700', variable: '--ff-sans' });
const mono = localFont({ src: '../node_modules/@fontsource-variable/jetbrains-mono/files/jetbrains-mono-latin-wght-normal.woff2', weight: '100 800', variable: '--ff-mono', preload: false });

const description = 'Mosaïk gives every project its own workspace: your apps and windows arranged and ready in one click. Free and open source.';

export const metadata: Metadata = {
  metadataBase: new URL(site.url),
  title: { default: `${site.name} | Workspace Organizer`, template: `%s | ${site.name}` },
  description,
  openGraph: { type: 'website', siteName: site.name, title: `${site.name} | Workspace Organizer`, description },
  twitter: { card: 'summary', title: `${site.name} | Workspace Organizer`, description },
};

export const viewport: Viewport = { themeColor: '#EDEFEA' };

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={`${display.variable} ${sans.variable} ${mono.variable}`}>
      <body>
        <Sprite />
        <a href="#content" className="sr-only focus:not-sr-only focus:fixed focus:left-4 focus:top-4 focus:z-[60] focus:rounded-full focus:bg-ink focus:px-5 focus:py-3 focus:text-paper">
          Skip to content
        </a>
        <Header />
        <main id="content">{children}</main>
        <Footer />
      </body>
    </html>
  );
}
__MK_EOF__
put app/not-found.tsx <<'__MK_EOF__'
import Link from 'next/link';

export default function NotFound() {
  return (
    <div className="page-x pb-24 pt-40">
      <h1 className="h1">That window isn’t here.</h1>
      <p className="mt-5 max-w-md text-lg text-slate">The page you asked for doesn’t exist. Head back to the start.</p>
      <Link href="/" className="btn btn-lapis mt-8">
        Go to the homepage
      </Link>
    </div>
  );
}
__MK_EOF__
put app/page.tsx <<'__MK_EOF__'
import { DeskThumb, Stage, SwitcherArt, TabsArt, ZonesArt } from '@/components/desk';
import { Check, DownloadButton, PlatformList } from '@/components/ui';
import { WORKSPACES } from '@/lib/data';
import { site } from '@/lib/site';

const FEATURES = [
  {
    id: 'zones',
    title: 'Zones give every window a home',
    body: 'One click reads what you are working on: the window you work in gets the most room, a terminal goes under your editor, and the windows that help sit beside it. Or pick one of ten layouts, or carve the screen into your own named areas, like Code, Terminal and Review. Before you click, Mosaïk shows where each window would go.',
    points: ['Zones belong to the project, not the monitor', 'Every window wears its zone’s colour, so you can tell at a glance', 'Works across multiple displays'],
    Art: ZonesArt,
  },
  {
    id: 'tabs',
    title: 'Tabs for any app, not only the browser',
    body: 'Stack your editor, terminal and design tool into one frame and flip between them like browser tabs. Tabs dragged out of Chrome or Edge join in too. One window to find instead of twelve.',
    points: ['Group any windows into one tab strip', 'Tab order is remembered per workspace', 'Pull a tab out and it becomes a window again'],
    Art: TabsArt,
  },
  {
    id: 'layouts',
    title: 'Switch projects, not just windows',
    body: 'Save a workspace once and bring it back whenever you need it. Positions, sizes and tabs return together, so changing task takes a keystroke instead of ten minutes of rearranging.',
    points: ['Sets of the apps you use together, opened in one click', 'Saved on your computer, never in a cloud', 'Switch from the tray, the screen’s edge, or press Win+Alt+Space'],
    Art: SwitcherArt,
  },
];

const Intro = ({ title, children, className = '' }: { title: string; children: React.ReactNode; className?: string }) => (
  <div className={`grid gap-5 lg:grid-cols-12 lg:items-end lg:gap-14 ${className}`}>
    <h2 className="h2 lg:col-span-7">{title}</h2>
    <p className="max-w-md text-[17px] leading-[1.6] text-slate lg:col-span-5">{children}</p>
  </div>
);

export default function Home() {
  return (
    <>
      <section className="pb-20 pt-28 sm:pt-32 md:pb-28">
        <div className="page-x">
          <div className="grid gap-7 lg:grid-cols-12 lg:items-end lg:gap-14">
            <h1 className="font-display text-[clamp(2.4rem,4.7vw,4.1rem)] font-bold leading-[1.03] tracking-[-0.035em] lg:col-span-7">
              Every window,
              <br />
              in its place.
            </h1>
            <div className="lg:col-span-5 lg:pb-2">
              <p className="max-w-[30rem] text-[18px] leading-[1.45] text-slate sm:text-[19px]">
                Every project gets its own workspace: your apps and windows arranged and ready in one click.
              </p>
              <p className="mt-2 text-[15px] text-slate">Free and open source.</p>
            </div>
          </div>
          <div className="mt-10 md:mt-12">
            <Stage />
          </div>
        </div>
      </section>

      <section id="how" className="scroll-mt-16 border-t border-grout py-20 md:py-28">
        <div className="page-x">
          <h2 className="h2 max-w-3xl">Three ideas that tidy a whole desktop.</h2>
          <div className="mt-14 space-y-20 md:mt-20 md:space-y-28">
            {FEATURES.map(({ id, title, body, points, Art }, i) => (
              <article key={id} className="grid items-center gap-8 lg:grid-cols-12 lg:gap-14">
                <div className={`lg:col-span-5 ${i % 2 ? 'lg:order-2' : ''}`}>
                  <h3 className="font-display text-[clamp(1.5rem,2.4vw,2rem)] font-bold leading-[1.1] tracking-[-0.025em] text-balance">{title}</h3>
                  <p className="mt-4 text-[17px] leading-[1.6] text-slate">{body}</p>
                  <ul className="mt-6 space-y-3 text-[15px]">
                    {points.map((p) => (
                      <Check key={p}>{p}</Check>
                    ))}
                  </ul>
                </div>
                <div className={`lg:col-span-7 ${i % 2 ? 'lg:order-1' : ''}`}>
                  <Art />
                </div>
              </article>
            ))}
          </div>
        </div>
      </section>

      <section id="people" className="scroll-mt-16 border-t border-grout py-20 md:py-28">
        <div className="page-x">
          <Intro title="One desktop, many kinds of work.">A workspace is whatever you need open together. These four are the ones we kept reaching for.</Intro>
          <div className="mt-12 grid gap-x-8 gap-y-12 sm:grid-cols-2">
            {WORKSPACES.map((w, i) => (
              <div key={w.id}>
                <div className="overflow-hidden rounded-[16px] ring-1 ring-black/10">
                  <DeskThumb index={i} />
                </div>
                <div className="mt-4 flex items-baseline justify-between gap-4">
                  <h3 className="font-display text-[1.35rem] font-bold tracking-[-0.02em]">{w.name}</h3>
                  <p className="shrink-0 text-sm font-semibold text-lapis">{w.role}</p>
                </div>
                <p className="mt-1.5 max-w-md text-[15.5px] leading-relaxed text-slate">{w.about}</p>
              </div>
            ))}
          </div>
        </div>
      </section>

      <section id="platforms" className="scroll-mt-16 border-t border-grout py-20 md:py-28">
        <div className="page-x">
          <Intro title="Desktop first, because that’s where windows live." className="mb-10">
            Only a native app can move other apps’ windows, and each operating system allows a different amount. Here is the plan.
          </Intro>
          <PlatformList />
        </div>
      </section>

      <section className="on-ink bg-ink py-24 text-paper md:py-28">
        <div className="page-x grid gap-8 lg:grid-cols-12 lg:items-end lg:gap-14">
          <h2 className="font-display text-[clamp(2.2rem,4.4vw,3.6rem)] font-bold leading-[1.04] tracking-[-0.035em] lg:col-span-7">Put your work back together.</h2>
          <div className="lg:col-span-5">
            <p className="max-w-[28rem] text-[17px] leading-[1.6] text-white/70">
              Mosaïk is free and open source, and stays that way. If it earns its place on your desktop, you can support its work on Patreon.
            </p>
            <div className="mt-7 flex flex-wrap gap-3">
              <DownloadButton className="btn btn-saffron" />
              <a href={site.patreon} rel="noopener noreferrer" className="btn btn-line-light">
                Support on Patreon
              </a>
            </div>
          </div>
        </div>
      </section>
    </>
  );
}
__MK_EOF__
put app/robots.ts <<'__MK_EOF__'
import type { MetadataRoute } from 'next';
import { site } from '@/lib/site';

export default function robots(): MetadataRoute.Robots {
  return { rules: { userAgent: '*', allow: '/' }, sitemap: `${site.url}/sitemap.xml` };
}
__MK_EOF__
put app/sitemap.ts <<'__MK_EOF__'
import type { MetadataRoute } from 'next';
import { site } from '@/lib/site';

export default function sitemap(): MetadataRoute.Sitemap {
  return ['', '/download', '/conditions'].map((path) => ({ url: `${site.url}${path}` }));
}
__MK_EOF__
put components/apps.tsx <<'__MK_EOF__'
/* The pretend apps drawn inside the demo desktops. Plain markup, sized in pixels on a 1280 px canvas. */
import { memo, useId, type ReactNode } from 'react';
import { TILES, ZONE, type Win } from '@/lib/data';
import { Icon, Wordmark } from '@/components/ui';

/* ── shared chrome ─────────────────────────────────────────────────────── */
const Controls = ({ className = '' }: { className?: string }) => (
  <div className="flex h-full">
    {['min', 'max', 'close'].map((n) => (
      <span key={n} className={`grid w-[44px] place-items-center ${className}`}>
        <Icon n={n} s={11} w={1.2} />
      </span>
    ))}
  </div>
);

function TitleBar({ title, dark, glyph }: { title: string; dark?: boolean; glyph: string }) {
  return (
    <div className={`flex h-[34px] shrink-0 select-none items-center justify-between text-[12px] ${dark ? 'bg-[#101216] text-[#8E97AA]' : 'bg-[#E9ECF1] text-[#4A5262]'}`}>
      <div className="flex min-w-0 items-center gap-2 pl-3">
        <span className="h-[13px] w-[13px] shrink-0 rounded-[4px]" style={{ background: glyph }} />
        <span className="truncate">{title}</span>
      </div>
      <Controls />
    </div>
  );
}

const Avatar = ({ name, color, size }: { name: string; color: string; size: number }) => (
  <span className="grid shrink-0 place-items-center rounded-full font-semibold text-white" style={{ width: size, height: size, background: color, fontSize: size * 0.4 }}>
    {name[0]}
  </span>
);

/** A unique id that is safe inside url(#…) references. */
const useSvgId = () => `g${useId().replace(/[^\w-]/g, '')}`;

/* ── code editor and terminal ──────────────────────────────────────────── */
const KW = /\b(import|from|export|function|const|let|return|async|await|if|new|throw|type|else)\b/;

/** A tiny TSX highlighter. Plain text inherits the editor colour, so only tokens get a span. */
function Highlight({ line }: { line: string }) {
  return line.split(/(\/\/.*$|"[^"]*"|`[^`]*`|\b\d+\b|<\/?[A-Za-z][A-Za-z.]*|\b[A-Za-z_]+(?=\())/).map((p, i) => {
    if (i % 2 === 0) return p.split(KW).map((q, j) => (j % 2 ? <span key={`${i}.${j}`} className="sk">{q}</span> : q));
    const c = p.startsWith('//') ? 'sc' : p[0] === '"' || p[0] === '`' ? 'ss' : /^\d/.test(p) ? 'sn' : KW.test(p) ? 'sk' : 'sf';
    return <span key={i} className={c}>{p}</span>;
  });
}

const CODE = `import { useState } from "react"
import { useRouter } from "next/navigation"
import { useCart } from "@/lib/cart"
import { formatPrice } from "@/lib/money"

export function Checkout() {
  const router = useRouter()
  const { items, total } = useCart()
  const [status, setStatus] = useState<"idle" | "paying">("idle")

  async function pay() {
    setStatus("paying")
    const res = await fetch("/api/pay", {
      method: "POST",
      body: JSON.stringify({ items, total }),
    })
    if (!res.ok) throw new Error("Payment failed")
    router.push("/thanks")
  }

  return (
    <button onClick={pay} disabled={status === "paying"}>
      Pay {formatPrice(total)}
    </button>
  )
}`.split('\n');

const TREE: [number, string, boolean?][] = [
  [0, 'app', true], [1, 'checkout', true], [2, 'page.tsx'], [2, 'checkout.tsx', true], [0, 'components', true],
  [1, 'button.tsx'], [1, 'price.tsx'], [0, 'lib', true], [1, 'cart.ts'], [1, 'money.ts'], [1, 'pay.ts'],
];

export function EditorApp({ bare }: { bare?: boolean }) {
  return (
    <div className="flex h-full w-full flex-col bg-[#1E2027] text-[#C9CEDA]">
      {!bare && <TitleBar title="checkout.tsx - shop - Code" dark glyph="#2B44D9" />}
      <div className="flex min-h-0 flex-1">
        <div className="flex w-[42px] shrink-0 flex-col items-center gap-[18px] bg-[#16181D] pt-3 text-[#6B7487]">
          <span className="text-[#C9CEDA]"><Icon n="file" s={18} /></span>
          <Icon n="search" s={18} />
          <Icon n="branch" s={18} />
          <Icon n="frame" s={18} />
        </div>
        <div className="w-[168px] shrink-0 bg-[#181A20] py-2 text-[12.5px]">
          <p className="px-4 pb-2 pt-1 text-[10.5px] font-semibold tracking-[0.08em] text-[#6B7487]">SHOP</p>
          {TREE.map(([d, name, dir]) => (
            <div key={name} className={`flex h-[22px] items-center gap-1.5 truncate ${name === 'checkout.tsx' ? 'bg-[#262A36] text-white' : 'text-[#A3ABBC]'}`} style={{ paddingLeft: 16 + d * 14 }}>
              <span className="text-[#6B7487]"><Icon n={dir ? 'chevD' : 'file'} s={11} /></span>
              {name}
            </div>
          ))}
        </div>
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex h-[34px] shrink-0 items-end bg-[#16181D] text-[12.5px]">
            {['checkout.tsx', 'cart.ts', 'pay.ts'].map((t, i) => (
              <div key={t} className={`flex h-full items-center gap-2 px-4 ${i ? 'text-[#7B8499]' : 'border-t-2 border-[#7C93FF] bg-[#1E2027] text-white'}`}>
                <span className="h-2 w-2 rounded-full bg-[#7C93FF]" />
                {t}
              </div>
            ))}
          </div>
          <div className="flex h-[24px] shrink-0 items-center gap-1.5 px-4 text-[11.5px] text-[#7B8499]">
            app <Icon n="chevR" s={10} /> checkout <Icon n="chevR" s={10} /> checkout.tsx
          </div>
          <div className="relative min-h-0 flex-1 overflow-hidden py-1 font-mono text-[12.5px] leading-[20px]">
            {CODE.map((l, i) => (
              <div key={i} className={`flex ${i === 16 ? 'bg-[#272B38]' : ''}`}>
                <span className="w-[46px] shrink-0 select-none pr-4 text-right text-[#4C5468]">{i + 1}</span>
                <span className="whitespace-pre"><Highlight line={l} /></span>
              </div>
            ))}
            <div className="absolute inset-y-1 right-2 w-[54px] space-y-[3px] opacity-60">
              {CODE.map((l, i) => (
                <div key={i} className="h-[3px] rounded-full bg-[#4C5468]" style={{ width: Math.max(8, Math.min(54, l.length * 1.1)), marginLeft: l.search(/\S|$/) * 1.2 }} />
              ))}
            </div>
          </div>
        </div>
      </div>
      <div className="flex h-[24px] shrink-0 items-center justify-between bg-[#101216] px-3 text-[11.5px] text-[#8E97AA]">
        <span className="flex items-center gap-1.5"><Icon n="branch" s={12} /> feat/checkout</span>
        <span>Ln 17, Col 40 &nbsp;&nbsp; TypeScript React</span>
      </div>
    </div>
  );
}

/* class names: ss green, sv violet, sn amber, sf blue, sm grey; unmarked text is the terminal's own colour */
const PROMPT: [string, string][] = [['sf', 'shop'], ['sm', ' on '], ['sv', ' feat/checkout'], ['ss', ' ❯ ']];
const TERM: [string, string][][] = [
  [...PROMPT, ['', 'npm run dev']],
  [['', '  ▲ Next.js 16']],
  [['sm', '  - Local:        '], ['sf', 'http://localhost:3000']],
  [['ss', ' ✓ '], ['', 'Ready in 842ms']],
  [['sn', ' ○ '], ['sm', 'Compiling /checkout ...']],
  [['ss', ' ✓ '], ['', 'Compiled /checkout in 311ms']],
  [['sm', ' GET /checkout '], ['ss', '200'], ['sm', ' in 38ms']],
  [['sm', ' POST /api/pay '], ['ss', '200'], ['sm', ' in 112ms']],
  [['sm', ' GET /thanks '], ['ss', '200'], ['sm', ' in 21ms']],
  PROMPT,
];

function TerminalApp() {
  return (
    <div className="flex h-full w-full flex-col bg-[#0F1115] text-[#D9DEE9]">
      <div className="flex h-[34px] shrink-0 select-none items-stretch justify-between bg-[#101216] text-[12px] text-[#8E97AA]">
        <div className="flex items-end pl-2">
          <div className="flex h-[28px] items-center gap-2 rounded-t-[8px] bg-[#0F1115] px-4 text-[#D9DEE9]">
            <span className="text-[#5FD3B6]">&gt;_</span> PowerShell
          </div>
          <div className="flex h-[28px] items-center gap-2 px-4">Ubuntu</div>
          <div className="flex h-[28px] items-center px-2"><Icon n="plus" s={12} /></div>
        </div>
        <Controls />
      </div>
      <div className="flex min-h-0 flex-1 flex-col justify-end overflow-hidden px-4 py-3 font-mono text-[12.5px] leading-[20px]">
        {TERM.map((segs, i) => (
          <div key={i} className="whitespace-pre">
            {segs.map(([c, t], j) => (c ? <span key={j} className={c}>{t}</span> : t))}
            {i === TERM.length - 1 && <span className="blink inline-block h-[14px] w-[7px] translate-y-[2px] bg-[#D9DEE9]" />}
          </div>
        ))}
      </div>
    </div>
  );
}

/* ── browser ───────────────────────────────────────────────────────────── */
/** A small mosaic swatch: one stroked path per colour instead of a rect per tile. */
function Mosaic({ seed, colors, cols }: { seed: number; colors: string[]; cols: number }) {
  const paths: Record<string, string> = {};
  let a = seed * 9301 + 49297;
  for (let i = 0; i < cols * cols; i++) {
    a = (a * 9301 + 49297) % 233280;
    const c = colors[Math.floor((a / 233280) * colors.length)];
    paths[c] = `${paths[c] ?? ''}M${(i % cols) * 10 + 2.5} ${Math.floor(i / cols) * 10 + 2.5}h5v5h-5z`;
  }
  return (
    <svg viewBox={`0 0 ${cols * 10 + 1} ${cols * 10 + 1}`} className="block h-full w-full" preserveAspectRatio="xMidYMid slice" strokeWidth="3" strokeLinejoin="round" aria-hidden="true">
      <rect width="100%" height="100%" fill="#E8E4DA" />
      {Object.entries(paths).map(([c, d]) => (
        <path key={c} d={d} fill={c} stroke={c} />
      ))}
    </svg>
  );
}

function Pull() {
  const diff = [' export function total(items: Item[]) {', '-  return items.reduce((s, i) => s + i.price, 0)', '+  const sum = items.reduce((s, i) => s + i.price * i.qty, 0)', '+  return Math.round(sum * 100) / 100', ' }', ' ', '+export const SHIPPING_FREE_OVER = 75'];
  return (
    <div className="h-full px-6 py-5">
      <p className="text-[22px] font-semibold leading-tight tracking-[-0.01em]">
        Add one-tap checkout <span className="font-normal text-[#8A93A3]">#482</span>
      </p>
      <div className="mt-2.5 flex items-center gap-2.5 text-[12.5px] text-[#5B6474]">
        <span className="rounded-full bg-[#1F8A5B] px-2.5 py-[3px] font-semibold text-white">Open</span>
        <span>
          <b className="text-[#1B1F29]">ana</b> wants to merge 3 commits into <code className="rounded-[4px] bg-[#EEF0F4] px-1.5">main</code> from{' '}
          <code className="rounded-[4px] bg-[#EEF0F4] px-1.5">feat/checkout</code>
        </span>
      </div>
      <div className="mt-4 flex gap-6 border-b border-[#E3E6EB] text-[13px] text-[#5B6474]">
        {['Conversation 4', 'Commits 3', 'Checks 6', 'Files changed 5'].map((t, i) => (
          <span key={t} className={`pb-2.5 ${i === 3 ? 'border-b-2 border-[#2B44D9] font-semibold text-[#1B1F29]' : ''}`}>{t}</span>
        ))}
      </div>
      <div className="mt-4 overflow-hidden rounded-[8px] border border-[#E3E6EB]">
        <div className="flex items-center justify-between bg-[#F6F7F9] px-3.5 py-2 text-[12.5px]">
          <span className="font-semibold">lib/cart.ts</span>
          <span><b className="text-[#1F8A5B]">+14</b> <b className="text-[#C93450]">−3</b></span>
        </div>
        <div className="font-mono text-[12px] leading-[21px]">
          {diff.map((l, i) => {
            const s = l[0];
            return (
              <div key={i} className={`flex whitespace-pre ${s === '+' ? 'bg-[#E6F6EC]' : s === '-' ? 'bg-[#FCE9EC]' : ''}`}>
                <span className="w-[34px] shrink-0 select-none text-right text-[#9AA3B2]">{i + 41}</span>
                <span className={`w-[22px] shrink-0 text-center ${s === '+' ? 'text-[#1F8A5B]' : s === '-' ? 'text-[#C93450]' : 'text-transparent'}`}>{s}</span>
                <span>{l.slice(1)}</span>
              </div>
            );
          })}
        </div>
      </div>
      <div className="mt-4 flex items-center gap-2.5 rounded-[8px] border border-[#BFE3CF] bg-[#F1FAF5] px-3.5 py-2.5 text-[13px]">
        <span className="grid h-5 w-5 place-items-center rounded-full bg-[#1F8A5B] text-white"><Icon n="check" s={12} w={2} /></span>
        <b>All checks have passed</b> <span className="text-[#5B6474]">6 successful checks</span>
      </div>
    </div>
  );
}

const REFS: [string, string[]][] = [
  ['Roman floor, Ostia', TILES],
  ['Byzantine gold', ['#1B2130', '#C9A24A', '#8C3B2E']],
  ['Glazed ceramics', ['#12786A', '#E8E4DA', '#2B44D9']],
  ['Tessera study', ['#C93450', '#F4B13A', '#EDEFEA', '#1B2130']],
  ['Lapis & bone', ['#2B44D9', '#EDEFEA', '#1B2130']],
  ['Courtyard pattern', ['#12786A', '#F4B13A', '#C93450']],
];
const Refs = () => (
  <div className="h-full bg-[#F6F7F9] px-5 py-4">
    <div className="flex items-end justify-between">
      <p className="text-[18px] font-semibold tracking-[-0.01em]">Tessellation, references</p>
      <p className="text-[12px] text-[#8A93A3]">24 pins</p>
    </div>
    <div className="mt-3.5 grid grid-cols-3 gap-3">
      {REFS.map(([cap, c], i) => (
        <div key={cap}>
          <div className="aspect-[4/3] overflow-hidden rounded-[8px] border border-black/5">
            <Mosaic seed={i + 3} colors={c} cols={i % 2 ? 10 : 7} />
          </div>
          <p className="mt-1.5 truncate text-[11.5px] text-[#5B6474]">{cap}</p>
        </div>
      ))}
    </div>
  </div>
);

const RESULTS = [
  ['Ocean heat content: a regional reassessment', 'journals.example.org › climate › 2024', 'Upper-ocean heat content rose in every basin between 2005 and 2022, with the largest gains in the subtropical Atlantic.'],
  ['Sea surface temperature records, 1980 to today', 'data.example.gov › sst › monthly', 'Monthly gridded anomalies relative to the 1991 to 2020 mean, with uncertainty estimates for each cell.'],
  ['Methods for estimating deep-ocean warming', 'arxiv.example.org › abs › 2403.01177', 'We compare three interpolation schemes for sparse float data and quantify the bias each introduces below 700 m.'],
];

const Sources = () => (
  <div className="h-full px-6 py-5">
    <div className="flex h-[38px] items-center gap-2.5 rounded-full border border-[#DADFE6] px-4 text-[13px] shadow-xs">
      <Icon n="search" s={14} /> ocean heat content trend 0-2000 m
    </div>
    <p className="mt-3 text-[11.5px] text-[#8A93A3]">About 1,240,000 results</p>
    <div className="mt-3 space-y-4">
      {RESULTS.map(([t, u, s]) => (
        <div key={t}>
          <p className="text-[11.5px] text-[#4A5262]">{u}</p>
          <p className="text-[16px] leading-snug text-[#2B44D9]">{t}</p>
          <p className="mt-0.5 text-[12.5px] leading-snug text-[#5B6474]">{s}</p>
        </div>
      ))}
    </div>
  </div>
);

const Dash = () => (
  <div className="h-full bg-[#F6F7F9] px-5 py-4">
    <p className="text-[16px] font-semibold">Creator dashboard</p>
    <div className="mt-3 grid grid-cols-3 gap-2.5">
      {[['Followers', '18.4K', '+212 today'], ['Avg. viewers', '912', '+8%'], ['Chat / min', '148', 'peak 203']].map(([k, v, d]) => (
        <div key={k} className="rounded-[8px] border border-[#E3E6EB] bg-white px-3 py-2.5">
          <p className="text-[11px] text-[#8A93A3]">{k}</p>
          <p className="text-[20px] font-semibold leading-tight">{v}</p>
          <p className="text-[11px] text-[#1F8A5B]">{d}</p>
        </div>
      ))}
    </div>
    <div className="mt-3 space-y-1.5 text-[12px] text-[#4A5262]">
      {['nova_k followed', 'Kestrel subscribed (Tier 1)', 'pilotfish raided with 64 viewers'].map((t) => (
        <p key={t} className="flex items-center gap-2"><span className="h-1.5 w-1.5 rounded-full bg-[#2B44D9]" />{t}</p>
      ))}
    </div>
  </div>
);

const PAGES: Record<string, [string, string, () => ReactNode]> = {
  pr: ['git.acme.dev/shop/pull/482/files', 'Add one-tap checkout #482', Pull],
  refs: ['pinboard.example/brand-refresh', 'Tessellation, references', Refs],
  sources: ['search.example/?q=ocean+heat+content', 'ocean heat content trend', Sources],
  dash: ['studio.example/dashboard', 'Creator dashboard', Dash],
};

function BrowserApp({ variant = 'pr' }: { variant?: string }) {
  const [url, tab, Page] = PAGES[variant] ?? PAGES.pr;
  return (
    <div className="flex h-full w-full flex-col bg-white text-[#1B1F29]">
      <div className="flex h-[36px] shrink-0 select-none items-end justify-between bg-[#DDE1E8] pl-2 text-[12px]">
        <div className="flex h-[30px] items-center gap-2 rounded-t-[8px] bg-[#F6F7F9] pl-3 pr-4">
          <span className="h-3 w-3 rounded-[4px] bg-[#12786A]" />
          <span className="max-w-[160px] truncate">{tab}</span>
          <Icon n="close" s={10} />
        </div>
        <Controls className="text-[#4A5262]" />
      </div>
      <div className="flex h-[40px] shrink-0 items-center gap-3 bg-[#F6F7F9] px-3 text-[#5B6474]">
        <Icon n="back" s={15} />
        <Icon n="fwd" s={15} />
        <Icon n="reload" s={15} />
        <div className="flex h-[28px] min-w-0 flex-1 items-center gap-2 rounded-full bg-[#E9ECF1] px-3 text-[12.5px]">
          <Icon n="lock" s={12} />
          <span className="truncate">{url}</span>
        </div>
        <Avatar name="S" color="#2B44D9" size={22} />
      </div>
      <div className="min-h-0 flex-1 overflow-hidden border-t border-[#E3E6EB]"><Page /></div>
    </div>
  );
}

/* ── chat ──────────────────────────────────────────────────────────────── */
type Msg = [who: string, time: string, text: string];
const PEOPLE: Record<string, string> = { Ana: '#2B44D9', Ravi: '#C93450', Mia: '#12786A', Elena: '#C93450', You: '#2B44D9' };

const TEAM: Msg[] = [
  ['Mia', '09:31', 'Morning. Checkout copy is in review, feedback welcome.'],
  ['Ravi', '09:48', 'Looks good to me. One nit on the empty cart state.'],
  ['Ana', '10:05', 'Fixed the nit and pushed. CI is running.'],
  ['Mia', '10:12', 'Thanks! Approving once it goes green.'],
  ['Ana', '10:42', 'Staging is green. Merging #482 after lunch.'],
  ['Ravi', '10:44', 'Pay button copy is updated. Can someone check it on mobile?'],
  ['Mia', '10:51', 'On it. Release notes draft is in the doc.'],
  ['Ana', '10:58', 'Thanks both. Tagging v2.4.0 at 3.'],
];

const CLIENT: Msg[] = [
  ['Elena', 'Mon', 'Hi! Attaching the references we talked about on the call.'],
  ['You', 'Mon', 'Got them, thank you. The tessellation direction is great, I will start there.'],
  ['Elena', 'Mon', 'Perfect. No rush, we only need it before the pitch.'],
  ['Elena', 'Tue', 'Love the tile mark. Could the wordmark sit a little tighter?'],
  ['You', 'Tue', 'Yes. Sending a tighter version with the lockups this afternoon.'],
  ['Elena', 'Wed', 'Perfect. Approved for the pitch deck.'],
];

/* live chat: [name, colour, text, badge]; a name of '!' marks a system notice */
const BADGE: Record<string, string> = { m: '#12786A', s: '#7C5CFF', v: '#F4B13A' };
const N: Record<string, string> = { Kestrel: '#F4B13A', tern: '#5FD3B6', dunes_: '#C792EA', nova_k: '#7C93FF', quill: '#E86F5B', mossy: '#F78FA7', pilotfish: '#5FD3B6' };
const B: Record<string, string> = { Kestrel: 'm', nova_k: 's', quill: 's', pilotfish: 'v' };
const LIVE: [string, string][] = [
  ['Kestrel', 'reminder: be kind in here, new people join every night'],
  ['tern', 'first time catching a live run of this boss'],
  ['dunes_', 'good luck chat is rooting for u'],
  ['nova_k', 'that opener was so smooth'],
  ['quill', 'brb snacks'],
  ['mossy', 'is this the speedrun route or just vibes'],
  ['pilotfish', 'vibes. route is way riskier'],
  ['tern', 'lmao the camera'],
  ['Kestrel', 'clip that one @quill'],
  ['dunes_', 'hiii'],
  ['Kestrel', 'welcome in. clips are on, no spoilers pls'],
  ['quill', 'ok that last fight was insane'],
  ['mossy', 'wait how did u get past phase 2 lmao'],
  ['tern', 'he parried it. he PARRIED it'],
  ['pilotfish', 'phase 2 is just patience tbh'],
  ['nova_k', 'KEKW'],
  ['mossy', '@pilotfish i keep dying to the sweep'],
  ['pilotfish', 'roll toward it not away'],
  ['!', 'pilotfish is raiding with 64 viewers'],
  ['quill*', 'raid gang!!'],
  ['dunes_', 'o7'],
  ['tern', 'been here since the 5am stream and the soundtrack still slaps'],
  ['nova_k', 'who made the overlay, it looks clean'],
  ['Kestrel', '@nova_k its his own, link is in the panel'],
  ['mossy', 'ok going in, wish me luck'],
];

function Messages({ msgs, compact }: { msgs: Msg[]; compact?: boolean }) {
  return (
    <div className={compact ? 'space-y-2 px-4 py-2.5' : 'space-y-3.5 px-4 py-3.5'}>
      {msgs.map(([who, time, text], i) =>
        compact ? (
          <p key={i} className="text-[13px] leading-snug text-[#2A303C]">
            <b style={{ color: PEOPLE[who] }}>{who}</b> <span className="text-[11px] text-[#8A93A3]">{time}</span>
            <br />
            {text}
          </p>
        ) : (
          <div key={i} className="flex gap-2.5">
            <Avatar name={who} color={PEOPLE[who]} size={30} />
            <div className="min-w-0">
              <p className="text-[12.5px]"><b>{who}</b> <span className="text-[11px] text-[#8A93A3]">{time}</span></p>
              <p className="text-[13px] leading-snug text-[#2A303C]">{text}</p>
            </div>
          </div>
        ),
      )}
    </div>
  );
}

const Composer = ({ text }: { text: string }) => (
  <div className="mx-4 mb-3.5 flex h-[38px] shrink-0 items-center justify-between rounded-[8px] border border-[#D5DAE2] px-3 text-[12.5px] text-[#8A93A3]">
    {text}
    <Icon n="send" s={14} />
  </div>
);

function ChatApp({ variant = 'team' }: { variant?: string }) {
  if (variant === 'live') {
    return (
      <div className="flex h-full w-full flex-col bg-[#18181B] text-[#E6E6EA]">
        <TitleBar title="Stream chat" dark glyph="#C93450" />
        <div className="flex h-[34px] shrink-0 items-center justify-between border-b border-white/10 px-4 text-[12px]">
          <b>Stream chat</b>
          <span className="flex items-center gap-1.5 text-[#8E97AA]"><Icon n="eye" s={13} /> 1,247</span>
        </div>
        <div className="chat-fade flex min-h-0 flex-1 flex-col justify-end gap-[3px] overflow-hidden py-2 text-[13px] leading-[1.4]">
          {LIVE.map(([name, text], i) => {
            if (name === '!') {
              return (
                <p key={i} className="mx-2 my-1 rounded-[5px] border-l-[3px] border-[#7C5CFF] bg-[#7C5CFF]/15 px-2.5 py-1.5 text-[12.5px] text-[#D8D2FF]">
                  {text}
                </p>
              );
            }
            const hi = name.endsWith('*');
            const n = hi ? name.slice(0, -1) : name;
            return (
              <p key={i} className={`px-3 py-[3px] ${hi ? 'mx-1 rounded-[5px] bg-white/[0.07]' : ''}`}>
                {B[n] && <span className="mr-1.5 inline-block h-[13px] w-[13px] translate-y-[2px] rounded-[3px]" style={{ background: BADGE[B[n]] }} />}
                <b style={{ color: N[n] }}>{n}</b>
                <span className="text-[#8E97AA]">: </span>
                {text}
              </p>
            );
          })}
        </div>
        <div className="mx-3 mb-3 flex h-[36px] shrink-0 items-center rounded-[8px] bg-[#26262C] px-3 text-[12.5px] text-[#8E97AA]">Send a message</div>
      </div>
    );
  }
  if (variant === 'client') {
    return (
      <div className="flex h-full w-full flex-col bg-white text-[#1B1F29]">
        <TitleBar title="Northwind - Elena Voss" glyph="#C93450" />
        <div className="flex h-[38px] shrink-0 items-center gap-2 border-b border-[#E3E6EB] px-4 text-[13px]">
          <Avatar name="E" color="#C93450" size={22} /> <b>Elena Voss</b> <span className="text-[#8A93A3]">Northwind</span>
        </div>
        <div className="chat-fade flex min-h-0 flex-1 flex-col justify-end overflow-hidden"><Messages msgs={CLIENT} compact /></div>
        <Composer text="Message Elena" />
      </div>
    );
  }
  return (
    <div className="flex h-full w-full flex-col bg-white text-[#1B1F29]">
      <TitleBar title="Acme - releases" glyph="#C93450" />
      <div className="flex min-h-0 flex-1">
        <div className="w-[150px] shrink-0 bg-[#1B2130] px-3 py-3 text-[12.5px] text-[#9AA3B8]">
          <p className="mb-2.5 text-[13px] font-semibold text-white">Acme</p>
          {['general', 'releases', 'design', 'random'].map((c) => (
            <p key={c} className={`flex items-center gap-1.5 rounded-[6px] px-2 py-[5px] ${c === 'releases' ? 'bg-[#2B44D9] text-white' : ''}`}>
              <Icon n="hash" s={12} /> {c}
            </p>
          ))}
          <p className="mb-1.5 mt-4 px-2 text-[11px] text-[#6B7487]">Direct messages</p>
          {['Ana', 'Ravi', 'Mia'].map((n) => (
            <p key={n} className="flex items-center gap-2 px-2 py-[5px]"><span className="h-2 w-2 rounded-full bg-[#5FD3B6]" />{n}</p>
          ))}
        </div>
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex h-[38px] shrink-0 items-center gap-1.5 border-b border-[#E3E6EB] px-4 text-[13px] font-semibold">
            <Icon n="hash" s={13} /> releases
          </div>
          <div className="chat-fade flex min-h-0 flex-1 flex-col justify-end overflow-hidden"><Messages msgs={TEAM} /></div>
          <Composer text="Message #releases" />
        </div>
      </div>
    </div>
  );
}

/* ── design tool ───────────────────────────────────────────────────────── */
const LAYERS: [number, string, boolean?][] = [[0, 'Page 1'], [0, 'Logo lockups'], [1, 'Lockup / dark', true], [1, 'Lockup / light'], [1, 'Mark'], [0, 'Colour'], [1, 'Tiles']];

function DesignApp() {
  return (
    <div className="flex h-full w-full flex-col bg-[#1E2027] text-[#C9CEDA]">
      <TitleBar title="Logo system - Design" dark glyph="#F4B13A" />
      <div className="flex h-[40px] shrink-0 items-center justify-between border-b border-white/10 bg-[#16181D] px-3 text-[#8E97AA]">
        <div className="flex items-center gap-1">
          {['move', 'frame', 'shape', 'pen', 'text', 'hand'].map((n, i) => (
            <span key={n} className={`grid h-[28px] w-[28px] place-items-center rounded-[6px] ${i ? '' : 'bg-[#2B44D9] text-white'}`}>
              <Icon n={n} s={15} />
            </span>
          ))}
        </div>
        <div className="flex items-center gap-4 text-[12px]">
          <span>100%</span>
          <span className="rounded-[6px] bg-[#2B44D9] px-3 py-1 font-semibold text-white">Share</span>
        </div>
      </div>
      <div className="flex min-h-0 flex-1">
        <div className="w-[176px] shrink-0 border-r border-white/10 bg-[#16181D] py-2.5 text-[12.5px]">
          <p className="px-3.5 pb-2 text-[11px] font-semibold text-[#6B7487]">Layers</p>
          {LAYERS.map(([d, n, sel], i) => (
            <div key={n} className={`flex h-[26px] items-center gap-1.5 ${sel ? 'bg-[#2B44D9]/30 text-white' : 'text-[#A3ABBC]'}`} style={{ paddingLeft: 14 + d * 16 }}>
              <Icon n={d === 0 && i > 0 ? 'chevD' : 'shape'} s={11} />
              {n}
            </div>
          ))}
        </div>
        <div className="relative min-w-0 flex-1 overflow-hidden bg-[#2A2D36]" style={{ backgroundImage: 'radial-gradient(#3A3E4A 1px, transparent 1px)', backgroundSize: '18px 18px' }}>
          <div className="absolute left-[7%] top-[9%] h-[40%] w-[40%] rounded-[6px] bg-white p-[8%] shadow-lg">
            <Wordmark className="h-full w-full" fill="#161B25" decorative />
          </div>
          <div className="absolute left-[53%] top-[9%] h-[40%] w-[40%] rounded-[6px] bg-[#161B25] p-[8%] shadow-lg">
            <Wordmark className="h-full w-full" fill="#F7F8F4" decorative />
          </div>
          <div className="absolute left-[7%] top-[56%] flex h-[34%] w-[40%] gap-[3%] rounded-[6px] bg-[#EDEFEA] p-[4%] shadow-lg">
            {['#2B44D9', '#12786A', '#F4B13A', '#C93450'].map((c) => (
              <span key={c} className="flex-1 rounded-[5px]" style={{ background: c }} />
            ))}
          </div>
          <div className="absolute left-[53%] top-[56%] grid h-[34%] w-[40%] place-items-center rounded-[6px] bg-[#2B44D9] shadow-lg">
            <Wordmark className="w-[62%]" fill="#FFFFFF" tiles={['#FFFFFF', '#FFFFFF']} decorative />
          </div>
          <div className="pointer-events-none absolute left-[7%] top-[9%] h-[40%] w-[40%] border border-[#7C93FF]">
            {['-3px', 'calc(100% - 3px)'].flatMap((t) =>
              ['-3px', 'calc(100% - 3px)'].map((l) => <span key={l + t} className="absolute h-[7px] w-[7px] border border-[#7C93FF] bg-white" style={{ left: l, top: t }} />),
            )}
            <span className="absolute left-1/2 top-[calc(100%+8px)] -translate-x-1/2 whitespace-nowrap rounded-[4px] bg-[#7C93FF] px-1.5 py-[1px] text-[10.5px] font-semibold text-white">320 × 200</span>
          </div>
        </div>
        <div className="w-[190px] shrink-0 space-y-3.5 border-l border-white/10 bg-[#16181D] px-3.5 py-3 text-[12px]">
          <p className="text-[11px] font-semibold text-[#6B7487]">Design</p>
          <div className="grid grid-cols-2 gap-2 text-[#A3ABBC]">
            {['X 120', 'Y 96', 'W 320', 'H 200'].map((v) => (
              <span key={v} className="rounded-[5px] bg-[#23262E] px-2 py-1">{v}</span>
            ))}
          </div>
          <p className="pt-1 text-[11px] font-semibold text-[#6B7487]">Fill</p>
          {['FFFFFF', '2B44D9', 'F4B13A'].map((h) => (
            <div key={h} className="flex items-center gap-2 text-[#A3ABBC]">
              <span className="h-[18px] w-[18px] rounded-[4px] ring-1 ring-white/20" style={{ background: `#${h}` }} />
              <span className="flex-1">{h}</span>
              <span>100%</span>
            </div>
          ))}
          <p className="pt-1 text-[11px] font-semibold text-[#6B7487]">Text</p>
          <p className="text-[#A3ABBC]">Bricolage Grotesque<br />Bold · 96 · −3.5%</p>
        </div>
      </div>
    </div>
  );
}

/* ── notes, reader, charts ─────────────────────────────────────────────── */
function NotesApp({ variant = 'brief' }: { variant?: string }) {
  if (variant === 'draft') {
    const P = ({ children }: { children: ReactNode }) => <p className="mt-2 text-[13.5px] leading-[1.7] text-[#2A303C]">{children}</p>;
    return (
      <div className="flex h-full w-full flex-col bg-[#FBFBF8] text-[#1B1F29]">
        <TitleBar title="Thesis - 3. Results" glyph="#12786A" />
        <div className="min-h-0 flex-1 overflow-hidden px-9 py-7">
          <p className="text-[11.5px] text-[#8A93A3]">Chapter 3</p>
          <p className="font-display text-[28px] font-bold leading-tight tracking-[-0.02em]">Results</p>
          <p className="mt-5 font-display text-[17px] font-semibold">3.1 Warming of the upper ocean</p>
          <P>
            Between 1980 and 2020 the global mean sea surface temperature anomaly rose by 0.87 °C (Figure 3.2). The increase was not uniform: the
            subtropical Atlantic and the Indian Ocean warmed fastest, while the Southern Ocean showed the weakest trend over the same period.
          </P>
          <p className="mt-3 text-[13.5px] leading-[1.7] text-[#2A303C]">
            Heat content in the top 700 m followed a similar pattern<sup className="text-[#C93450]">12</sup>. We therefore treat the two series as
            consistent and report regional estimates in Table 3.1, with uncertainty ranges derived from the float coverage described in Section 2.4.
          </p>
          <p className="mt-5 font-display text-[17px] font-semibold">3.2 Regional differences</p>
          <P>
            Regional trends diverge most strongly after 2005, when the float network reached full global coverage and the sampling bias in the
            earlier record largely disappears.
          </P>
        </div>
        <div className="flex h-[26px] shrink-0 items-center justify-between border-t border-[#E3E6EB] px-4 text-[11.5px] text-[#8A93A3]">
          <span>2,418 words</span>
          <span>Saved</span>
        </div>
      </div>
    );
  }
  return (
    <div className="flex h-full w-full flex-col bg-white text-[#1B1F29]">
      <TitleBar title="Brand refresh brief - Notes" glyph="#12786A" />
      <div className="min-h-0 flex-1 overflow-hidden px-6 py-4">
        <p className="font-display text-[22px] font-bold leading-tight tracking-[-0.02em]">Brand refresh brief</p>
        <p className="mt-2 flex gap-4 text-[12px] text-[#5B6474]">
          <span className="rounded-full bg-[#FFF1D6] px-2.5 py-0.5 font-semibold text-[#8A5A00]">In progress</span>
          <span>Due Oct 17</span>
          <span>Owner: you</span>
        </p>
        <p className="mt-3.5 text-[13px] font-semibold">Goals</p>
        <ul className="mt-1 space-y-1 text-[13px] text-[#2A303C]">
          <li>• A mark that works at 16 px and on a billboard</li>
          <li>• Warm, mineral palette, no gradients</li>
        </ul>
        <p className="mt-3.5 text-[13px] font-semibold">To do</p>
        <div className="mt-1.5 space-y-1.5 text-[13px]">
          {(['Tighten wordmark spacing', 'Export lockups for the deck', 'Colour-blind check'] as const).map((t, i) => (
            <p key={t} className="flex items-center gap-2">
              <span className={`grid h-[15px] w-[15px] place-items-center rounded-[4px] border ${i < 2 ? 'border-[#12786A] bg-[#12786A] text-white' : 'border-[#9AA3B2]'}`}>
                {i < 2 && <Icon n="check" s={11} w={2.2} />}
              </span>
              <span className={i < 2 ? 'text-[#8A93A3] line-through' : ''}>{t}</span>
            </p>
          ))}
        </div>
      </div>
    </div>
  );
}

const ABSTRACT =
  'We analyse temperature profiles from 3,900 autonomous floats between 2005 and 2022 and estimate heat content change in the upper 2000 m of the ocean. Regional trends are computed on a one-degree grid and corrected for the sampling bias of the pre-float record.';

function PaperApp() {
  return (
    <div className="flex h-full w-full flex-col bg-[#3A3D45] text-[#E6E9F0]">
      <TitleBar title="ocean-heat-2024.pdf - Reader" dark glyph="#C93450" />
      <div className="flex h-[34px] shrink-0 items-center justify-between bg-[#2B2E35] px-4 text-[12px] text-[#AEB5C4]">
        <span>7 / 24</span>
        <span className="flex items-center gap-3"><Icon n="search" s={14} /> 125%</span>
      </div>
      <div className="flex min-h-0 flex-1">
        <div className="w-[64px] shrink-0 space-y-2.5 bg-[#2B2E35] px-2.5 py-3">
          {[0, 1, 2, 3].map((i) => (
            <div key={i} className={`aspect-[3/4] rounded-[3px] bg-white ${i === 2 ? 'ring-2 ring-[#7C93FF]' : 'opacity-80'}`} />
          ))}
        </div>
        <div className="min-w-0 flex-1 overflow-hidden px-6 pt-5">
          <div className="mx-auto h-[130%] max-w-[420px] rounded-[2px] bg-white px-7 py-6 text-[#1B1F29] shadow-xl">
            <p className="text-center font-display text-[18px] font-bold leading-tight">Upper-ocean heat content and its regional trends</p>
            <p className="mt-1.5 text-center text-[10.5px] text-[#5B6474]">R. Okafor, L. Haugen, M. Tanaka</p>
            <p className="mt-4 text-[9px] font-bold uppercase tracking-wide">Abstract</p>
            <p className="mt-1 text-[10px] leading-[1.55]">
              {ABSTRACT.slice(0, 120)}
              <mark className="bg-[#F4D37A] text-inherit">{ABSTRACT.slice(120, 250)}</mark>
              {ABSTRACT.slice(250)}
            </p>
            <div className="mt-3.5 grid grid-cols-2 gap-4 text-[9.5px] leading-[1.5] text-[#2A303C]">
              <p>
                The warming signal is largest in the subtropical gyres, where surface heat is mixed downward by winter convection and carried below the
                mixed layer. Between 30°N and 30°S, the upper 700 m gained heat in every basin we examined.
              </p>
              <p>
                Uncertainty is dominated by sparse sampling in the Southern Ocean before 2007. After the float network reached full coverage, the
                sampling error falls below the size of the regional signal in all but the highest latitudes.
              </p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function LineChart({ data, xLabels, yTicks, color, h, dark }: { data: number[]; xLabels: string[]; yTicks: number[]; color: string; h: number; dark?: boolean }) {
  const id = useSvgId();
  const [w, padL, padR, padT, padB] = [420, 38, 14, 12, 26];
  const [min, max] = [Math.min(...yTicks), Math.max(...yTicks)];
  const X = (i: number, n = data.length) => padL + (i / (n - 1)) * (w - padL - padR);
  const Y = (v: number) => padT + (1 - (v - min) / (max - min)) * (h - padT - padB);
  const line = data.map((v, i) => `${i ? 'L' : 'M'}${X(i).toFixed(1)} ${Y(v).toFixed(1)}`).join(' ');
  const [grid, txt] = dark ? ['#2A2F3B', '#7B8499'] : ['#E4E7EC', '#8A93A3'];
  const last = data.length - 1;
  return (
    <svg viewBox={`0 0 ${w} ${h}`} className="block h-full w-full" aria-hidden="true" fontSize="10" fill={txt}>
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor={color} stopOpacity="0.28" />
          <stop offset="1" stopColor={color} stopOpacity="0" />
        </linearGradient>
      </defs>
      {yTicks.map((t) => (
        <g key={t}>
          <line x1={padL} x2={w - padR} y1={Y(t)} y2={Y(t)} stroke={grid} />
          <text x={padL - 8} y={Y(t) + 3.5} textAnchor="end">{t}</text>
        </g>
      ))}
      {xLabels.map((l, i) => (
        <text key={l} x={X(i, xLabels.length)} y={h - 8} textAnchor="middle">{l}</text>
      ))}
      <path d={`${line} L${X(last).toFixed(1)} ${h - padB} L${X(0).toFixed(1)} ${h - padB} Z`} fill={`url(#${id})`} />
      <path d={line} fill="none" stroke={color} strokeWidth="2" strokeLinejoin="round" strokeLinecap="round" />
      <circle cx={X(last)} cy={Y(data[last])} r="4.5" fill={dark ? '#14161B' : '#FFFFFF'} stroke={color} strokeWidth="2" />
    </svg>
  );
}

const SST = Array.from({ length: 41 }, (_, i) => 0.02 * i + 0.06 * Math.sin(i * 0.9) + 0.03 * Math.sin(i * 2.3) - 0.05);
const VIEWERS = Array.from({ length: 36 }, (_, i) => 420 + i * 22 + 90 * Math.sin(i * 0.8) + 60 * Math.sin(i * 2.1) + (i > 24 ? 160 : 0));

function ChartApp({ variant = 'sst' }: { variant?: string }) {
  const dark = variant === 'viewers';
  return (
    <div className={`flex h-full w-full flex-col ${dark ? 'bg-[#14161B] text-[#E6E9F0]' : 'bg-white text-[#1B1F29]'}`}>
      <TitleBar title={dark ? 'Stats' : 'Sea surface temperature - Data'} dark={dark} glyph={dark ? '#12786A' : '#2B44D9'} />
      <div className="flex min-h-0 flex-1 flex-col px-5 py-4">
        <p className={`text-[12px] ${dark ? 'text-[#8E97AA]' : 'text-[#8A93A3]'}`}>{dark ? 'Concurrent viewers' : 'Sea surface temperature anomaly, global mean'}</p>
        {dark ? (
          <p className="flex items-baseline gap-2 text-[28px] font-semibold leading-tight">
            1,247 <span className="flex items-center gap-1.5 text-[12px] font-semibold text-[#5FD3B6]"><span className="h-2 w-2 rounded-full bg-[#5FD3B6]" /> Live</span>
          </p>
        ) : (
          <p className="flex items-baseline gap-2 text-[28px] font-semibold leading-tight">
            +0.87 °C <span className="text-[12px] font-semibold text-[#C93450]">since 1980</span>
          </p>
        )}
        <div className="mt-2 min-h-0 flex-1">
          {dark ? (
            <LineChart dark color="#5FD3B6" data={VIEWERS} xLabels={['8 pm', '8:30', '9 pm', '9:30', '10 pm']} yTicks={[0, 500, 1000, 1500]} h={170} />
          ) : (
            <LineChart color="#2B44D9" data={SST} xLabels={['1980', '1990', '2000', '2010', '2020']} yTicks={[-0.2, 0, 0.4, 0.8]} h={190} />
          )}
        </div>
      </div>
    </div>
  );
}

/* ── broadcast software ────────────────────────────────────────────────── */
function Meter({ label, level, icon }: { label: string; level: number; icon: string }) {
  return (
    <div className="flex items-center gap-2.5 text-[11.5px] text-[#AEB5C4]">
      <span className="flex w-[70px] items-center gap-1.5"><Icon n={icon} s={13} /> {label}</span>
      <div className="flex flex-1 gap-[2px]">
        {Array.from({ length: 22 }, (_, i) => i / 22).map((f, i) => (
          <span key={i} className="h-[9px] flex-1 rounded-[1.5px]" style={{ background: f >= level ? '#2A2F3B' : f > 0.82 ? '#C93450' : f > 0.62 ? '#F4B13A' : '#5FD3B6' }} />
        ))}
      </div>
    </div>
  );
}

function StreamApp() {
  const sky = useSvgId();
  return (
    <div className="flex h-full w-full flex-col bg-[#14161B] text-[#E6E9F0]">
      <TitleBar title="Broadcast - Scene: Main" dark glyph="#2B44D9" />
      <div className="flex min-h-0 flex-1 gap-3 p-3">
        <div className="flex min-w-0 flex-1 flex-col gap-2.5">
          <div className="relative min-h-0 flex-1 overflow-hidden rounded-[8px] bg-black">
            <svg viewBox="0 0 640 360" className="absolute inset-0 h-full w-full" preserveAspectRatio="xMidYMid slice" aria-hidden="true">
              <defs>
                <linearGradient id={sky} x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0" stopColor="#1B2442" />
                  <stop offset="0.7" stopColor="#4A3A6B" />
                  <stop offset="1" stopColor="#C9566B" />
                </linearGradient>
              </defs>
              <rect width="640" height="360" fill={`url(#${sky})`} />
              <circle cx="430" cy="200" r="46" fill="#F4B13A" opacity="0.95" />
              <path d="M0 270 L110 170 L190 240 L280 150 L400 262 L470 210 L640 290 V360 H0Z" fill="#241C3F" />
              <path d="M0 310 L90 250 L180 300 L300 240 L420 305 L540 255 L640 300 V360 H0Z" fill="#150F2B" />
              <rect x="372" y="22" width="248" height="38" rx="8" fill="#000" opacity="0.55" />
              <text x="390" y="47" fontSize="16" fill="#fff" fontWeight="600">New follower: nova_k</text>
            </svg>
            <span className="absolute left-3 top-3 flex items-center gap-1.5 rounded-[5px] bg-[#C93450] px-2 py-[3px] text-[11px] font-bold">
              <span className="h-1.5 w-1.5 rounded-full bg-white" /> LIVE
            </span>
            <div className="absolute bottom-3 right-3 h-[26%] w-[22%] overflow-hidden rounded-[8px] border-2 border-[#2B44D9] bg-[#222A3D]">
              <svg viewBox="0 0 100 80" className="h-full w-full" aria-hidden="true">
                <circle cx="50" cy="34" r="15" fill="#E0B9A0" />
                <path d="M16 80c3-22 18-28 34-28s31 6 34 28z" fill="#2B44D9" />
              </svg>
            </div>
          </div>
          <div className="flex shrink-0 items-center justify-between text-[12px]">
            <span className="rounded-[6px] bg-[#C93450] px-3.5 py-[7px] font-semibold text-white">Stop streaming</span>
            <span className="text-[#8E97AA]">02:14:07 · 1080p 60 fps · 6,000 kbps</span>
          </div>
        </div>
        <div className="flex w-[210px] shrink-0 flex-col gap-3 text-[12px]">
          <div>
            <p className="mb-1.5 text-[11px] font-semibold text-[#6B7487]">Scenes</p>
            {['Main', 'Starting soon', 'Be right back'].map((s, i) => (
              <p key={s} className={`rounded-[5px] px-2.5 py-[5px] ${i ? 'text-[#AEB5C4]' : 'bg-[#2B44D9] text-white'}`}>{s}</p>
            ))}
          </div>
          <div>
            <p className="mb-1.5 text-[11px] font-semibold text-[#6B7487]">Sources</p>
            {['Game capture', 'Webcam', 'Alerts overlay'].map((s) => (
              <p key={s} className="flex items-center gap-2 px-1 py-[3px] text-[#AEB5C4]"><Icon n="eye" s={12} /> {s}</p>
            ))}
          </div>
          <div className="mt-auto space-y-2.5">
            <Meter label="Mic" level={0.58} icon="mic" />
            <Meter label="Desktop" level={0.74} icon="spk" />
          </div>
        </div>
      </div>
    </div>
  );
}

/* ── windows ───────────────────────────────────────────────────────────── */
export const AppBody = memo(function AppBody({ win }: { win: Pick<Win, 'kind' | 'variant'> }) {
  const v = win.variant;
  switch (win.kind) {
    case 'editor': return <EditorApp />;
    case 'terminal': return <TerminalApp />;
    case 'browser': return <BrowserApp variant={v} />;
    case 'chat': return <ChatApp variant={v} />;
    case 'design': return <DesignApp />;
    case 'notes': return <NotesApp variant={v} />;
    case 'paper': return <PaperApp />;
    case 'chart': return <ChartApp variant={v} />;
    case 'stream': return <StreamApp />;
  }
});

/** A window. With Mosaïk it wears its zone frame and tag; plain, it is just a window; flat, it is maximised. */
export function ZoneFrame({ tag, zone, plain, flat, children }: { tag: string; zone: Win['zone']; plain?: boolean; flat?: boolean; children: ReactNode }) {
  const z = ZONE[zone];
  return (
    <div className="relative h-full w-full">
      <span
        className="absolute -top-[25px] left-0 flex h-[21px] items-center rounded-full px-[11px] text-[11.5px] font-semibold leading-none transition-opacity duration-300"
        style={{ background: z.bg, color: z.fg, opacity: plain ? 0 : 1 }}
      >
        {tag}
      </span>
      <div
        className="h-full w-full overflow-hidden transition-[box-shadow] duration-300"
        style={{
          borderRadius: flat ? 0 : 10,
          boxShadow: flat ? 'none' : plain ? '0 0 0 1px rgba(255,255,255,0.14), 0 18px 40px -14px rgba(0,0,0,0.65)' : `0 0 0 2px ${z.bg}, 0 22px 44px -12px rgba(0,0,0,0.6)`,
        }}
      >
        {children}
      </div>
    </div>
  );
}

export const AppWindow = ({ win, plain, flat }: { win: Win; plain?: boolean; flat?: boolean }) => (
  <ZoneFrame tag={win.tag} zone={win.zone} plain={plain} flat={flat}>
    <AppBody win={win} />
  </ZoneFrame>
);
__MK_EOF__
put components/desk.tsx <<'__MK_EOF__'
'use client';
/* The demo desktops: the interactive one in the hero, the feature illustrations and the workspace thumbnails. */
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { AppBody, AppWindow, EditorApp, ZoneFrame } from '@/components/apps';
import { Icon } from '@/components/ui';
import { DESK_H, DESK_H_TALL, SWITCHER_KEYS, TILES, WORKSPACES, ZONE, deskLayout, slotBox, type Rect, type Win, type Zone } from '@/lib/data';

/** A 1280 px wide canvas scaled to its container in pure CSS, so it is right from the first paint. */
const Scaled = ({ tall, className = '', children }: { tall?: boolean; className?: string; children: ReactNode }) => (
  <div className={`desk ${tall ? 'desk-tall' : ''} ${className}`}>
    <div className="desk-in">{children}</div>
  </div>
);

/** Mosaïk's icon in the tray: the four tiles. */
const TrayIcon = () => (
  <svg viewBox="0 0 16 16" width={14} height={14} aria-hidden="true" className="shrink-0">
    <rect x="1" y="1" width="8" height="8" rx="1.6" fill={TILES[0]} />
    <rect x="10.5" y="1" width="4.5" height="4.5" rx="1.2" fill={TILES[1]} />
    <rect x="10.5" y="7" width="4.5" height="8" rx="1.2" fill={TILES[2]} />
    <rect x="1" y="10.5" width="8" height="4.5" rx="1.2" fill={TILES[3]} />
  </svg>
);

/** A workspace's layout in small: its zones, in their colours, where they are. */
const LayoutGlyph = ({ windows }: { windows: Win[] }) => (
  <svg viewBox="0 0 16 16" width={16} height={16} aria-hidden="true" className="shrink-0">
    {windows.map(({ id, slot: [c, r, cw, rh], zone }) => (
      <rect key={id} x={1 + (c / 12) * 14 + 0.5} y={1 + (r / 8) * 14 + 0.5} width={(cw / 12) * 14 - 1} height={(rh / 8) * 14 - 1} rx="1.2" fill={ZONE[zone].bg} />
    ))}
  </svg>
);

/** Dark wallpaper: a faint field of tiles with two soft pools of colour. */
const Wallpaper = () => <div className="wallpaper absolute inset-0" />;

/** The Windows taskbar, there with or without Mosaïk: apps in the middle, Mosaïk among the tray icons. */
function Taskbar() {
  return (
    <div className="absolute inset-x-0 bottom-0 z-[30] h-[40px] bg-[#0B0D12]/90" aria-hidden="true">
      <span className="absolute inset-y-0 left-1/2 flex -translate-x-1/2 items-center gap-2">
        <span className="mr-1 grid h-[22px] w-[22px] grid-cols-2 gap-[2px]">
          {TILES.map((c) => (
            <span key={c} className="rounded-[2px] bg-white/55" />
          ))}
        </span>
        {TILES.map((c, i) => (
          <span key={c} className={`h-[26px] w-[34px] rounded-[5px] ${i ? 'bg-white/[0.07]' : 'bg-white/15'}`}>
            <span className="mx-auto mt-[8px] block h-[10px] w-[10px] rounded-[3px]" style={{ background: c }} />
          </span>
        ))}
      </span>
      <span className="absolute inset-y-0 right-4 flex items-center gap-3 text-[12px] text-white/60">
        <TrayIcon />
        10:42
      </span>
    </div>
  );
}

/* ── the interactive desktop ───────────────────────────────────────────── */
type Move = { x: number; y: number; sx?: number; sy?: number };
type Look = { m: Move; mt: Move; o: number; t: string };
const px = (n: number) => `${n}px`;
const EASE = 'cubic-bezier(.22,1,.36,1)';
const EXIT = 'cubic-bezier(.4,0,1,1)';
const HOME: Move = { x: 0, y: 0 };
const nudge = (m: Move, dy: number): Move => ({ ...m, y: m.y + dy });
const reduced = () => typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches;

type Phase = 'enter' | 'idle' | 'out';

/** Both desk heights at once: CSS picks the tall one on phones, so nothing jumps after load. */
function geometry(index: number) {
  const [a, b] = [deskLayout(index, DESK_H), deskLayout(index, DESK_H_TALL)];
  const main = a.main;
  const toFull = (L: typeof a): Move => {
    const r = L.rects[main];
    return { x: L.full.left + L.full.width / 2 - (r.left + r.width / 2), y: L.full.top + L.full.height / 2 - (r.top + r.height / 2), sx: L.full.width / r.width, sy: L.full.height / r.height };
  };
  const inv = (m: Move): Move => ({ x: -m.x, y: -m.y, sx: 1 / (m.sx ?? 1), sy: 1 / (m.sy ?? 1) });
  return { a, b, main, full: toFull(a), fullT: toFull(b), home: inv(toFull(a)), homeT: inv(toFull(b)) };
}

function vars(r: Rect, rt: Rect, look: Look, z: number): CSSProperties {
  return {
    '--l': px(r.left), '--w': px(r.width), '--t': px(r.top), '--h': px(r.height), '--tt': px(rt.top), '--ht': px(rt.height),
    '--x': px(look.m.x), '--y': px(look.m.y), '--yt': px(look.mt.y), '--sx': look.m.sx ?? 1, '--sy': look.m.sy ?? 1, '--syt': look.mt.sy ?? 1,
    opacity: look.o, transition: look.t, zIndex: z,
  } as CSSProperties;
}

export function Stage() {
  const [sel, setSel] = useState(0); // the workspace picked
  const [shown, setShown] = useState(0); // the workspace on screen
  const [ordered, setOrdered] = useState(false);
  const [phase, setPhase] = useState<Phase>('idle');
  const want = useRef(0);
  const intro = useRef<number | undefined>(undefined);
  const stage = useRef<HTMLDivElement>(null);

  // One orchestrated moment: the desktop starts messy, then every window finds its zone.
  useEffect(() => {
    intro.current = window.setTimeout(() => setOrdered(true), reduced() ? 0 : 1400);
    return () => window.clearTimeout(intro.current);
  }, []);

  // New windows mount at their starting pose; flush that style, then let them travel.
  useLayoutEffect(() => {
    if (phase !== 'enter') return;
    void stage.current?.offsetWidth;
    setPhase('idle');
  }, [phase]);

  const choose = useCallback(
    (i: number) => {
      setSel(i);
      want.current = i;
      if (phase === 'out' || (i === shown && phase === 'idle')) return;
      setPhase('out');
      window.setTimeout(() => {
        setShown(want.current);
        setPhase('enter');
      }, reduced() ? 0 : ordered ? 380 : 170);
    },
    [phase, shown, ordered],
  );

  const setState = (next: boolean) => {
    window.clearTimeout(intro.current);
    setOrdered(next);
  };

  const ws = WORKSPACES[shown];
  const g = geometry(shown);
  const topZ = Math.max(...g.a.z);

  const windows = ws.windows.flatMap((w, i) => {
    const st = `${i * (ordered ? 0.06 : 0.05)}s`;
    const pose = g.a.poses[i];
    const poseT = { x: pose.x, y: g.b.poses[i].y };
    const z = g.a.z[i];
    const dim = ordered || z === topZ ? 0 : 0.2;

    if (i !== g.main) {
      const look: Look =
        phase === 'enter'
          ? { m: ordered ? pose : nudge(pose, 26), mt: ordered ? poseT : nudge(poseT, 26), o: 0, t: 'none' }
          : phase === 'out'
            ? ordered
              ? { m: pose, mt: poseT, o: 0, t: `transform .3s ${EXIT} ${i * 0.025}s, opacity .075s linear ${0.225 + i * 0.025}s` }
              : { m: nudge(pose, 10), mt: nudge(poseT, 10), o: 0, t: 'transform .16s ease-in, opacity .16s ease-in' }
            : ordered
              ? { m: HOME, mt: HOME, o: 1, t: `transform .8s ${EASE} ${st}, opacity .1s ease-out ${st}` }
              : { m: pose, mt: poseT, o: 1, t: `transform .8s ${EASE} ${st}, opacity .24s ease-out ${st}` };
      return [
        <div key={`${ws.id}-${w.id}`} className="win" style={vars(g.a.rects[i], g.b.rects[i], look, z)}>
          <div className="relative h-full pt-[28px]">
            <AppWindow win={w} plain={!ordered} />
            <div className="dim inset-x-0 bottom-0 top-[28px] rounded-[10px]" style={{ opacity: dim }} />
          </div>
        </div>,
      ];
    }

    // The main window is the one people keep maximised. It exists at both sizes and the two cross-fade while one
    // travels to the other's rectangle, so its text is never stretched or re-flowed on screen.
    const tiled: Look =
      phase === 'out'
        ? { m: g.full, mt: g.fullT, o: 0, t: `transform .28s ${EXIT}, opacity .28s ${EXIT}` }
        : phase === 'enter' || !ordered
          ? { m: g.full, mt: g.fullT, o: 0, t: phase === 'enter' ? 'none' : `transform .8s ${EASE} ${st}, opacity .22s ease-out` }
          : { m: HOME, mt: HOME, o: 1, t: `transform .8s ${EASE} ${st}, opacity .25s ease-out .18s` };
    const max: Look =
      phase === 'enter'
        ? ordered
          ? { m: g.home, mt: g.homeT, o: 0, t: 'none' }
          : { m: nudge(HOME, 26), mt: nudge(HOME, 26), o: 0, t: 'none' }
        : phase === 'out'
          ? ordered
            ? { m: g.home, mt: g.homeT, o: 0, t: 'opacity .12s' }
            : { m: nudge(HOME, 10), mt: nudge(HOME, 10), o: 0, t: 'transform .16s ease-in, opacity .16s ease-in' }
          : ordered
            ? { m: g.home, mt: g.homeT, o: 0, t: `transform .8s ${EASE} ${st}, opacity .22s ease-out` }
            : { m: HOME, mt: HOME, o: 1, t: `transform .8s ${EASE} ${st}, opacity .25s ease-out .18s` };
    return [
      <div key={`${ws.id}-${w.id}`} className="win pointer-events-none" style={vars(g.a.rects[i], g.b.rects[i], tiled, z)}>
        <div className="h-full pt-[28px]">
          <AppWindow win={w} plain={!ordered} />
        </div>
      </div>,
      <div key={`${ws.id}-${w.id}-max`} className="win pointer-events-none" style={vars(g.a.full, g.b.full, max, z)}>
        <div className="relative h-full">
          <AppWindow win={w} plain flat />
          <div className="dim inset-0" style={{ opacity: dim }} />
        </div>
      </div>,
    ];
  });

  return (
    <div>
      <div ref={stage} className="stage-shadow overflow-hidden rounded-[14px] sm:rounded-[20px]" role="group" aria-label="Interactive demo of a Mosaïk desktop">
        <Scaled tall>
          <Wallpaper />
          <Taskbar />
          {windows}
        </Scaled>
      </div>

      <div className="mt-5 flex flex-col gap-5 lg:flex-row lg:items-center lg:justify-between">
        <div role="group" aria-label="Workspaces" className="flex flex-wrap gap-2">
          {WORKSPACES.map((w, i) => (
            <button
              key={w.id}
              type="button"
              onClick={() => choose(i)}
              aria-pressed={i === sel}
              className={`flex min-h-[44px] cursor-pointer items-center gap-2.5 rounded-full border px-4 text-[14px] font-semibold transition-colors ${
                i === sel ? 'border-ink bg-ink text-paper' : 'border-ink/20 text-ink hover:border-ink/60'
              }`}
            >
              <LayoutGlyph windows={w.windows} />
              {w.label}
            </button>
          ))}
        </div>
        <div className="inline-flex shrink-0 self-start rounded-full border border-ink/20 p-1 lg:self-auto" role="group" aria-label="Desktop state">
          {[false, true].map((v) => (
            <button
              key={String(v)}
              type="button"
              onClick={() => setState(v)}
              aria-pressed={ordered === v}
              className={`min-h-[38px] cursor-pointer rounded-full px-4 text-sm font-semibold transition-colors ${ordered === v ? 'bg-lapis text-white' : 'text-slate hover:text-ink'}`}
            >
              {v ? 'With Mosaïk' : 'Without Mosaïk'}
            </button>
          ))}
        </div>
      </div>
      <p className="mt-4 max-w-xl text-[15px] leading-relaxed text-slate" aria-live="polite">
        <span className="font-semibold text-ink">{WORKSPACES[sel].name}.</span> {WORKSPACES[sel].summary}
      </p>
    </div>
  );
}

/* ── feature illustrations ─────────────────────────────────────────────── */
const Cursor = ({ x, y }: { x: number; y: number }) => (
  <svg className="absolute z-[60]" style={{ left: x, top: y }} width="44" height="44" viewBox="0 0 24 24" aria-hidden="true">
    <path d="M5 3l14 7-6 2-2 6z" fill="#FFFFFF" stroke="#161B25" strokeWidth="1.4" strokeLinejoin="round" />
  </svg>
);

function ZoneBox({ x, y, w, h, zone, filled }: { x: number; y: number; w: number; h: number; zone: Zone; filled?: boolean }) {
  const c = ZONE[zone].bg;
  return <div className="absolute rounded-[18px]" style={{ left: x, top: y, width: w, height: h, border: `2px ${filled ? 'solid' : 'dashed'} ${c}`, background: `${c}${filled ? '33' : '14'}` }} />;
}

/** A window placed by hand on a still desktop. `tilt` rotates it as if it were being dragged. */
function Placed({ win, x, y, w, h, tilt, z }: { win: Pick<Win, 'kind' | 'variant' | 'tag' | 'zone'>; x: number; y: number; w: number; h: number; tilt?: number; z?: number }) {
  return (
    <div className="absolute" style={{ left: x, top: y, width: w, height: h, transform: tilt ? `rotate(${tilt}deg)` : undefined, zIndex: z }}>
      <ZoneFrame tag={win.tag} zone={win.zone}>
        <AppBody win={win} />
      </ZoneFrame>
    </div>
  );
}

const ART = 'rounded-[20px] bg-ink ring-1 ring-black/10';

/** Zones: named areas. A window dragged near one is pulled into it. */
export function ZonesArt() {
  return (
    <div role="img" aria-label="A desktop divided into three coloured zones. An editor and a terminal already sit in theirs, and a pull request window is being dragged toward the empty Review zone.">
      <Scaled className={ART}>
        <Wallpaper />
        <ZoneBox x={40} y={52} w={720} h={700} zone="lapis" />
        <ZoneBox x={788} y={52} w={452} h={340} zone="verdigris" />
        <ZoneBox x={788} y={410} w={452} h={342} zone="saffron" filled />
        <Placed win={{ kind: 'editor', tag: 'Code', zone: 'lapis' }} x={56} y={96} w={688} h={640} />
        <Placed win={{ kind: 'terminal', tag: 'Terminal', zone: 'verdigris' }} x={804} y={96} w={420} h={280} />
        <p className="absolute text-center text-[18px] font-semibold text-[#F4B13A]" style={{ left: 788, top: 560, width: 452 }}>
          Release to place in Review
        </p>
        <Placed win={{ kind: 'browser', variant: 'pr', tag: 'Review', zone: 'saffron' }} x={836} y={318} w={420} h={300} tilt={-5} z={40} />
        <Cursor x={1070} y={330} />
      </Scaled>
    </div>
  );
}

const TABS: [string, string][] = [['Editor', '#2B44D9'], ['Terminal', '#12786A'], ['Browser', '#F4B13A'], ['Design', '#C93450']];

/** Tabs: any app can join a tab strip, not only a browser. */
export function TabsArt() {
  return (
    <div role="img" aria-label="One window with a tab strip. Editor, terminal, browser and design tools are tabs of the same frame, and a chat window is being dragged toward a new-tab slot.">
      <Scaled className={ART}>
        <Wallpaper />
        <div className="absolute" style={{ left: 36, top: 52, width: 1090, height: 710 }}>
          <ZoneFrame tag="Code" zone="lapis">
            <div className="flex h-full flex-col bg-[#16181D]">
              <div className="flex h-[46px] shrink-0 items-end gap-1.5 bg-[#101216] px-3 text-[14px]">
                {TABS.map(([name, c], i) => (
                  <div key={name} className={`flex h-[38px] items-center gap-2.5 rounded-t-[10px] px-5 ${i ? 'text-[#8E97AA]' : 'bg-[#1E2027] font-semibold text-white'}`} style={i ? undefined : { boxShadow: `inset 0 3px 0 ${c}` }}>
                    <span className="h-3 w-3 rounded-[4px]" style={{ background: c }} />
                    {name}
                  </div>
                ))}
                <div className="mb-1.5 ml-1 flex h-[30px] items-center gap-1.5 rounded-[8px] border-2 border-dashed border-[#F4B13A] px-3 text-[13px] font-semibold text-[#F4B13A]">
                  <Icon n="plus" s={14} w={2} /> Drop to add
                </div>
              </div>
              <div className="min-h-0 flex-1">
                <EditorApp bare />
              </div>
            </div>
          </ZoneFrame>
        </div>
        <Placed win={{ kind: 'chat', variant: 'team', tag: 'Team', zone: 'madder' }} x={880} y={46} w={350} h={236} tilt={5} z={40} />
        <Cursor x={1030} y={58} />
      </Scaled>
    </div>
  );
}

const Key = ({ children, dim }: { children: ReactNode; dim?: boolean }) => (
  <span className={`rounded-[6px] border border-white/15 px-1.5 py-0.5 text-[11.5px] ${dim ? 'text-[#8E97AA]' : ''}`}>{children}</span>
);

/** What the switcher calls each kind of app, under a workspace's name. */
const APP_WORDS: Record<Win['kind'], string> = {
  editor: 'Code editor',
  terminal: 'Terminal',
  browser: 'Browser',
  chat: 'Chat',
  design: 'Design app',
  notes: 'Notes',
  paper: 'PDF reader',
  chart: 'Charts',
  stream: 'Broadcast',
};

/** Saved layouts: Mosaïk's switcher as the app draws it (Win+Alt+Space opens it; 1 to 9 pick). */
export function SwitcherArt() {
  return (
    <div className="switcher-bg relative overflow-hidden rounded-[20px] bg-ink px-5 py-10 ring-1 ring-black/10 sm:px-10 sm:py-14">
      <div className="mx-auto w-full max-w-[470px] overflow-hidden rounded-[16px] border border-white/10 bg-[#1B2130] text-[#EDEFEA] shadow-[0_30px_70px_-20px_rgba(0,0,0,0.7)]" role="img" aria-label="Mosaïk's switcher, opened with Windows, Alt and Space: four saved workspaces, each with its layout in small and its apps, each picked with its number. Brand refresh is selected.">
        <div className="flex h-[52px] items-center gap-3 border-b border-white/10 pl-5 pr-4 text-[15px] text-[#A0A9B8]">
          <span className="flex-1">Switch workspace</span>
          <span className="flex gap-1.5">
            {SWITCHER_KEYS.map((k) => (
              <Key key={k}>{k}</Key>
            ))}
          </span>
        </div>
        <div className="p-2">
          {WORKSPACES.map((w, i) => (
            <div key={w.id} className={`relative flex items-center gap-4 rounded-[10px] px-3 py-2.5 ${w.id === 'brand' ? 'bg-[#262E3F]' : ''}`}>
              {w.id === 'brand' && <span className="absolute inset-y-2.5 left-0 w-[3px] rounded-full bg-[#8A9BFF]" />}
              <div className="relative h-[40px] w-[64px] shrink-0 overflow-hidden rounded-[6px] bg-[#0D1016] ring-1 ring-white/10">
                {w.windows.map(({ id, slot: [c, r, cw, rh], zone }) => (
                  <span key={id} className="absolute rounded-[2px]" style={{ left: `${(c / 12) * 100 + 2}%`, top: `${(r / 8) * 100 + 4}%`, width: `${(cw / 12) * 100 - 3}%`, height: `${(rh / 8) * 100 - 6}%`, background: ZONE[zone].bg }} />
                ))}
              </div>
              <div className="min-w-0 flex-1">
                <p className="truncate text-[15px] font-semibold">
                  {w.name}
                  {i === 0 && <span className="ml-2 text-[12.5px] text-[#8A9BFF]">Open</span>}
                </p>
                <p className="truncate text-[12.5px] text-[#A0A9B8]">{w.windows.map((x) => APP_WORDS[x.kind]).join(', ')}</p>
              </div>
              <Key dim>{i + 1}</Key>
            </div>
          ))}
        </div>
        <div className="flex items-center gap-5 border-t border-white/10 px-5 py-2.5 text-[12px] text-[#A0A9B8]">
          <span><b className="font-semibold text-[#EDEFEA]">Enter</b> open</span>
          <span><b className="font-semibold text-[#EDEFEA]">S</b> save current layout</span>
          <span><b className="font-semibold text-[#EDEFEA]">Esc</b> close</span>
        </div>
      </div>
    </div>
  );
}

/** A whole workspace as a still desktop, for thumbnails. */
export function DeskThumb({ index }: { index: number }) {
  return (
    <Scaled className="bg-ink">
      <Wallpaper />
      <Taskbar />
      {WORKSPACES[index].windows.map((w) => {
        const b = slotBox(w.slot, DESK_H);
        return (
          <div key={w.id} className="absolute" style={{ left: b.left, top: b.top, width: b.width, height: b.height }}>
            <AppWindow win={w} />
          </div>
        );
      })}
    </Scaled>
  );
}
__MK_EOF__
put components/header.tsx <<'__MK_EOF__'
'use client';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { DownloadButton, Wordmark } from '@/components/ui';

const NAV = [
  ['/#how', 'How it works'],
  ['/#people', 'Who it’s for'],
  ['/#platforms', 'Platforms'],
];

export function Header() {
  const home = usePathname() === '/';
  return (
    <header className="fixed inset-x-0 top-0 z-50 border-b border-grout/80 bg-plaster/85 backdrop-blur-md">
      <div className="page-x flex h-16 items-center justify-between">
        <Link
          href="/"
          aria-label="Mosaïk, home"
          className="text-ink"
          onClick={(e) => {
            if (home) {
              e.preventDefault();
              history.replaceState(null, '', '/');
              scrollTo({ top: 0 });
            }
          }}
        >
          <Wordmark className="block h-7 w-auto" decorative />
        </Link>
        <nav aria-label="Main" className="hidden items-center gap-8 md:flex">
          {NAV.map(([href, label]) => (
            <Link key={href} href={href} className="text-[15px] font-medium text-slate transition-colors hover:text-ink">
              {label}
            </Link>
          ))}
        </nav>
        <DownloadButton className="btn btn-lapis min-h-[40px] px-5 text-sm" label="Download" />
      </div>
    </header>
  );
}
__MK_EOF__
put components/ui.tsx <<'__MK_EOF__'
import Link from 'next/link';
import { TILES } from '@/lib/data';
import { site } from '@/lib/site';

/* Shared vector shapes live once in the page and are drawn with <use>, so the markup stays small. */
const ICONS: Record<string, string> = {
  min: 'M3 8h10',
  max: 'M3.5 3.5h9v9h-9z',
  close: 'M3.5 3.5l9 9M12.5 3.5l-9 9',
  file: 'M4 1.5h5l3 3v10H4z M9 1.5v3h3',
  search: 'M7 11.5a4.5 4.5 0 1 0 0-9 4.5 4.5 0 0 0 0 9z M10.5 10.5L14 14',
  plus: 'M8 3v10M3 8h10',
  hash: 'M6.5 2.5L5.5 13.5M11 2.5L10 13.5M3 6h10.5M2.5 10h10.5',
  back: 'M10 3L5 8l5 5',
  fwd: 'M6 3l5 5-5 5',
  reload: 'M13 8a5 5 0 1 1-1.6-3.7M13 2.5v3h-3',
  lock: 'M4.5 7h7v6.5h-7z M6 7V5.2a2 2 0 0 1 4 0V7',
  check: 'M3 8.5l3 3 7-7',
  chevD: 'M4 6l4 4 4-4',
  chevR: 'M6 4l4 4-4 4',
  send: 'M2 8l12-5-4 11-2.5-4.5z',
  move: 'M3 2l9 5-4 1.5L6.5 13z',
  frame: 'M5 2v12M11 2v12M2 5h12M2 11h12',
  shape: 'M2.5 2.5h11v11h-11z',
  pen: 'M3 13l1-3.5L11 2.5l2.5 2.5-7 7z',
  text: 'M3 3.5h10M8 3.5v9',
  hand: 'M5 8V4a1 1 0 0 1 2 0v3M7 7V3a1 1 0 0 1 2 0v4M9 7V4a1 1 0 0 1 2 0v5c0 2.5-1.5 4-3.5 4S4 11.5 3.5 9.5L3 8',
  branch: 'M5 2.5v8M5 13a1 1 0 1 0 0-.01M11 5a1 1 0 1 0 0-.01M11 6c0 3.5-6 2-6 4.5',
  eye: 'M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8z M8 9.8a1.8 1.8 0 1 0 0-3.6 1.8 1.8 0 0 0 0 3.6z',
  mic: 'M8 2.5a2 2 0 0 0-2 2V8a2 2 0 0 0 4 0V4.5a2 2 0 0 0-2-2z M4 7.5a4 4 0 0 0 8 0M8 11.5v2',
  spk: 'M2.5 6v4h2.5L9 13V3L5 6z M11.5 5.5a3.5 3.5 0 0 1 0 5',
  down: 'M8 2.5v8M4.5 7.5L8 11l3.5-3.5M3 13.5h10',
};

/** Bricolage Grotesque 800 outlines of the wordmark; the dots of the ï are drawn as two tiles. */
const WORDMARK =
  'M69 0V-660H310L482-185H487L655-660H885V0H733L742-496H731L540 0H408L221-496H210L219 0ZM1225 14Q1145 14 1084-17Q1022-48 987-110Q952-172 952-265Q952-358 987-420Q1022-481 1084-512Q1146-542 1226-542Q1306-542 1368-511Q1430-480 1465-418Q1500-357 1500-264Q1500-169 1464-107Q1428-45 1366-16Q1303 14 1225 14ZM1230-103Q1266-103 1290-120Q1314-136 1326-170Q1338-204 1338-254Q1338-307 1325-344Q1312-380 1286-400Q1261-419 1221-419Q1187-419 1162-402Q1138-386 1126-352Q1114-318 1114-267Q1114-185 1144-144Q1175-103 1230-103ZM1775 14Q1722 14 1679 4Q1636-5 1604-23Q1572-41 1552-66Q1531-92 1523-123L1643-176Q1650-159 1668-142Q1685-124 1714-113Q1744-102 1786-102Q1826-102 1848-114Q1871-125 1871-147Q1871-163 1858-172Q1846-182 1821-190Q1796-197 1759-204Q1721-212 1682-222Q1643-233 1609-252Q1575-270 1554-300Q1534-331 1534-378Q1534-427 1560-464Q1586-500 1638-521Q1689-542 1763-542Q1829-542 1879-525Q1929-508 1962-476Q1996-445 2009-401L1880-355Q1875-377 1860-393Q1844-409 1820-418Q1797-426 1765-426Q1726-426 1704-414Q1683-402 1683-382Q1683-366 1698-356Q1712-345 1739-338Q1766-331 1803-323Q1842-315 1880-304Q1919-294 1950-276Q1982-259 2000-230Q2019-202 2019-157Q2019-104 1991-66Q1963-27 1908-6Q1854 14 1775 14ZM2197 14Q2153 14 2119-4Q2085-21 2066-54Q2047-88 2047-136Q2047-181 2064-210Q2082-240 2116-258Q2149-275 2196-286Q2243-298 2303-307Q2332-312 2352-316Q2371-321 2381-331Q2391-341 2391-360Q2391-385 2373-403Q2355-421 2315-421Q2287-421 2264-411Q2242-401 2226-382Q2210-363 2202-336L2060-378Q2073-420 2096-451Q2120-482 2153-502Q2186-523 2228-532Q2271-542 2319-542Q2399-542 2450-516Q2500-491 2524-438Q2549-384 2549-300V-219Q2549-183 2550-146Q2552-110 2554-74Q2557-37 2561 0H2419Q2415-23 2411-56Q2407-88 2405-122H2386Q2372-84 2346-52Q2319-21 2282-4Q2244 14 2197 14ZM2270-100Q2288-100 2306-106Q2325-113 2342-124Q2359-136 2373-153Q2387-170 2394-190L2392-269L2414-264Q2395-252 2372-245Q2349-238 2326-234Q2302-230 2280-226Q2257-221 2240-214Q2222-207 2212-195Q2202-183 2202-162Q2202-134 2221-117Q2240-100 2270-100ZM2633 0V-528H2794V0ZM2884 0V-721H3042V-323Q3071-344 3096-368Q3122-392 3144-418Q3165-444 3182-472Q3200-499 3213-528H3397Q3384-489 3361-450Q3338-412 3305-380Q3272-347 3226-324Q3181-301 3124-293V-275Q3196-289 3242-276Q3289-262 3317-232Q3345-202 3360-162Q3375-122 3385-83L3406 0H3231L3221-50Q3211-99 3198-134Q3184-169 3159-188Q3134-208 3087-208H3042V0Z';

export function Sprite() {
  return (
    <svg width="0" height="0" className="absolute" aria-hidden="true">
      <defs>
        <path id="wm" d={WORDMARK} />
        {Object.entries(ICONS).map(([k, d]) => (
          <path key={k} id={`i-${k}`} d={d} />
        ))}
      </defs>
    </svg>
  );
}

export function Icon({ n, s = 14, w = 1.4 }: { n: string; s?: number; w?: number }) {
  return (
    <svg viewBox="0 0 16 16" width={s} height={s} strokeWidth={w} className="ic" aria-hidden="true">
      <use href={`#i-${n}`} />
    </svg>
  );
}

/** The Mosaïk wordmark, from the shared outlines. */
export function Wordmark({ className, fill = 'currentColor', tiles = TILES, decorative }: { className: string; fill?: string; tiles?: string[]; decorative?: boolean }) {
  return (
    <svg viewBox="59 -806 3357 830" className={className} {...(decorative ? { 'aria-hidden': true } : { role: 'img', 'aria-label': 'Mosaïk' })}>
      <use href="#wm" fill={fill} />
      <rect x="2538" y="-796" width="158" height="158" rx="26" fill={tiles[0]} />
      <rect x="2731" y="-796" width="158" height="158" rx="26" fill={tiles[1]} />
    </svg>
  );
}

/** A list item with a tick. */
export const Check = ({ children }: { children: React.ReactNode }) => (
  <li className="flex items-start gap-3">
    <span className="mt-[3px] text-lapis">
      <Icon n="check" s={16} w={2} />
    </span>
    {children}
  </li>
);

/** One click: Microsoft's small installer, which installs Mosaïk from the Store and starts it. */
export const DownloadButton = ({ className, label = 'Download for Windows' }: { className: string; label?: string }) => (
  <a href={site.download} className={`${className} gap-2.5`}>
    <Icon n="down" s={17} w={1.8} />
    {label}
  </a>
);

export function Footer() {
  const links: [string, string][] = [
    ['/download', 'Download'],
    ['/conditions', 'Privacy and terms'],
    [site.repo, 'GitHub'],
    [site.patreon, 'Support on Patreon'],
    [`mailto:${site.email}`, site.email],
  ];
  return (
    <footer className="border-t border-grout">
      <div className="page-x flex flex-col gap-8 py-12 md:flex-row md:items-end md:justify-between">
        <div className="space-y-3">
          <Wordmark className="block h-8 w-auto text-ink" />
          <p className="max-w-xs text-sm text-slate">Open source, released under CC0. Take it, change it, ship it.</p>
        </div>
        <nav aria-label="Footer" className="flex flex-wrap gap-x-8 gap-y-3 text-[15px] font-medium">
          {links.map(([href, label]) =>
            href.startsWith('/') ? (
              <Link key={href} href={href} className="text-slate hover:text-ink">
                {label}
              </Link>
            ) : (
              <a key={href} href={href} className="text-slate hover:text-ink" rel="noopener noreferrer">
                {label}
              </a>
            ),
          )}
        </nav>
      </div>
    </footer>
  );
}

const PLATFORMS = [
  ['Windows', 'Available', '#12786A', 'Windows 11, and Windows 10 version 22H2. Windows lets one app move, resize and group other apps’ windows, which is all Mosaïk needs.'],
  ['macOS', 'Planned', '#F4B13A', 'Next. Mosaïk will ask for the Accessibility permission, then place and group windows the same way.'],
  ['Linux', 'Planned, X11 first', '#2B44D9', 'X11 allows the full feature set. Wayland deliberately limits what an app can do to other apps’ windows, so support there depends on the desktop environment.'],
];

export function PlatformList() {
  return (
    <ul className="divide-y divide-grout border-y border-grout">
      {PLATFORMS.map(([name, status, color, text]) => (
        <li key={name} className="grid gap-2 py-6 md:grid-cols-12 md:gap-8">
          <h3 className="font-display text-[1.6rem] font-bold tracking-[-0.025em] md:col-span-3">{name}</h3>
          <p className="flex items-center gap-2.5 text-[15px] font-semibold md:col-span-3">
            <span className="h-2.5 w-2.5 rounded-full" style={{ backgroundColor: color }} aria-hidden="true" />
            {status}
          </p>
          <p className="max-w-xl text-[15.5px] leading-relaxed text-slate md:col-span-6">{text}</p>
        </li>
      ))}
    </ul>
  );
}
__MK_EOF__
put lib/data.ts <<'__MK_EOF__'
export type Zone = 'lapis' | 'verdigris' | 'saffron' | 'madder';
type Kind = 'editor' | 'terminal' | 'browser' | 'chat' | 'design' | 'notes' | 'paper' | 'chart' | 'stream';
/** [column, row, columns, rows] on a 12 x 8 grid */
type Slot = [number, number, number, number];
export interface Win { id: string; kind: Kind; variant?: string; tag: string; zone: Zone; slot: Slot }
export interface Workspace { id: string; label: string; name: string; role: string; summary: string; about: string; windows: Win[] }
export interface Rect { left: number; top: number; width: number; height: number }

export const ZONE: Record<Zone, { bg: string; fg: string }> = {
  lapis: { bg: '#2B44D9', fg: '#FFFFFF' },
  verdigris: { bg: '#12786A', fg: '#FFFFFF' },
  saffron: { bg: '#F4B13A', fg: '#161B25' },
  madder: { bg: '#C93450', fg: '#FFFFFF' },
};
export const TILES = ['#2B44D9', '#F4B13A', '#12786A', '#C93450'];

const w = (id: string, kind: Kind, tag: string, zone: Zone, slot: Slot, variant?: string): Win => ({ id, kind, tag, zone, slot, variant });

export const WORKSPACES: Workspace[] = [
  {
    id: 'release',
    label: 'Release',
    name: 'Ship the release',
    role: 'Developers',
    summary: 'Editor, terminal, pull request and team chat, switched as one.',
    about: 'Editor, terminal, pull request and team chat, all in place the moment you pick up a task.',
    windows: [
      w('ed', 'editor', 'Code', 'lapis', [0, 0, 7, 5]),
      w('tm', 'terminal', 'Terminal', 'verdigris', [0, 5, 7, 3]),
      w('pr', 'browser', 'Review', 'saffron', [7, 0, 5, 5], 'pr'),
      w('ch', 'chat', 'Team', 'madder', [7, 5, 5, 3], 'team'),
    ],
  },
  {
    id: 'brand',
    label: 'Brand',
    name: 'Brand refresh',
    role: 'Designers',
    summary: 'Canvas, references, the brief and the client thread, side by side.',
    about: 'The canvas, your references, the brief and the client thread, side by side instead of buried.',
    windows: [
      w('cv', 'design', 'Canvas', 'lapis', [0, 0, 8, 8]),
      w('rf', 'browser', 'References', 'saffron', [8, 0, 4, 3], 'refs'),
      w('br', 'notes', 'Brief', 'verdigris', [8, 3, 4, 2], 'brief'),
      w('cl', 'chat', 'Client', 'madder', [8, 5, 4, 3], 'client'),
    ],
  },
  {
    id: 'thesis',
    label: 'Thesis',
    name: 'Thesis, chapter 3',
    role: 'Researchers',
    summary: 'The paper you are reading, the draft you are writing and the data behind both.',
    about: 'The paper you are reading, the draft you are writing and the data behind both, one switch from the next project.',
    windows: [
      w('pp', 'paper', 'Reading', 'madder', [0, 0, 4, 8]),
      w('dr', 'notes', 'Draft', 'verdigris', [4, 0, 4, 8], 'draft'),
      w('dt', 'chart', 'Data', 'lapis', [8, 0, 4, 4], 'sst'),
      w('sr', 'browser', 'Sources', 'saffron', [8, 4, 4, 4], 'sources'),
    ],
  },
  {
    id: 'stream',
    label: 'Stream',
    name: 'Friday stream',
    role: 'Streamers',
    summary: 'Broadcast software, live chat and your dashboards, ready before you go live.',
    about: 'Broadcast software, live chat and dashboards, arranged before you go live and again for the next show.',
    windows: [
      w('ob', 'stream', 'Broadcast', 'lapis', [0, 0, 8, 5]),
      w('lc', 'chat', 'Chat', 'madder', [8, 0, 4, 8], 'live'),
      w('st', 'chart', 'Stats', 'verdigris', [0, 5, 4, 3], 'viewers'),
      w('db', 'browser', 'Dashboard', 'saffron', [4, 5, 4, 3], 'dash'),
    ],
  },
];

/* Every desktop is drawn on a fixed canvas and scaled to fit, so all sizes are plain pixels. */
const DESK_W = 1280;
export const DESK_H = 800;
export const DESK_H_TALL = 960; // phones get a taller desktop
const TAG = 28; // room above a window for its zone tag
const TASKBAR = 40; // the Windows taskbar, along the bottom
const [TOP, BOTTOM, SIDE, PAD] = [22, TASKBAR + 14, 26, 12];

/** The shortcut that opens Mosaïk's switcher (the app's default). */
export const SWITCHER_KEYS = ['Win', 'Alt', 'Space'];

const r1 = (n: number) => Math.round(n * 10) / 10;

export function slotBox([c, r, cols, rows]: Slot, deskH: number): Rect {
  const colW = (DESK_W - SIDE * 2) / 12;
  const rowH = (deskH - TOP - BOTTOM) / 8;
  return { left: r1(SIDE + c * colW + PAD / 2), top: r1(TOP + r * rowH + TAG), width: r1(cols * colW - PAD), height: r1(rows * rowH - TAG - PAD / 2) };
}

/** Where a maximised window sits: everything above the taskbar. */
const fullRect = (deskH: number): Rect => ({ left: 0, top: 0, width: DESK_W, height: deskH - TASKBAR });

function mulberry32(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

interface DeskLayout {
  /** each window's frame, including the strip above it for the zone tag */
  rects: Rect[];
  /** how far each window moves for the messy desktop */
  poses: { x: number; y: number }[];
  z: number[];
  /** the window people keep maximised without Mosaïk: the biggest one */
  main: number;
  full: Rect;
}

/**
 * How most people really work: one window maximised, the rest small windows parked on top of it along the
 * right edge and the bottom, overlapping it and each other a little. Windows keep their real size, so their
 * text never re-flows or re-scales while they move.
 */
export function deskLayout(index: number, deskH: number): DeskLayout {
  const rnd = mulberry32((index + 1) * 7919 + 13);
  const jit = (n: number) => Math.round((rnd() - 0.5) * 2 * n);
  const rects = WORKSPACES[index].windows.map((win) => {
    const b = slotBox(win.slot, deskH);
    return { left: b.left, top: b.top - TAG, width: b.width, height: b.height + TAG };
  });
  const order = rects.map((_, i) => i).sort((a, b) => rects[b].width * rects[b].height - rects[a].width * rects[a].height);
  const placed = rects.map((r) => ({ left: r.left, top: r.top }));
  const maxB = deskH - TASKBAR - 12;
  // Each desktop is messy in its own way. Fractions place a window inside the free range: [across, down].
  const WHERE: Record<string, [number, number]> = {
    tm: [1, 1], pr: [0.5, 0.1], ch: [0.02, 0.8], // developer: terminal bottom right, review mid-screen, chat tucked bottom left
    rf: [0.62, 0.02], br: [0.1, 0.55], cl: [1, 1], // designer: palettes floating around the canvas
    dr: [0, 0.3], dt: [0.88, 0], sr: [0.55, 1], // researcher: tall draft on the left, data up top, sources low
    lc: [1, 0.35], st: [0.03, 1], db: [0.3, 0.18], // streamer: chat down the right, dashboards on the left
  };
  order.slice(1).forEach((i) => {
    const r = rects[i];
    const [fx, fy] = WHERE[WORKSPACES[index].windows[i].id] ?? [1, 0.5];
    placed[i] = {
      left: Math.round(16 + fx * Math.max(DESK_W - r.width - 32, 0)) + (fx > 0 && fx < 1 ? jit(10) : 0),
      top: Math.round(TOP + fy * Math.max(maxB - r.height - TOP, 0)),
    };
  });
  return {
    rects,
    poses: rects.map((r, i) => ({ x: Math.round(placed[i].left - r.left), y: Math.round(placed[i].top - r.top) })),
    z: rects.map((_, i) => (i === order[0] ? 2 : 3 + order.indexOf(i))),
    main: order[0],
    full: fullRect(deskH),
  };
}
__MK_EOF__
put next.config.ts <<'__MK_EOF__'
import type { NextConfig } from 'next';

const security = [
  { key: 'X-Content-Type-Options', value: 'nosniff' },
  { key: 'Referrer-Policy', value: 'strict-origin-when-cross-origin' },
  { key: 'X-Frame-Options', value: 'DENY' },
  { key: 'Permissions-Policy', value: 'camera=(), microphone=(), geolocation=()' },
];

export default {
  poweredByHeader: false,
  agentRules: false, // don't drop AGENTS.md / CLAUDE.md into the site folder
  turbopack: { root: process.cwd() }, // this folder is the whole app, whatever sits around it
  headers: async () => [{ source: '/:path*', headers: security }],
} satisfies NextConfig;
__MK_EOF__
put package.json <<'__MK_EOF__'
{
  "name": "mosaik",
  "private": true,
  "scripts": {
    "dev": "next dev",
    "build": "next build",
    "start": "next start",
    "typecheck": "next typegen && tsc --noEmit"
  },
  "dependencies": {
    "@fontsource-variable/bricolage-grotesque": "5.3.0",
    "@fontsource-variable/instrument-sans": "5.3.0",
    "@fontsource-variable/jetbrains-mono": "5.3.0",
    "next": "16.3.8",
    "react": "19.3.0",
    "react-dom": "19.3.0"
  },
  "devDependencies": {
    "@tailwindcss/postcss": "4.3.3",
    "@types/node": "22.20.4",
    "@types/react": "19.3.0",
    "@types/react-dom": "19.3.0",
    "tailwindcss": "4.3.3",
    "typescript": "5.9.3"
  }
}
__MK_EOF__
put postcss.config.mjs <<'__MK_EOF__'
export default { plugins: { '@tailwindcss/postcss': {} } };
__MK_EOF__
put tsconfig.json <<'__MK_EOF__'
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": [
      "dom",
      "dom.iterable",
      "esnext"
    ],
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noImplicitReturns": true,
    "skipLibCheck": true,
    "noEmit": true,
    "module": "esnext",
    "moduleResolution": "bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "jsx": "react-jsx",
    "incremental": true,
    "plugins": [
      {
        "name": "next"
      }
    ],
    "paths": {
      "@/*": [
        "./*"
      ]
    },
    "allowJs": true,
    "esModuleInterop": true
  },
  "include": [
    "next-env.d.ts",
    "**/*.ts",
    "**/*.tsx",
    ".next/types/**/*.ts",
    ".next/dev/types/**/*.ts"
  ],
  "exclude": [
    "node_modules"
  ]
}
__MK_EOF__
put vercel.json <<'__MK_EOF__'
{
  "$schema": "https://openapi.vercel.sh/vercel.json",
  "framework": "nextjs"
}
__MK_EOF__

printf '%s\n' "$MK_VERSION" > "$MARKER"
ok "$FILES files written"

export NEXT_TELEMETRY_DISABLED=1
unset NODE_ENV
NEXT="node_modules/.bin/next"

# ── Dependencies (only when package.json changed) ───────────────────────
PKG_SUM="$(cksum < package.json | cut -d' ' -f1)"
if [ ! -x "$NEXT" ] || [ "$(cat node_modules/.mk-pkg 2>/dev/null || true)" != "$PKG_SUM" ]; then
  step "Installing dependencies (about 30 seconds the first time)"
  npm install --no-audit --no-fund --loglevel=error
  printf '%s\n' "$PKG_SUM" > node_modules/.mk-pkg
  ok "Dependencies installed"
else
  ok "Dependencies up to date"
fi

# ── Deploy to Vercel ─────────────────────────────────────────────────────
if [ "$DEPLOY" = 1 ]; then
  read -r -a VC <<< "${VERCEL_BIN:-npx --yes --prefer-offline vercel@60.1.3}"
  G=()
  [ -z "${VERCEL_TOKEN:-}" ] || G+=(--token "$VERCEL_TOKEN")
  [ -z "${VERCEL_SCOPE:-}" ] || G+=(--scope "$VERCEL_SCOPE")
  vc() { "${VC[@]}" "$@" ${G[@]+"${G[@]}"}; }
  step "Checking types"
  npm run --silent typecheck || die "Type errors above. Nothing was deployed."
  ok "Types are clean"
  if ! vc whoami >/dev/null 2>&1; then
    [ -t 0 ] && [ -t 1 ] && [ -z "${VERCEL_TOKEN:-}" ] ||
      die "Not signed in to Vercel. Run 'npx vercel login' or export VERCEL_TOKEN, then rerun with --deploy."
    vc login
  fi
  if ! vc project inspect "$VERCEL_PROJECT" >/dev/null 2>&1; then
    step "Creating the Vercel project \"$VERCEL_PROJECT\""
    rm -f .vercel/project.json
    vc project add "$VERCEL_PROJECT" || die "Could not create the Vercel project \"$VERCEL_PROJECT\"."
  fi
  [ -f .vercel/project.json ] || vc link --yes --project "$VERCEL_PROJECT"
  if [ "$PREVIEW" = 1 ]; then
    step "Deploying a preview"
    vc deploy --yes --project "$VERCEL_PROJECT" || die "Deploy failed. Run './$SCRIPT_NAME --build' to see build errors locally."
    ok "Preview is up (URL above). Ship it for real with ./$SCRIPT_NAME --deploy"
  else
    step "Deploying to production"
    vc deploy --prod --yes --project "$VERCEL_PROJECT" || die "Deploy failed. Run './$SCRIPT_NAME --build' to see build errors locally."
    ok "Live in production (URL above)."
    vc domains add "${SITE_URL#*://}" "$VERCEL_PROJECT" >/dev/null 2>&1 || note "Attach the domain once:  npx vercel domains add ${SITE_URL#*://} $VERCEL_PROJECT"
    note "Next time:             edit $SCRIPT_NAME, then ./$SCRIPT_NAME --deploy"
  fi
  exit 0
fi

# ── Production build (only when the sources changed) ─────────────────────
if [ "$MODE" = prod ]; then
  SRC_SUM="$( { for d in app components lib public; do [ ! -d "$d" ] || find "$d" -type f -print0; done
                printf '%s\0' package.json next.config.ts postcss.config.mjs tsconfig.json; } | LC_ALL=C sort -z | xargs -0 cksum | cksum | cut -d' ' -f1)"
  if [ -f .next/BUILD_ID ] && [ "$(cat .next/.mk-build 2>/dev/null || true)" = "$SRC_SUM" ]; then
    ok "Production build is current"
  else
    step "Building for production"
    "$NEXT" build
    printf '%s\n' "$SRC_SUM" > .next/.mk-build
    ok "Build complete"
  fi
fi

printf '\n%s%s  %s is ready%s\n' "$CB" "$CA" "$SITE_NAME" "$C0"
note "Folder:    $APP_DIR"
note "Settings:  edit the top of $SCRIPT_NAME and rerun"
note "Deploy:    ./$SCRIPT_NAME --deploy"

if [ "$START" = 0 ]; then
  printf '\n  Start it later with:  cd %s && npm run %s -- -p %s\n\n' "$APP_DIR" "$([ "$MODE" = prod ] && echo start || echo dev)" "$PORT"
  exit 0
fi

# ── Start ────────────────────────────────────────────────────────────────
busy() { (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null; }
if busy "$PORT"; then
  [ "$PORT_SET" = 0 ] || die "Port $PORT is already in use. Stop the other server or pick another --port."
  for p in $(seq $((PORT + 1)) $((PORT + 20))); do busy "$p" || { warn "Port $PORT is busy, using $p"; PORT="$p"; break; }; done
  ! busy "$PORT" || die "No free port near $PORT. Pass one with --port."
fi
trap - ERR
cleanup
printf '\n  %s➜  http://localhost:%s%s   %s(Ctrl+C to stop)%s\n\n' "$CB" "$PORT" "$C0" "$CD" "$C0"
if [ "$MODE" = dev ]; then exec "$NEXT" dev -p "$PORT"; else exec "$NEXT" start -p "$PORT"; fi