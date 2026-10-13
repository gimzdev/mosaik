// A small picture of the main screen: its windows where they are now and, while the user points
// at Arrange or a layout, where they would go. The windows glide from one to the other; the zones
// they would fill appear, and each window wears its zone's colour and name, as on screen.

import type { CSSProperties, ReactNode } from 'react';

import type { ArrangePreview, PreviewWindow, Rect, ZoneColor } from './protocol';

export type StageMode = 'now' | 'then';

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
function zoneOf(preview: ArrangePreview, w: PreviewWindow) {
  if (!w.then) return null;
  const x = (w.then.left + w.then.right) / 2;
  const y = (w.then.top + w.then.bottom) / 2;
  return preview.zones.find((z) => inside(z.rect, x, y)) ?? null;
}

/** The picture in words, for screen readers. */
export function describe(preview: ArrangePreview, mode: StageMode): string {
  const n = preview.windows.length;
  if (n === 0) return 'No windows are open on your main screen.';
  if (mode === 'now') {
    return `Your main screen now: ${preview.windows.map((w) => w.label).join(', ')}.`;
  }
  const parts = preview.windows.map((w) => {
    const zone = zoneOf(preview, w);
    if (!zone) return `${w.label} stays where it is`;
    return w.tab ? `${w.label} as a tab in ${zone.name}` : `${w.label} in ${zone.name}`;
  });
  return `${parts.join(', ')}.`;
}

const ring = (color: ZoneColor) => `var(--zone-${color})`;

export function Stage({
  preview,
  mode,
  caption,
  maxHeight = 260,
}: {
  /** null while Mosaïk is asked. */
  preview: ArrangePreview | null;
  mode: StageMode;
  /** Says what the picture shows: the screen now, or with a layout. */
  caption: ReactNode;
  /** The tallest the picture gets, in pixels; it is smaller in a small window. */
  maxHeight?: number;
}) {
  const area = preview?.work_area ?? { left: 0, top: 0, right: 1920, bottom: 1032 };
  const ratio = (area.right - area.left) / Math.max(1, area.bottom - area.top);
  const then = mode === 'then';
  const windows = preview?.windows ?? [];
  // Windows that would become tabs, counted on the window that keeps showing in their zone.
  const tabsOn = new Map<number, number>();
  if (preview && then) {
    windows.forEach((w, i) => {
      const t = w.then;
      if (!w.tab || !t) return;
      const owner = windows.findIndex(
        (o, j) => j !== i && !o.tab && o.then !== null && o.then.left === t.left && o.then.top === t.top,
      );
      if (owner >= 0) tabsOn.set(owner, (tabsOn.get(owner) ?? 0) + 1);
    });
  }
  const empty = preview !== null && windows.length === 0;

  return (
    <figure className="m-0">
      <div
        className="relative mx-auto w-full overflow-hidden rounded-xl border border-[var(--line)] bg-[var(--desk)] forced-colors:border-[CanvasText]"
        style={{
          aspectRatio: String(ratio),
          // As tall as `maxHeight`, or 36% of a small window's height.
          maxWidth: `min(${String(Math.round(maxHeight * ratio))}px, ${String(Math.round(36 * ratio))}vh)`,
        }}
        role="img"
        aria-label={preview ? describe(preview, mode) : 'Your main screen, being read.'}
        aria-busy={preview === null || undefined}
        data-mode={mode}
      >
        <div className="absolute inset-[5px]">
          {preview?.zones.map((z) => (
            <div
              key={`${z.name}-${String(z.rect.left)}-${String(z.rect.top)}`}
              aria-hidden="true"
              className="stage-zone absolute rounded-[6px] border-2 border-dashed"
              style={{
                ...place(z.rect, area),
                borderColor: ring(z.color),
                background: `color-mix(in srgb, ${ring(z.color)} 12%, transparent)`,
                opacity: then || empty ? 1 : 0,
              }}
            >
              {empty || !windows.some((w) => zoneOf(preview, w) === z) ? (
                <span className="absolute inset-0 grid place-items-center text-caption font-semibold text-[var(--desk-label)]">
                  {z.name}
                </span>
              ) : null}
            </div>
          ))}
          {windows
            .map((w, i) => ({ w, i }))
            .reverse()
            .map(({ w, i }) => {
              // A window already where Arrange would put it wears its zone's colour and name, as
              // on screen, before anything moves.
              const settled = w.then !== null && near(w.now, w.then);
              const zone = (then || settled) && preview ? zoneOf(preview, w) : null;
              const target = then && w.then ? w.then : w.now;
              const tabs = tabsOn.get(i) ?? 0;
              return (
                <div
                  key={`${w.label}-${String(i)}`}
                  aria-hidden="true"
                  className="stage-window absolute flex flex-col overflow-hidden rounded-[6px] border border-[var(--desk-line)] bg-[var(--desk-window)] shadow-[0_6px_18px_-8px_rgb(0_0_0/0.55)] forced-colors:border-[CanvasText]"
                  style={{
                    ...place(target, area),
                    zIndex: w.tab && then ? 1 : windows.length - i + 1,
                    opacity: w.tab && then ? 0 : 1,
                    boxShadow: zone && !w.tab ? `0 0 0 2px ${ring(zone.color)}` : undefined,
                    transitionDelay: `${String(Math.min(i, 6) * 45)}ms`,
                  }}
                >
                  <div className="flex h-[15px] min-w-0 shrink-0 items-center gap-1 bg-[var(--desk-bar)] px-1.5">
                    <span className="min-w-0 flex-1 truncate text-[9.5px] leading-none font-semibold text-[var(--desk-text)]">
                      {w.label}
                    </span>
                    {zone && !w.tab ? (
                      <span
                        className="zone-chip shrink-0 truncate rounded-full px-1.5 text-[8.5px] leading-[11px] font-semibold"
                        style={
                          {
                            '--chip': ring(zone.color),
                            '--on-chip': zone.color === 'saffron' ? 'var(--on-zone-saffron)' : 'var(--on-zone)',
                            maxWidth: '55%',
                          } as CSSProperties
                        }
                      >
                        {zone.name}
                        {tabs ? ` +${String(tabs)}` : ''}
                      </span>
                    ) : null}
                  </div>
                </div>
              );
            })}
        </div>
        {empty && !(then && preview.zones.length) ? (
          <p className="absolute inset-0 grid place-items-center px-4 text-center text-small font-semibold text-[var(--desk-label)]">
            Nothing is open on your main screen yet.
          </p>
        ) : null}
      </div>
      <figcaption className="mt-2 text-center text-small text-[var(--muted)]">{caption}</figcaption>
    </figure>
  );
}
