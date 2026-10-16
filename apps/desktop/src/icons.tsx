// Pictures: apps' own icons (read from Windows once per app, a few at a time; an app without one
// shows none), and the few line drawings the window uses for its controls.

import { type ReactNode, useEffect, useState } from 'react';

import { appIcons } from './engine';

/** Icons read so far: a `data:` URL, or null for an app without one. */
const cache = new Map<string, string | null>();
/** Callers waiting for each key being read. */
const waiting = new Map<string, ((url: string | null) => void)[]>();
/** Keys asked for since the last read: read together, once the page has asked for all it shows. */
let queued: string[] = [];
let scheduled = false;

function read() {
  scheduled = false;
  const keys = queued;
  queued = [];
  void appIcons(keys).then((found) => {
    for (const key of keys) {
      const url = found[key] ?? null;
      cache.set(key, url);
      for (const done of waiting.get(key) ?? []) done(url);
      waiting.delete(key);
    }
  });
}

function load(key: string): Promise<string | null> {
  return new Promise((resolve) => {
    const list = waiting.get(key);
    if (list) {
      list.push(resolve);
      return;
    }
    waiting.set(key, [resolve]);
    queued.push(key);
    if (!scheduled) {
      scheduled = true;
      setTimeout(read, 0);
    }
  });
}

/** The icon for `key`: a URL, null when there is none, undefined while it is being read. */
function useIcon(key: string | null): string | null | undefined {
  // Only the answer matters, not which one: it is in the cache once it has come.
  const [, setRead] = useState(0);
  useEffect(() => {
    if (key === null || cache.has(key)) return;
    let live = true;
    void load(key).then(() => {
      if (live) setRead((n) => n + 1);
    });
    return () => {
      live = false;
    };
  }, [key]);
  if (key === null) return null;
  return cache.get(key);
}

/** An app's own icon, as Windows has it, or nothing: never a made-up picture in its place.
 * Decorative: the app's name is always said next to it. `keepSpace` keeps an icon's room while
 * there is none, so names in a list stay in line. */
export function AppIcon({
  icon,
  size = 20,
  keepSpace = false,
}: {
  icon: string | null;
  size?: number;
  keepSpace?: boolean;
}) {
  const url = useIcon(icon);
  if (!url) {
    return keepSpace ? <span aria-hidden="true" className="shrink-0" style={{ width: size, height: size }} /> : null;
  }
  return (
    <img
      src={url}
      alt=""
      aria-hidden="true"
      width={size}
      height={size}
      draggable={false}
      className="shrink-0 object-contain"
      style={{ width: size, height: size }}
    />
  );
}

/** A workspace's apps at a glance: their icons, the first few, then how many more; their names
 * when none of them has an icon to show. */
export function AppIcons({
  names,
  icons,
  size = 20,
  most = 5,
}: {
  names: string[];
  icons: (string | null)[];
  size?: number;
  most?: number;
}) {
  if (!icons.some((i) => i !== null)) {
    return <span className="truncate text-caption text-[var(--muted)]">{names.join(', ')}</span>;
  }
  const withIcons = names.map((n, i) => ({ n, icon: icons[i] ?? null })).filter((a) => a.icon !== null);
  const shown = withIcons.slice(0, most);
  const more = names.length - shown.length;
  return (
    <span className="flex items-center gap-1.5" aria-hidden="true">
      {shown.map(({ n, icon }, i) => (
        <AppIcon key={`${n}-${String(i)}`} icon={icon} size={size} />
      ))}
      {more > 0 ? <span className="pl-0.5 text-caption font-semibold text-[var(--muted)]">+{more}</span> : null}
    </span>
  );
}

/** The window's own line drawings, 16 units square, in the text colour. (`satisfies`, so a name
 * that is not here does not type-check.) */
const GLYPHS = {
  more: (
    <>
      <circle cx="3.5" cy="8" r="1.25" fill="currentColor" stroke="none" />
      <circle cx="8" cy="8" r="1.25" fill="currentColor" stroke="none" />
      <circle cx="12.5" cy="8" r="1.25" fill="currentColor" stroke="none" />
    </>
  ),
  plus: <path d="M8 3v10M3 8h10" />,
  back: <path d="M10 3.5L5.5 8 10 12.5" />,
  next: <path d="M6 3.5L10.5 8 6 12.5" />,
  down: <path d="M3.5 6l4.5 4.5L12.5 6" />,
  close: <path d="M4 4l8 8M12 4l-8 8" />,
  undo: <path d="M5.5 4L2.5 7l3 3M2.8 7h6.7a4 4 0 0 1 0 8H7" />,
  play: <path d="M5 3.5v9l7-4.5z" fill="currentColor" />,
  edit: <path d="M3 13l.8-3.2L10.5 3l2.5 2.5-6.8 6.8z" />,
  trash: <path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.7 8.5h5.6l.7-8.5" />,
  spark: <path d="M8 1.5l1.6 4.9 4.9 1.6-4.9 1.6L8 14.5l-1.6-4.9L1.5 8l4.9-1.6z" fill="currentColor" stroke="none" />,
  // Mosaïk's four tiles (its mark), for Move windows: the screen made a mosaic.
  tiles: (
    <g fill="currentColor" stroke="none">
      <rect x="1" y="1" width="8" height="8" rx="1.6" />
      <rect x="10.5" y="1" width="4.5" height="4.5" rx="1.3" />
      <rect x="10.5" y="7" width="4.5" height="8" rx="1.3" />
      <rect x="1" y="10.5" width="8" height="4.5" rx="1.3" />
    </g>
  ),
  bell: <path d="M4 11V7a4 4 0 0 1 8 0v4l1 1.5H3zM6.5 14h3" />,
} satisfies Record<string, ReactNode>;

export function Glyph({
  name,
  size = 16,
  className = '',
}: {
  name: keyof typeof GLYPHS;
  size?: number;
  className?: string;
}) {
  return (
    <svg
      aria-hidden="true"
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`shrink-0 ${className}`}
    >
      {GLYPHS[name]}
    </svg>
  );
}

/** A small turning ring, for a button waiting on Mosaïk. */
export function Spinner({ size = 14 }: { size?: number }) {
  return (
    <svg aria-hidden="true" width={size} height={size} viewBox="0 0 16 16" className="spin shrink-0">
      <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeOpacity="0.25" strokeWidth="2" />
      <path d="M8 2a6 6 0 0 1 6 6" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}
