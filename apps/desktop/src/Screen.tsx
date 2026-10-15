// The picture of the main screen as the chosen layout, or a workspace opened, would leave it:
// each window in its zone, a thin line of desktop apart, a tile of its zone's colour with its
// app's icon and name and its zone's number in its title bar, as on screen. A tab group shows its
// tabs above the tab that shows; an app a workspace starts shows dashed, where it will open. A
// window the layout has no zone for is not in the picture (Home names it below). When the layout
// changes, the windows glide to their new places.
//
// The windows can be put in other places before anything moves: a window dragged onto another
// trades places with it, or takes an empty zone (the owner: "we should be able to drag drop apps
// to permute them"). Without a mouse, Enter (or a click) picks a window and Enter on another
// window, or on an empty zone, puts it there; Escape lets go. Each window keeps its colour when
// it moves, as a tile moved in a mosaic; each zone keeps its number, which says where it is.

import {
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';

import { AppIcon } from './icons';
import type { ArrangePreview, PreviewWindow, PreviewZone, Rect, ZoneColor } from './protocol';

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
export function zoneOf(preview: ArrangePreview, w: PreviewWindow): PreviewZone | null {
  if (!w.then) return null;
  const x = (w.then.left + w.then.right) / 2;
  const y = (w.then.top + w.then.bottom) / 2;
  return preview.zones.find((z) => inside(z.rect, x, y)) ?? null;
}

/** Every window is already where the layout would put it (or minimised by it). */
export const settled = (preview: ArrangePreview) =>
  preview.windows.length > 0 && preview.windows.every((w) => w.minimized || (w.then !== null && near(w.now, w.then)));

/** Where a window is, in words: "Visual Studio Code in zone 1", "Windows Terminal starts in
 * zone 3", with a tab group's other tabs. */
function said(preview: ArrangePreview, w: PreviewWindow): string {
  if (w.minimized) return `${w.label} minimized`;
  const zone = zoneOf(preview, w);
  const where = zone ? `in zone ${zone.name}` : 'where it is';
  const others = (w.tabs ?? []).filter((t) => !t.shown).map((t) => t.label);
  const tabs = others.length ? ` with ${others.join(', ')} as tabs` : '';
  return `${w.label} ${w.starting ? 'starts ' : ''}${where}${tabs}`;
}

/** The picture in words, for screen readers. */
export function describe(preview: ArrangePreview): string {
  if (preview.windows.length === 0) return 'Nothing is open on your main screen.';
  return `${preview.windows.map((w) => said(preview, w)).join(', ')}.`;
}

/**
 * The picture once `window` is put in the `zone`-th zone, before Mosaïk says: it trades places
 * with the window there (or leaves its own zone empty), and the two zones' colours go with their
 * windows; the zones keep their numbers. Mosaïk's own answer follows (it may also make room for a
 * window that needs more).
 */
export function moved(preview: ArrangePreview, window: number, zone: number): ArrangePreview {
  const target = preview.zones[zone];
  const w = preview.windows.find((x) => x.window === window);
  const from = w ? zoneOf(preview, w) : null;
  if (!target || !w || !from || from === target) return preview;
  const there = preview.windows.find((x) => x !== w && zoneOf(preview, x) === target);
  return {
    ...preview,
    windows: preview.windows.map((x) =>
      x === w ? { ...x, then: target.rect } : x === there ? { ...x, then: from.rect } : x,
    ),
    zones: preview.zones.map((z) =>
      z === target ? { ...z, color: from.color } : z === from ? { ...z, color: target.color } : z,
    ),
  };
}

const tint = (color: ZoneColor) => `var(--zone-${color})`;

const reducedMotion = () => typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;

/** A window being dragged: how far the pointer went, and the zone it is over (not its own). */
interface Drag {
  window: number;
  dx: number;
  dy: number;
  over: number | null;
}

export function Screen({
  preview,
  unread = false,
  className = '',
  tallest = 52,
  onMove,
  onPick,
}: {
  /** null while Mosaïk is asked, or when it could not say (`unread`). */
  preview: ArrangePreview | null;
  /** The most of the window's height it takes, in percent: less where more goes below it. */
  tallest?: number;
  /** Mosaïk could not read the screen (safe mode, say): the picture stays empty, and says so. */
  unread?: boolean;
  className?: string;
  /** A window was put in another zone (its index in `preview.zones`). Without it, the picture
   * stays as it is. */
  onMove?: ((window: number, zone: number) => void) | undefined;
  /** The window picked to be put elsewhere (its app's name), or none any more. */
  onPick?: (label: string | null) => void;
}) {
  const area = preview?.work_area ?? { left: 0, top: 0, right: 1920, bottom: 1032 };
  const ratio = (area.right - area.left) / Math.max(1, area.bottom - area.top);
  const shown = (preview?.windows ?? []).map((w, i) => ({ w, i })).filter(({ w }) => !w.minimized && w.then);
  const empty = preview !== null && preview.windows.length === 0;
  const movable = onMove !== undefined && preview !== null;
  const hint = useId();

  const inner = useRef<HTMLDivElement>(null);
  const tiles = useRef(new Map<number, HTMLElement>());
  const [chosen, setChosen] = useState<number | null>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  /** Where the pointer went down on a window, until it moves far enough to drag it. */
  const press = useRef<{ window: number; pointer: number; x: number; y: number } | null>(null);
  /** A drag just ended: the click that follows is not a pick. */
  const dragged = useRef(false);
  /** The window let go of, where it was on screen then: it lands from there. */
  const [landing, setLanding] = useState<{ window: number; from: DOMRect } | null>(null);

  // The zone each window is in, by its index in the picture's zones.
  const zoneIndex = (w: PreviewWindow) => {
    const z = preview ? zoneOf(preview, w) : null;
    return z && preview ? preview.zones.indexOf(z) : -1;
  };
  const occupied = new Set(shown.map(({ w }) => zoneIndex(w)));
  const freeZones = (preview?.zones ?? []).map((z, i) => ({ z, i })).filter(({ i }) => !occupied.has(i));
  // A window picked that is not in the picture any more (another layout, a window closed) is let go.
  const picked = chosen !== null && shown.some(({ w }) => w.window === chosen) ? chosen : null;

  const pick = (window: number | null) => {
    setChosen(window);
    onPick?.(window === null ? null : (preview?.windows.find((w) => w.window === window)?.label ?? null));
  };

  const put = (window: number, zone: number) => {
    pick(null);
    if (zone >= 0) onMove?.(window, zone);
  };

  /** The zone under the pointer, by its index in the picture's zones. */
  const zoneAt = (clientX: number, clientY: number) => {
    const box = inner.current?.getBoundingClientRect();
    if (!preview || !box || box.width <= 0 || box.height <= 0) return -1;
    const x = area.left + ((clientX - box.left) / box.width) * (area.right - area.left);
    const y = area.top + ((clientY - box.top) / box.height) * (area.bottom - area.top);
    return preview.zones.findIndex((z) => inside(z.rect, x, y));
  };

  const down = (e: ReactPointerEvent, w: PreviewWindow) => {
    if (!movable || e.button !== 0) return;
    press.current = { window: w.window, pointer: e.pointerId, x: e.clientX, y: e.clientY };
  };
  const moveTo = (e: ReactPointerEvent, w: PreviewWindow) => {
    const p = press.current;
    if (p?.pointer !== e.pointerId || p.window !== w.window) return;
    const dx = e.clientX - p.x;
    const dy = e.clientY - p.y;
    if (!drag && Math.hypot(dx, dy) < 5) return;
    if (!drag) {
      e.currentTarget.setPointerCapture(e.pointerId);
      pick(null);
    }
    const at = zoneAt(e.clientX, e.clientY);
    setDrag({ window: w.window, dx, dy, over: at >= 0 && at !== zoneIndex(w) ? at : null });
  };
  const up = (e: ReactPointerEvent) => {
    if (press.current?.pointer !== e.pointerId) return;
    press.current = null;
    if (!drag) return;
    dragged.current = true;
    const from = tiles.current.get(drag.window)?.getBoundingClientRect();
    if (from) setLanding({ window: drag.window, from });
    setDrag(null);
    if (drag.over !== null) put(drag.window, drag.over);
  };
  const cancel = () => {
    press.current = null;
    setDrag(null);
  };

  const choose = (w: PreviewWindow) => {
    if (dragged.current) {
      dragged.current = false;
      return;
    }
    if (picked === null) pick(w.window);
    else if (picked === w.window) pick(null);
    else put(picked, zoneIndex(w));
  };

  // A window let go of lands from where it was dropped, rather than jumping back first: drawn in
  // its new place at once (no glide), then moved there from the drop.
  useLayoutEffect(() => {
    if (!landing) return;
    const done = () => {
      setLanding((now) => (now === landing ? null : now));
    };
    const el = tiles.current.get(landing.window);
    const to = el?.getBoundingClientRect();
    if (!el || !to || to.width <= 0 || to.height <= 0 || reducedMotion() || typeof el.animate !== 'function') {
      requestAnimationFrame(done);
      return;
    }
    const dx = landing.from.left - to.left;
    const dy = landing.from.top - to.top;
    const sx = landing.from.width / to.width;
    const sy = landing.from.height / to.height;
    el.animate(
      [
        {
          transform: `translate(${String(dx)}px, ${String(dy)}px) scale(${String(sx)}, ${String(sy)})`,
          transformOrigin: '0 0',
        },
        { transform: 'none', transformOrigin: '0 0' },
      ],
      { duration: 260, easing: 'cubic-bezier(0.2, 0.75, 0.25, 1)' },
    ).finished.then(done, done);
  }, [landing]);

  const label = preview
    ? describe(preview)
    : unread
      ? 'Your main screen could not be read.'
      : 'Your main screen, being read.';

  return (
    <div
      className={`desk relative overflow-hidden rounded-[18px] forced-colors:border forced-colors:border-[CanvasText] ${className}`}
      // As wide as there is room for, and never so tall that the layouts below it are out of
      // sight in a small window.
      style={{ aspectRatio: String(ratio), width: `min(100%, calc(${String(tallest)}vh * ${String(ratio)}))` }}
      role="group"
      aria-label={label}
      aria-busy={(preview === null && !unread) || undefined}
      onKeyDown={(e) => {
        if (e.key === 'Escape' && (picked !== null || drag)) {
          e.stopPropagation();
          cancel();
          pick(null);
        }
      }}
    >
      {movable ? (
        <span id={hint} className="sr-only">
          Drag it onto another window to swap them, or press Enter, then Enter on the other window.
        </span>
      ) : null}
      <div ref={inner} className="absolute inset-[6px]">
        {freeZones.map(({ z, i }) => {
          const over = drag?.over === i;
          const style: CSSProperties = {
            ...place(z.rect, area),
            borderColor: over ? 'var(--text)' : 'var(--desk-line)',
          };
          const box = 'absolute m-[3px] rounded-[9px] border border-dashed';
          return picked !== null ? (
            <button
              key={`free-${String(i)}`}
              type="button"
              className={`${box} cursor-pointer hover:border-[var(--text)] focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[var(--focus)]`}
              style={{
                ...style,
                width: `calc(${style.width ?? '0'} - 6px)`,
                height: `calc(${style.height ?? '0'} - 6px)`,
              }}
              aria-label="Empty zone"
              onClick={() => {
                put(picked, i);
              }}
            />
          ) : (
            <div
              key={`free-${String(i)}`}
              aria-hidden="true"
              className={box}
              style={{
                ...style,
                width: `calc(${style.width ?? '0'} - 6px)`,
                height: `calc(${style.height ?? '0'} - 6px)`,
              }}
            />
          );
        })}
        {shown.map(({ w, i }) => {
          const zone = preview ? zoneOf(preview, w) : null;
          const target = w.then ?? w.now;
          const dragging = drag?.window === w.window;
          const over = drag !== null && drag.over !== null && drag.over === zoneIndex(w);
          const lit = picked === w.window || dragging;
          const look: CSSProperties = {
            ...place(target, area),
            transitionDelay: `${String(Math.min(i, 6) * 40)}ms`,
          };
          if (dragging) {
            Object.assign(look, {
              transform: `translate(${String(drag.dx)}px, ${String(drag.dy)}px)`,
              transitionProperty: 'none',
              zIndex: 20,
            });
          } else if (landing?.window === w.window) {
            // Drawn in its new place at once, over the window it traded with, as it lands.
            look.transitionProperty = 'none';
            look.zIndex = 20;
          }
          // A tab group: its tabs above the one that shows, as its strip is on screen.
          const tabs = w.tabs && w.tabs.length > 1 ? w.tabs : null;
          const tile = (
            <span
              className={`flex h-full flex-col overflow-hidden rounded-[9px] border forced-colors:border-[CanvasText] forced-colors:bg-[Canvas] ${
                dragging ? 'shadow-[0_22px_40px_-12px_rgb(0_0_0/0.85)]' : 'shadow-[0_10px_24px_-14px_rgb(0_0_0/0.8)]'
              } ${lit || over ? 'outline-2 outline-offset-1 outline-[var(--text)]' : ''} ${
                // An app a workspace starts: where it will open, drawn in outline.
                w.starting ? 'border-dashed' : ''
              }`}
              style={{
                // Each window a tile lit by its zone's colour, as in a mosaic.
                borderColor: zone ? `color-mix(in srgb, ${tint(zone.color)} 70%, transparent)` : 'var(--desk-line)',
                background: w.starting
                  ? 'transparent'
                  : zone
                    ? `linear-gradient(180deg, color-mix(in srgb, ${tint(zone.color)} 18%, var(--desk-window)), var(--desk-window) 75%)`
                    : 'var(--desk-window)',
              }}
            >
              {tabs ? (
                <span className="flex h-[15px] min-w-0 shrink-0 items-end gap-px px-1 pt-[2px]">
                  {tabs.map((t, k) => (
                    <span
                      key={`${t.label}-${String(k)}`}
                      className={`flex h-full min-w-0 items-center gap-1 rounded-t-[4px] px-1 ${
                        t.shown ? 'bg-[var(--desk-bar)]' : 'opacity-60'
                      }`}
                    >
                      <AppIcon icon={t.icon} size={9} />
                      <span className="hidden min-w-0 truncate text-[8.5px] leading-none font-semibold text-[var(--desk-text)] @[160px]:block">
                        {t.label}
                      </span>
                    </span>
                  ))}
                </span>
              ) : null}
              <span
                className={`flex h-[19px] min-w-0 shrink-0 items-center gap-1.5 px-1.5 ${
                  w.starting ? '' : 'bg-[var(--desk-bar)]'
                }`}
              >
                <AppIcon icon={w.icon} size={12} />
                <span className="min-w-0 flex-1 truncate text-left text-[10.5px] leading-none font-semibold text-[var(--desk-text)]">
                  {w.label}
                </span>
                {zone ? (
                  <span
                    className="zone-chip hidden min-w-[15px] shrink-0 rounded-full px-1 text-center text-[9.5px] leading-[13px] font-bold tabular-nums @[110px]:block"
                    style={
                      {
                        '--chip': tint(zone.color),
                        '--on-chip': zone.color === 'saffron' ? 'var(--on-zone-saffron)' : 'var(--on-zone)',
                      } as CSSProperties
                    }
                  >
                    {zone.name}
                  </span>
                ) : null}
              </span>
              {w.starting ? (
                <span className="m-auto px-1 text-center text-[10px] leading-tight text-[var(--desk-label)]">
                  Opens here
                </span>
              ) : null}
            </span>
          );
          return movable ? (
            <button
              key={w.window || `${w.label}-${String(i)}`}
              ref={(el) => {
                if (el) tiles.current.set(w.window, el);
                else tiles.current.delete(w.window);
              }}
              type="button"
              aria-label={said(preview, w)}
              aria-pressed={picked === w.window}
              aria-describedby={hint}
              className={`stage-window @container absolute touch-none p-[3px] text-left focus-visible:outline-none [&:focus-visible>span]:outline-2 [&:focus-visible>span]:outline-offset-1 [&:focus-visible>span]:outline-[var(--focus)] ${
                dragging ? 'cursor-grabbing' : 'cursor-grab'
              }`}
              style={look}
              onPointerDown={(e) => {
                down(e, w);
              }}
              onPointerMove={(e) => {
                moveTo(e, w);
              }}
              onPointerUp={up}
              onPointerCancel={cancel}
              onLostPointerCapture={() => {
                if (drag?.window === w.window && !press.current) setDrag(null);
              }}
              onClick={() => {
                choose(w);
              }}
            >
              {tile}
            </button>
          ) : (
            <div
              key={w.window || `${w.label}-${String(i)}`}
              aria-hidden="true"
              className="stage-window @container absolute p-[3px]"
              style={look}
            >
              {tile}
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
