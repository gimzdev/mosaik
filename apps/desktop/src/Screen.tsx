// The picture of the main screen as the chosen layout would leave it: each window in its zone, a
// thin line of desktop apart, a tile of its zone's colour with its app's icon and name and its
// zone's tag in its title bar, as on screen. A
// window the layout has no zone for is not in the picture (Home names it below). When the layout
// changes, the windows glide to their new places.

import type { CSSProperties } from 'react';

import { AppIcon } from './icons';
import type { ArrangePreview, PreviewWindow, Rect, ZoneColor } from './protocol';

/** Where `r` sits in `area`, as percentages for CSS. */
function place(r: Rect, area: Rect) {
  const w = area.right - area.left || 1;
  const h = area.bottom - area.top || 1;
  const pc = (v: number) => `${String(Math.round(v * 10000) / 100)}%`;
  return {
    left: pc((r.left - area.left) / w),
    top: pc((r.top - area.top) / h),
    width: pc((r.right - r.left) / w),
    height: pc((r.bottom - r.top) / h),
  };
}

const inside = (r: Rect, x: number, y: number) => x >= r.left && x < r.right && y >= r.top && y < r.bottom;

/** Whether two places are the same, give or take a few pixels. */
const near = (a: Rect, b: Rect) =>
  Math.abs(a.left - b.left) <= 4 &&
  Math.abs(a.top - b.top) <= 4 &&
  Math.abs(a.right - b.right) <= 4 &&
  Math.abs(a.bottom - b.bottom) <= 4;

/** The zone a window would fill: the one its new place's middle is in. */
export function zoneOf(preview: ArrangePreview, w: PreviewWindow) {
  if (!w.then) return null;
  const x = (w.then.left + w.then.right) / 2;
  const y = (w.then.top + w.then.bottom) / 2;
  return preview.zones.find((z) => inside(z.rect, x, y)) ?? null;
}

/** Every window is already where the layout would put it (or minimised by it). */
export const settled = (preview: ArrangePreview) =>
  preview.windows.length > 0 && preview.windows.every((w) => w.minimized || (w.then !== null && near(w.now, w.then)));

/** The picture in words, for screen readers. */
export function describe(preview: ArrangePreview): string {
  if (preview.windows.length === 0) return 'Nothing is open on your main screen.';
  const parts = preview.windows.map((w) => {
    if (w.minimized) return `${w.label} minimized`;
    const zone = zoneOf(preview, w);
    return zone ? `${w.label} in ${zone.name}` : `${w.label} where it is`;
  });
  return `${parts.join(', ')}.`;
}

const tint = (color: ZoneColor) => `var(--zone-${color})`;

export function Screen({
  preview,
  unread = false,
  className = '',
}: {
  /** null while Mosaïk is asked, or when it could not say (`unread`). */
  preview: ArrangePreview | null;
  /** Mosaïk could not read the screen (safe mode, say): the picture stays empty, and says so. */
  unread?: boolean;
  className?: string;
}) {
  const area = preview?.work_area ?? { left: 0, top: 0, right: 1920, bottom: 1032 };
  const ratio = (area.right - area.left) / Math.max(1, area.bottom - area.top);
  const shown = (preview?.windows ?? []).map((w, i) => ({ w, i })).filter(({ w }) => !w.minimized && w.then);
  const empty = preview !== null && preview.windows.length === 0;

  return (
    <div
      className={`desk relative overflow-hidden rounded-[18px] forced-colors:border forced-colors:border-[CanvasText] ${className}`}
      // As wide as there is room for, and never so tall that the layouts below it are out of
      // sight in a small window.
      style={{ aspectRatio: String(ratio), width: `min(100%, calc(52vh * ${String(ratio)}))` }}
      role="img"
      aria-label={
        preview ? describe(preview) : unread ? 'Your main screen could not be read.' : 'Your main screen, being read.'
      }
      aria-busy={(preview === null && !unread) || undefined}
    >
      <div className="absolute inset-[6px]">
        {shown.map(({ w, i }) => {
          const zone = preview ? zoneOf(preview, w) : null;
          const target = w.then ?? w.now;
          return (
            <div
              key={`${w.label}-${String(i)}`}
              aria-hidden="true"
              className="stage-window @container absolute p-[3px]"
              style={{ ...place(target, area), transitionDelay: `${String(Math.min(i, 6) * 40)}ms` }}
            >
              <div
                className="flex h-full flex-col overflow-hidden rounded-[9px] border shadow-[0_10px_24px_-14px_rgb(0_0_0/0.8)] forced-colors:border-[CanvasText] forced-colors:bg-[Canvas]"
                style={{
                  // Each window a tile lit by its zone's colour, as in a mosaic.
                  borderColor: zone ? `color-mix(in srgb, ${tint(zone.color)} 70%, transparent)` : 'var(--desk-line)',
                  background: zone
                    ? `linear-gradient(180deg, color-mix(in srgb, ${tint(zone.color)} 18%, var(--desk-window)), var(--desk-window) 75%)`
                    : 'var(--desk-window)',
                }}
              >
                <div className="flex h-[19px] min-w-0 shrink-0 items-center gap-1.5 bg-[var(--desk-bar)] px-1.5">
                  <AppIcon icon={w.icon} size={12} />
                  <span className="min-w-0 flex-1 truncate text-[10.5px] leading-none font-semibold text-[var(--desk-text)]">
                    {w.label}
                  </span>
                  {zone ? (
                    <span
                      className="zone-chip hidden shrink-0 truncate rounded-full px-1.5 text-[9.5px] leading-[13px] font-semibold @[150px]:block"
                      style={
                        {
                          '--chip': tint(zone.color),
                          '--on-chip': zone.color === 'saffron' ? 'var(--on-zone-saffron)' : 'var(--on-zone)',
                          maxWidth: '50%',
                        } as CSSProperties
                      }
                    >
                      {zone.name}
                    </span>
                  ) : null}
                </div>
              </div>
            </div>
          );
        })}
      </div>
      {empty ? (
        <p className="absolute inset-0 grid place-items-center px-4 text-center text-small font-semibold text-[var(--desk-label)]">
          Nothing is open on your main screen.
        </p>
      ) : null}
      {preview === null && unread ? (
        <p className="absolute inset-0 grid place-items-center px-4 text-center text-small font-semibold text-[var(--desk-label)]">
          Mosaïk could not read your screen.
        </p>
      ) : null}
    </div>
  );
}
