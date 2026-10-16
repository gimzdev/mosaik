// The picture of the main screen as the chosen layout, or a workspace opened, would leave it: its
// zones as tiles of their colours, a thin line of desktop apart, each with its number at the top
// right, as Mosaïk tags windows on screen; in each zone, the windows it takes by their title bars,
// front first, a tab group as one bar of its tabs, an app a workspace starts drawn dashed. An
// empty zone shows its colour and number too, and says it is empty. (Each window used to be drawn
// where it would be, so two windows of one zone were drawn one over the other; the owner, of a
// workspace's picture: "look at the anim wtf is happening".) When the layout changes, the zones
// glide to their new places.
//
// With `edit` (ADR 0031), the picture changes before anything moves: a window dragged onto
// another trades places with it, or into a zone joins it, behind its windows (ADR 0033); the
// lines between zones are dragged to resize them, or moved with the arrow keys (`LineHandle`),
// every window staying in its zone. Without a mouse, Enter (or a click)
// picks a window, and Enter on another window or on a zone puts it there; Escape lets go. In
// Arrange's picture a window takes its zone's colour with it, as a tile moved in a mosaic, and a
// zone keeps its number, which says where it is. In a workspace's, each of its apps moves on its
// own (a tab leaves its group) and wears the colour of the zone it goes in.
//
// A suggested workspace has no zones until it opens: its picture shows the apps it starts.

import {
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';

import { LineHandle } from './editor/LineHandle';
import { type Draft, lines } from './editor/model';
import { AppIcon } from './icons';
import type { ArrangePreview, PreviewTab, PreviewWindow, RatioRect, Rect, ZoneColor } from './protocol';
import { zoneChip, zoneTint } from './ui';

/** The main screen's work area when Mosaïk has not said yet: a common one. */
const SOME_SCREEN: Rect = { left: 0, top: 0, right: 1920, bottom: 1032 };
/** The monitor of the picture's zones, to the lines between them. */
const MAIN = 'main';
/** A title bar's height on the picture, and the room between two, in pixels. */
const BAR = 24;
const GAP = 3;

/** The picture's width: as wide as there is room for, and never so tall that what is below it is
 * out of sight in a small window (`tallest`, in percent of the window's height). What goes above
 * and below it takes the same width, so they line up with it. */
export function pictureWidth(area: Rect | null, tallest = 44): CSSProperties {
  const a = area ?? SOME_SCREEN;
  const ratio = (a.right - a.left) / Math.max(1, a.bottom - a.top);
  return { width: `min(100%, calc(${String(tallest)}vh * ${String(ratio)}))` };
}

/** The picture's size: its width (`pictureWidth`), in the screen's proportions. */
function deskSize(area: Rect, tallest: number): CSSProperties {
  const ratio = (area.right - area.left) / Math.max(1, area.bottom - area.top);
  return { aspectRatio: String(ratio), ...pictureWidth(area, tallest) };
}

const wide = (r: Rect) => r.right - r.left || 1;
const high = (r: Rect) => r.bottom - r.top || 1;

/** A rectangle of the work area as fractions of it. */
function ratioOf(r: Rect, area: Rect): RatioRect {
  return {
    left: (r.left - area.left) / wide(area),
    top: (r.top - area.top) / high(area),
    right: (r.right - area.left) / wide(area),
    bottom: (r.bottom - area.top) / high(area),
  };
}

/** Fractions of the work area in its pixels, rounded as Mosaïk rounds them. */
function rectOf(a: RatioRect, area: Rect): Rect {
  return {
    left: Math.round(area.left + a.left * wide(area)),
    top: Math.round(area.top + a.top * high(area)),
    right: Math.round(area.left + a.right * wide(area)),
    bottom: Math.round(area.top + a.bottom * high(area)),
  };
}

const pc = (v: number) => `${String(Math.round(v * 10000) / 100)}%`;

const inside = (r: Rect, x: number, y: number) => x >= r.left && x < r.right && y >= r.top && y < r.bottom;

/** Whether two places are the same, give or take a few pixels. */
const near = (a: Rect, b: Rect) =>
  Math.abs(a.left - b.left) <= 4 &&
  Math.abs(a.top - b.top) <= 4 &&
  Math.abs(a.right - b.right) <= 4 &&
  Math.abs(a.bottom - b.bottom) <= 4;

/** The zone a window goes in, by its index in the picture's zones; -1 for none (minimised, left
 * where it is, a place of its own as full screen, or a zone on another screen). Mosaïk says which;
 * a picture that does not say (an older Mosaïk's) is read by where the window's middle lands. */
export function zoneIndex(preview: ArrangePreview, w: PreviewWindow): number {
  if (w.minimized || !w.then) return -1;
  if (w.zone !== undefined) return preview.zones.findIndex((z) => z.id === w.zone);
  if (w.app !== undefined) return -1;
  const x = (w.then.left + w.then.right) / 2;
  const y = (w.then.top + w.then.bottom) / 2;
  return preview.zones.findIndex((z) => inside(z.rect, x, y));
}

/** Every window is already where the layout would put it (or minimised by it). */
export const settled = (preview: ArrangePreview) =>
  preview.windows.length > 0 && preview.windows.every((w) => w.minimized || (w.then !== null && near(w.now, w.then)));

/** "3", "3 and 4", "2, 3 and 4". */
function and(list: string[]): string {
  if (list.length <= 1) return list.join('');
  return `${list.slice(0, -1).join(', ')} and ${list[list.length - 1] ?? ''}`;
}

/** Where a window goes, in words: "Visual Studio Code in zone 1", "Windows Terminal starts in zone
 * 3 with Claude as tabs". */
function said(preview: ArrangePreview, w: PreviewWindow): string {
  if (w.minimized) return `${w.label} minimized`;
  const zone = preview.zones[zoneIndex(preview, w)];
  const where = zone ? `in zone ${zone.name}` : w.then ? 'in a place of its own' : 'where it is';
  const others = (w.tabs ?? []).filter((t) => !t.shown).map((t) => t.label);
  const tabs = others.length ? ` with ${others.join(', ')} as tabs` : '';
  return `${w.label} ${w.starting ? 'starts ' : ''}${where}${tabs}`;
}

/** The picture in words, for screen readers: where each window goes, then the zones left empty. */
export function describe(preview: ArrangePreview): string {
  if (preview.windows.length === 0) return 'Nothing is open on your main screen.';
  const windows = `${preview.windows.map((w) => said(preview, w)).join(', ')}.`;
  const taken = new Set(preview.windows.map((w) => zoneIndex(preview, w)));
  const empty = preview.zones.filter((_, i) => !taken.has(i)).map((z) => z.name);
  if (!empty.length) return windows;
  return `${windows} ${empty.length === 1 ? `Zone ${and(empty)} is` : `Zones ${and(empty)} are`} empty.`;
}

/** A window put in the place of a zone, as Mosaïk will say it is. */
const put = (w: PreviewWindow, z: ArrangePreview['zones'][number]): PreviewWindow => ({
  ...w,
  then: z.rect,
  ...(z.id === undefined ? {} : { zone: z.id }),
});

/** The picture with windows `a` and `b` trading places: their zones, and their places in its
 * list, front to back (ADR 0032). */
function traded(preview: ArrangePreview, a: PreviewWindow, b: PreviewWindow): ArrangePreview {
  const [i, j] = [preview.windows.indexOf(a), preview.windows.indexOf(b)];
  const [za, zb] = [preview.zones[zoneIndex(preview, a)], preview.zones[zoneIndex(preview, b)]];
  if (i < 0 || j < 0 || i === j || !za || !zb) return preview;
  const windows = [...preview.windows];
  windows[i] = put(b, za);
  windows[j] = put(a, zb);
  return { ...preview, windows };
}

/** The windows of the `zone`-th zone, front first: Mosaïk lists each zone's own window first. */
const inZone = (preview: ArrangePreview, zone: number) => preview.windows.filter((x) => zoneIndex(preview, x) === zone);

/**
 * Arrange's picture once `window` is dropped in the `zone`-th zone, before Mosaïk says: `onto` a
 * window there, the two trade places, zones and places front to back (ADR 0032); onto the zone
 * itself, the window joins it, behind its windows (ADR 0033), or takes it when it is empty. When
 * the window and the one it trades with are their zones' own, or the window takes an empty zone,
 * the zones' colours go with them; a window that joins a zone takes its colour. The zones keep
 * their numbers. Mosaïk's own answer follows (it may also make room for a window that needs more,
 * or keep it where it was when the zone cannot have it).
 */
export function moved(
  preview: ArrangePreview,
  window: number,
  zone: number,
  onto: number | null = null,
): ArrangePreview {
  const target = preview.zones[zone];
  const w = preview.windows.find((x) => x.window === window);
  const fromIndex = w ? zoneIndex(preview, w) : -1;
  const from = preview.zones[fromIndex];
  if (!target || !w || !from) return preview;
  const dropped = onto === null ? undefined : inZone(preview, zone).find((x) => x.window === onto && x !== w);
  if (!dropped && fromIndex === zone) return preview;
  // Onto the zone itself, a window joins it at the back: last in the list, which is front to back.
  if (!dropped && inZone(preview, zone).length > 0) {
    return { ...preview, windows: [...preview.windows.filter((x) => x !== w), put(w, target)] };
  }
  const own = (x: PreviewWindow, z: number) => inZone(preview, z)[0] === x;
  const colours = fromIndex !== zone && own(w, fromIndex) && (!dropped || own(dropped, zone));
  const next = dropped
    ? traded(preview, w, dropped)
    : { ...preview, windows: preview.windows.map((x) => (x === w ? put(x, target) : x)) };
  if (!colours) return next;
  return {
    ...next,
    zones: next.zones.map((z, i) =>
      i === zone ? { ...z, color: from.color } : i === fromIndex ? { ...z, color: target.color } : z,
    ),
  };
}

/** A workspace's picture once an app is put elsewhere, before Mosaïk says: dropped onto another
 * app, the two trade places, zones and places front to back (ADR 0031, 0032); onto a zone, it goes
 * in it. A tab leaving its group, or trading places in it, waits for Mosaïk's answer, which draws
 * the group again. */
export function dropped(preview: ArrangePreview, d: Dropped): ArrangePreview {
  if (d.piece.tab || d.onto?.tab) return preview;
  const target = preview.zones[d.zone];
  const w = preview.windows.find((x) => x.app !== undefined && x.app === d.piece.app);
  if (!target || !w) return preview;
  const other = d.onto ? preview.windows.find((x) => x.app !== undefined && x.app === d.onto?.app) : undefined;
  if (other) return traded(preview, w, other);
  if (zoneIndex(preview, w) === d.zone) return preview;
  return { ...preview, windows: preview.windows.map((x) => (x === w ? put(x, target) : x)) };
}

/** The zones' places as fractions of the work area, in the picture's order: where the lines
 * between them start from. */
export function areasOf(preview: ArrangePreview): RatioRect[] {
  return preview.zones.map((z) => z.area ?? ratioOf(z.rect, preview.work_area));
}

/** The picture with its zones resized to `areas` (in its order), and their windows with them,
 * before Mosaïk says. */
export function stretched(preview: ArrangePreview, areas: readonly RatioRect[]): ArrangePreview {
  if (areas.length !== preview.zones.length) return preview;
  const rects = areas.map((a) => rectOf(a, preview.work_area));
  return {
    ...preview,
    zones: preview.zones.map((z, i) => {
      const area = areas[i];
      const rect = rects[i];
      return area && rect ? { ...z, rect, area } : z;
    }),
    windows: preview.windows.map((w) => {
      const r = rects[zoneIndex(preview, w)];
      return r && w.then ? { ...w, then: r } : w;
    }),
  };
}

const reducedMotion = () => typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;

/** A window, or a tab of a group, that can be put elsewhere on the picture. */
export interface Piece {
  /** The window; 0 for an app not open yet, or a tab behind another. */
  window: number;
  /** The workspace's app, in a workspace's picture. */
  app: string | null;
  label: string;
  icon: string | null;
  /** A tab of a group: put elsewhere, it leaves the group. */
  tab: boolean;
}

/** A piece put elsewhere: in the `zone`-th zone of the picture, onto another piece (which takes
 * its place) or onto the zone itself. `from` is the zone it was in: the same zone, onto another
 * piece, the two trade places in it (ADR 0032). */
export interface Dropped {
  piece: Piece;
  zone: number;
  onto: Piece | null;
  from: number;
}

export interface PictureEdits {
  /** In both kinds, a window or app dropped onto another trades places with it (two of one zone
   * trade places in it), and dropped onto a zone joins it. `arrange`: the screen's windows, to
   * move. `workspace`: a workspace's apps, saved in it at once. */
  kind: 'arrange' | 'workspace';
  onDrop: (dropped: Dropped) => void;
  /** A line between zones was moved: the zones' areas now, in the picture's order. */
  onStretch: (areas: RatioRect[]) => void;
}

/** A title bar in a zone: a window, or a tab group as one bar of its tabs. */
interface Bar {
  key: string;
  window: PreviewWindow;
  /** Its tabs, when it is a tab group. */
  tabs: PreviewTab[] | null;
}

/** The bars of each zone, front first. */
function barsByZone(preview: ArrangePreview): Bar[][] {
  const out = preview.zones.map((): Bar[] => []);
  preview.windows.forEach((w, i) => {
    const bars = out[zoneIndex(preview, w)];
    if (!bars) return;
    const tabs = w.tabs && w.tabs.length > 1 ? w.tabs : null;
    const id = w.app !== undefined ? `a:${w.app}` : w.window ? `w:${String(w.window)}` : `i:${String(i)}`;
    bars.push({ key: tabs ? `g:${id}` : id, window: w, tabs });
  });
  return out;
}

const tabKey = (bar: Bar, t: PreviewTab, k: number) => (t.app !== undefined ? `a:${t.app}` : `${bar.key}:${String(k)}`);

/** The piece picked to be put elsewhere: its key on the picture, and the zone it is in. */
interface Picked {
  key: string;
  piece: Piece;
  zone: number;
}

/** A target under the pointer, or picked with the keyboard: a zone, and the piece in it dropped
 * onto (none for the zone itself). */
interface Target {
  zone: number;
  onto: Piece | null;
  key: string | null;
}

/** A piece being dragged: where the pointer holds it and its size (the ghost's), the pointer in
 * the picture's box, and what it is over. */
interface Drag {
  key: string;
  piece: Piece;
  zone: number;
  grab: { x: number; y: number; w: number; h: number };
  x: number;
  y: number;
  over: Target | null;
}

/** Where a piece was last drawn, and in which zone: it flies from there when its zone changes. */
interface Spot {
  left: number;
  top: number;
  width: number;
  zone: number;
}

/** A zone's tile: its colour, lit while something is about to be put in it. */
function tileLook(color: ZoneColor, empty: boolean, lit: boolean): CSSProperties {
  const tint = zoneTint(color);
  return empty
    ? {
        borderStyle: 'dashed',
        borderColor: `color-mix(in srgb, ${tint} 65%, transparent)`,
        background: `color-mix(in srgb, ${tint} ${lit ? '26%' : '10%'}, transparent)`,
      }
    : {
        borderColor: `color-mix(in srgb, ${tint} ${lit ? '100%' : '70%'}, transparent)`,
        background: `linear-gradient(180deg, color-mix(in srgb, ${tint} ${lit ? '32%' : '18%'}, var(--desk-window)), var(--desk-window) 75%)`,
      };
}

const BAR_TEXT = 'min-w-0 truncate text-left text-[12px] leading-none font-semibold text-[var(--desk-text)]';

export function Screen({
  preview,
  unread = false,
  className = '',
  tallest = 44,
  edit,
  onPick,
}: {
  /** null while Mosaïk is asked, or when it could not say (`unread`). */
  preview: ArrangePreview | null;
  /** The most of the window's height it takes, in percent: less where more goes below it. */
  tallest?: number;
  /** Mosaïk could not read the screen (safe mode, say): the picture stays empty, and says so. */
  unread?: boolean;
  className?: string;
  /** What can be changed on it; without it, the picture stays as it is. */
  edit?: PictureEdits | undefined;
  /** The piece picked to be put elsewhere (its name), or none any more. */
  onPick?: (label: string | null) => void;
}) {
  const area = preview?.work_area ?? SOME_SCREEN;
  const zones = preview?.zones ?? [];
  const empty = preview !== null && preview.windows.length === 0;
  const kind = preview ? (edit?.kind ?? null) : null;
  const hint = useId();

  const inner = useRef<HTMLDivElement>(null);
  /** Every bar and tab drawn: its element, the piece it is (when it can be moved) and its zone. */
  const drawn = useRef(new Map<string, { el: HTMLElement; piece: Piece | null; zone: number }>());
  const zoneEls = useRef(new Map<number, HTMLElement>());
  const [picked, setPicked] = useState<Picked | null>(null);
  const [drag, setDragState] = useState<Drag | null>(null);
  const dragging = useRef<Drag | null>(null);
  /** Where the pointer went down on a piece, until it moves far enough to drag it. */
  const press = useRef<{ key: string; pointer: number; x: number; y: number } | null>(null);
  /** A drag just ended: the click that follows is not a pick. */
  const dragged = useRef(false);
  const last = useRef(new Map<string, Spot>());
  /** The piece put elsewhere with the keyboard, from its zone, on the picture shown then: it takes
   * the focus once drawn in another zone, which for a tab is once Mosaïk has answered; traded in
   * its own zone (`same`), once the picture has changed. */
  const focusNext = useRef<{ key: string; from: number; seen: ArrangePreview | null; same: boolean } | null>(null);
  /** Pieces traded in one zone, on the picture shown then: they fly to their new places once the
   * picture changes, though their zone does not. */
  const fly = useRef<{ keys: Set<string>; seen: ArrangePreview | null } | null>(null);
  /** The zones as a line between them is being moved, over the picture they started from. */
  const [stretch, setStretch] = useState<{ from: ArrangePreview; drafts: Draft[] } | null>(null);
  const stretchLatest = useRef<{ from: ArrangePreview; drafts: Draft[] } | null>(null);
  /** The picture's size in pixels, for how many title bars a zone has room for. */
  const [box, setBox] = useState<{ w: number; h: number } | null>(null);

  const bars = useMemo(() => (preview ? barsByZone(preview) : []), [preview]);
  const base = useMemo(
    (): Draft[] =>
      preview
        ? preview.zones.map((z, i) => ({
            key: z.id ?? `z${String(i)}`,
            id: z.id ?? null,
            name: z.name,
            color: z.color,
            monitor: MAIN,
            area: z.area ?? ratioOf(z.rect, preview.work_area),
          }))
        : [],
    [preview],
  );
  const stretching = stretch !== null && stretch.from === preview;
  const drafts = stretching ? stretch.drafts : base;

  // A piece picked that is not in the picture any more (another layout, a window closed) is let go.
  const pieceKeys = new Set<string>();
  if (kind) {
    for (const list of bars) {
      for (const b of list) {
        if (b.tabs && kind === 'workspace') b.tabs.forEach((t, k) => pieceKeys.add(tabKey(b, t, k)));
        else pieceKeys.add(b.key);
      }
    }
  }
  // A piece picked that is not in the picture any more (another layout, a window closed, the
  // picture not to be changed for now) is let go for good, not picked again when it comes back.
  if (picked !== null && !pieceKeys.has(picked.key)) setPicked(null);
  const chosen = picked && pieceKeys.has(picked.key) ? picked : null;
  // The page says where the piece picked goes, and stops when it is let go, however that was.
  const choosing = chosen?.piece.label ?? null;
  useEffect(() => {
    onPick?.(choosing);
  }, [choosing, onPick]);

  const setDrag = (d: Drag | null) => {
    dragging.current = d;
    setDragState(d);
  };

  const pick = (next: Picked | null) => {
    setPicked(next);
  };

  const putIn = (key: string, piece: Piece, from: number, t: Target) => {
    if (t.zone === from && t.key !== null) fly.current = { keys: new Set([key, t.key]), seen: preview };
    edit?.onDrop({ piece, zone: t.zone, onto: t.onto, from });
  };

  /** What is under the pointer: another piece (in the same zone too: the two trade places in it),
   * else a zone (not its own). */
  const targetAt = (x: number, y: number, self: string, from: number): Target | null => {
    const hit = (el: HTMLElement) => {
      const r = el.getBoundingClientRect();
      return x >= r.left && x < r.right && y >= r.top && y < r.bottom;
    };
    for (const [key, d] of drawn.current) {
      if (key !== self && d.piece && hit(d.el)) return { zone: d.zone, onto: d.piece, key };
    }
    for (const [zone, el] of zoneEls.current) {
      if (hit(el)) return zone === from ? null : { zone, onto: null, key: null };
    }
    return null;
  };

  /** Pointer and click handling for a piece: a press that moves 5 pixels drags it (a ghost of it
   * follows the pointer); a click picks it, or puts the piece picked onto it. */
  const handle = (key: string, piece: Piece, zone: number) => ({
    onPointerDown: (e: ReactPointerEvent<HTMLElement>) => {
      dragged.current = false;
      if (e.button !== 0) return;
      press.current = { key, pointer: e.pointerId, x: e.clientX, y: e.clientY };
      // Held from the start: a quick drag off a small tab is still this piece's.
      if (typeof e.currentTarget.setPointerCapture === 'function') e.currentTarget.setPointerCapture(e.pointerId);
    },
    onPointerMove: (e: ReactPointerEvent<HTMLElement>) => {
      const p = press.current;
      const room = inner.current?.getBoundingClientRect();
      if (p?.pointer !== e.pointerId || p.key !== key || !room) return;
      let d = dragging.current;
      if (!d) {
        if (Math.hypot(e.clientX - p.x, e.clientY - p.y) < 5) return;
        pick(null);
        const r = e.currentTarget.getBoundingClientRect();
        d = {
          key,
          piece,
          zone,
          grab: { x: p.x - r.left, y: p.y - r.top, w: r.width, h: r.height },
          x: 0,
          y: 0,
          over: null,
        };
      }
      setDrag({
        ...d,
        x: e.clientX - room.left,
        y: e.clientY - room.top,
        over: targetAt(e.clientX, e.clientY, key, zone),
      });
    },
    onPointerUp: (e: ReactPointerEvent<HTMLElement>) => {
      if (press.current?.pointer !== e.pointerId) return;
      press.current = null;
      const d = dragging.current;
      if (!d) return;
      dragged.current = true;
      setDrag(null);
      const room = inner.current?.getBoundingClientRect();
      if (!d.over || !room) return;
      // It lands from where it was let go of.
      last.current.set(key, {
        left: room.left + d.x - d.grab.x,
        top: room.top + d.y - d.grab.y,
        width: d.grab.w,
        zone: d.zone,
      });
      putIn(key, d.piece, d.zone, d.over);
    },
    onPointerCancel: () => {
      press.current = null;
      setDrag(null);
    },
    onLostPointerCapture: () => {
      if (dragging.current?.key === key && !press.current) setDrag(null);
    },
    onClick: () => {
      if (dragged.current) {
        dragged.current = false;
        return;
      }
      if (!chosen || chosen.key === key) {
        pick(chosen?.key === key ? null : { key, piece, zone });
        return;
      }
      focusNext.current = { key: chosen.key, from: chosen.zone, seen: preview, same: chosen.zone === zone };
      putIn(chosen.key, chosen.piece, chosen.zone, { zone, onto: piece, key });
      pick(null);
    },
  });

  /** Keeps track of an element drawn for `key` while it is in the page. */
  const keep = (key: string, piece: Piece | null, zone: number) => (el: HTMLElement | null) => {
    if (!el) return;
    drawn.current.set(key, { el, piece, zone });
    return () => {
      if (drawn.current.get(key)?.el === el) drawn.current.delete(key);
    };
  };

  // A piece whose zone changed, or traded in its zone, flies from where it was to its new place
  // (and a piece put there with the keyboard takes the focus there).
  useLayoutEffect(() => {
    const before = last.current;
    const now = new Map<string, Spot>();
    const still = reducedMotion();
    const traded = fly.current !== null && fly.current.seen !== preview ? fly.current.keys : null;
    if (traded) fly.current = null;
    for (const [key, d] of drawn.current) {
      const r = d.el.getBoundingClientRect();
      now.set(key, { left: r.left, top: r.top, width: r.width, zone: d.zone });
      const was = before.get(key);
      if (!was || still || typeof d.el.animate !== 'function') continue;
      if (was.zone === d.zone && !traded?.has(key)) continue;
      if (r.width <= 0 || was.width <= 0) continue;
      d.el.animate(
        [
          { transform: `translate(${String(was.left - r.left)}px, ${String(was.top - r.top)}px)` },
          { transform: 'none' },
        ],
        { duration: 300, easing: 'cubic-bezier(0.2, 0.75, 0.25, 1)' },
      );
    }
    last.current = now;
    const next = focusNext.current;
    if (next !== null) {
      const d = drawn.current.get(next.key);
      if (d && (next.same ? preview !== next.seen : d.zone !== next.from)) {
        d.el.focus();
        focusNext.current = null;
      } else if (preview !== next.seen) {
        // Mosaïk answered, and it stayed where it was (refused): the focus stays where it is.
        focusNext.current = null;
      }
    }
  });

  // A drag whose piece left the picture (its window closed, or no room for it now) never hears
  // the pointer let go on it: the release anywhere ends it.
  const dragOn = drag !== null;
  useEffect(() => {
    if (!dragOn) return;
    const end = () => {
      if (!dragging.current) return;
      dragging.current = null;
      press.current = null;
      setDragState(null);
    };
    window.addEventListener('pointerup', end);
    window.addEventListener('pointercancel', end);
    return () => {
      window.removeEventListener('pointerup', end);
      window.removeEventListener('pointercancel', end);
    };
  }, [dragOn]);

  useLayoutEffect(() => {
    const el = inner.current;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const watch = new ResizeObserver((entries) => {
      const r = entries[0]?.contentRect;
      if (!r) return;
      setBox((b) => (b?.w === r.width && b.h === r.height ? b : { w: r.width, h: r.height }));
    });
    watch.observe(el);
    return () => {
      watch.disconnect();
    };
  }, []);

  /** Whether the zones are where the picture has them. */
  const asShown = (next: Draft[]) =>
    next.length === base.length &&
    next.every((d, i) => {
      const was = base[i]?.area;
      return (
        was !== undefined &&
        Math.abs(d.area.left - was.left) <= 1e-9 &&
        Math.abs(d.area.top - was.top) <= 1e-9 &&
        Math.abs(d.area.right - was.right) <= 1e-9 &&
        Math.abs(d.area.bottom - was.bottom) <= 1e-9
      );
    });
  const stretchTo = (next: Draft[]) => {
    if (!preview) return;
    // Put back as they were (a drag let go of with Esc): nothing is being changed any more.
    if (asShown(next)) {
      stretchLatest.current = null;
      setStretch(null);
      return;
    }
    stretchLatest.current = { from: preview, drafts: next };
    setStretch({ from: preview, drafts: next });
  };
  const stretchDone = () => {
    const latest = stretchLatest.current;
    stretchLatest.current = null;
    setStretch(null);
    // Only what was dragged on the picture shown now, and changed.
    if (latest?.from !== preview || !edit || asShown(latest.drafts)) return;
    edit.onStretch(latest.drafts.map((d) => d.area));
  };

  const label = preview
    ? describe(preview)
    : unread
      ? 'Your main screen could not be read.'
      : 'Your main screen, being read.';

  /** A piece's look when it can be moved: picked, dragged (its ghost moves; it stays, faded), or
   * about to be dropped onto. */
  const movable = (key: string) =>
    `touch-none cursor-grab focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[var(--focus)] ${
      chosen?.key === key || drag?.over?.key === key ? 'outline-2 outline-offset-1 outline-[var(--text)]' : ''
    } ${drag?.key === key ? 'opacity-40' : ''}`;

  const bar = (b: Bar, zone: number, first: boolean, chip: string) => {
    const w = b.window;
    // The first bar leaves room on its right for the zone's number.
    const room = first ? (chip.length > 1 ? 'pr-8' : 'pr-7') : '';
    const look = `relative z-[1] flex h-[24px] min-w-0 shrink-0 items-center rounded-[6px] forced-colors:border forced-colors:border-[CanvasText] ${room} ${
      w.starting ? 'border border-dashed border-[var(--desk-label)]' : 'bg-[var(--desk-bar)]'
    }`;
    if (b.tabs && kind === 'workspace') {
      // A workspace's tab group: each tab is one of its apps, and moves on its own.
      return (
        <div key={b.key} className={`${look} gap-px px-[2px]`}>
          {b.tabs.map((t, k) => {
            const key = tabKey(b, t, k);
            const piece: Piece | null =
              t.app === undefined
                ? null
                : { window: t.shown ? w.window : 0, app: t.app, label: t.label, icon: t.icon, tab: true };
            const seg = `@container flex h-[20px] min-w-0 flex-1 items-center gap-1 rounded-[4px] px-1 ${
              t.shown ? 'bg-[color-mix(in_srgb,var(--desk-text)_14%,var(--desk-bar))]' : 'opacity-70'
            }`;
            const content = (
              <>
                <AppIcon icon={t.icon} size={12} />
                <span className={`hidden @[60px]:block ${BAR_TEXT} text-[11px]`}>{t.label}</span>
              </>
            );
            return piece ? (
              <button
                key={key}
                ref={keep(key, piece, zone)}
                type="button"
                aria-label={`${t.label}, a tab in zone ${zones[zone]?.name ?? ''}`}
                aria-pressed={chosen?.key === key}
                aria-describedby={hint}
                className={`${seg} ${movable(key)}`}
                {...handle(key, piece, zone)}
              >
                {content}
              </button>
            ) : (
              <span key={key} ref={keep(key, null, zone)} className={seg}>
                {content}
              </span>
            );
          })}
        </div>
      );
    }
    const piece: Piece | null =
      kind === 'workspace'
        ? w.app === undefined
          ? null
          : { window: w.window, app: w.app, label: w.label, icon: w.icon, tab: false }
        : kind === 'arrange' && w.window
          ? { window: w.window, app: null, label: w.label, icon: w.icon, tab: false }
          : null;
    const content = (
      <>
        <AppIcon icon={w.icon} size={13} />
        <span className={`flex-1 ${BAR_TEXT}`}>{w.label}</span>
        {b.tabs ? (
          <span className="shrink-0 pl-1 text-[11px] leading-none text-[var(--desk-label)]">+{b.tabs.length - 1}</span>
        ) : null}
        {w.starting ? (
          <span className="hidden shrink-0 pl-1 text-[11px] leading-none text-[var(--desk-label)] @[150px]:inline">
            starts
          </span>
        ) : null}
      </>
    );
    return piece && preview ? (
      <button
        key={b.key}
        ref={keep(b.key, piece, zone)}
        type="button"
        aria-label={said(preview, w)}
        aria-pressed={chosen?.key === b.key}
        aria-describedby={hint}
        className={`@container ${look} gap-1.5 pl-1.5 ${first ? '' : 'pr-1.5'} text-left ${movable(b.key)}`}
        {...handle(b.key, piece, zone)}
      >
        {content}
      </button>
    ) : (
      <div
        key={b.key}
        ref={keep(b.key, null, zone)}
        className={`@container ${look} gap-1.5 pl-1.5 ${first ? '' : 'pr-1.5'}`}
      >
        {content}
      </div>
    );
  };

  return (
    <div
      className={`desk relative overflow-hidden rounded-[18px] forced-colors:border forced-colors:border-[CanvasText] ${className}`}
      style={deskSize(area, tallest)}
      role="group"
      aria-label={label}
      aria-busy={(preview === null && !unread) || undefined}
      onKeyDown={(e) => {
        if (e.key === 'Escape' && (chosen !== null || dragging.current)) {
          e.preventDefault();
          e.stopPropagation();
          // A drag let go of this way ends with a click on the piece, which is not a pick.
          if (dragging.current) dragged.current = true;
          press.current = null;
          setDrag(null);
          pick(null);
        }
      }}
    >
      {kind ? (
        <span id={hint} className="sr-only">
          {kind === 'arrange'
            ? 'Drag it into a zone to add it there, or onto another window to swap them. Or press Enter, then Enter on a zone or on the other window.'
            : 'Drag it into a zone to add it there, or onto another app to swap them. Or press Enter, then Enter on a zone or on the other app.'}
        </span>
      ) : null}
      <div ref={inner} className="absolute inset-[6px]">
        {zones.map((z, i) => {
          const a = drafts[i]?.area ?? ratioOf(z.rect, area);
          const list = bars[i] ?? [];
          // As many title bars as there is room for; the rest are counted.
          // Its inside: less its margins, border and padding.
          const height = box ? (a.bottom - a.top) * box.h - 15 : Number.POSITIVE_INFINITY;
          const room = Math.max(0, Math.floor((height + GAP) / (BAR + GAP)));
          const fits = list.length <= room ? list : list.slice(0, room >= 2 ? room - 1 : room);
          const more = list.length - fits.length;
          const lit = drag?.over?.zone === i;
          return (
            <div
              key={i}
              ref={(el) => {
                if (!el) return;
                zoneEls.current.set(i, el);
                return () => {
                  if (zoneEls.current.get(i) === el) zoneEls.current.delete(i);
                };
              }}
              className="stage-window absolute"
              style={{
                left: pc(a.left),
                top: pc(a.top),
                width: pc(a.right - a.left),
                height: pc(a.bottom - a.top),
                ...(stretching
                  ? { transitionProperty: 'none' }
                  : { transitionDelay: `${String(Math.min(i, 6) * 30)}ms` }),
              }}
            >
              <div
                className={`relative m-[3px] flex h-[calc(100%-6px)] flex-col gap-[3px] rounded-[10px] border-[1.5px] p-[3px] forced-colors:border-[CanvasText] forced-colors:bg-[Canvas] ${
                  list.length ? 'shadow-[0_10px_24px_-14px_rgb(0_0_0/0.8)]' : ''
                } ${lit ? 'outline-2 outline-offset-1 outline-[var(--text)]' : ''}`}
                style={tileLook(z.color, list.length === 0, lit)}
              >
                {chosen && chosen.zone !== i ? (
                  <button
                    type="button"
                    aria-label={`Zone ${z.name}${list.length ? '' : ', empty'}`}
                    className="absolute inset-0 z-0 cursor-pointer rounded-[9px] hover:bg-[color-mix(in_srgb,var(--desk-text)_8%,transparent)] focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[var(--focus)]"
                    onClick={() => {
                      focusNext.current = { key: chosen.key, from: chosen.zone, seen: preview, same: false };
                      putIn(chosen.key, chosen.piece, chosen.zone, { zone: i, onto: null, key: null });
                      pick(null);
                    }}
                  />
                ) : null}
                {fits.map((b, k) => bar(b, i, k === 0, z.name))}
                {more > 0 && room >= 2 ? (
                  <span
                    title={list
                      .slice(fits.length)
                      .map((b) => b.window.label)
                      .join(', ')}
                    className="relative z-[1] flex h-[24px] shrink-0 items-center px-1.5 text-[11px] leading-none font-semibold text-[var(--desk-label)]"
                  >
                    +{more} more
                  </span>
                ) : null}
                {list.length === 0 && height >= BAR ? (
                  <span className="pointer-events-none m-auto text-[12px] leading-none font-semibold text-[var(--desk-label)]">
                    Empty
                  </span>
                ) : null}
                {height >= 18 ? (
                  <span
                    aria-hidden="true"
                    className="zone-chip pointer-events-none absolute top-[6px] right-[6px] z-[2] min-w-[18px] rounded-full px-1 text-center text-[11px] leading-[18px] font-bold tabular-nums"
                    style={zoneChip(z.color)}
                  >
                    {z.name}
                  </span>
                ) : null}
              </div>
            </div>
          );
        })}
        {kind && !drag && !chosen && zones.length > 1
          ? lines(drafts, MAIN).map((line) => (
              <LineHandle
                key={line.id}
                line={line}
                drafts={drafts}
                monitor={MAIN}
                area={inner}
                onChange={stretchTo}
                onEnd={stretchDone}
              />
            ))
          : null}
        {drag ? (
          <span
            aria-hidden="true"
            className="pointer-events-none absolute z-30 flex items-center gap-1.5 rounded-[6px] bg-[var(--desk-bar)] px-1.5 shadow-[0_18px_34px_-12px_rgb(0_0_0/0.9)] outline-2 outline-[var(--text)]"
            style={{
              left: drag.x - drag.grab.x,
              top: drag.y - drag.grab.y,
              width: Math.max(drag.grab.w, 120),
              height: drag.grab.h,
            }}
          >
            <AppIcon icon={drag.piece.icon} size={13} />
            <span className={BAR_TEXT}>{drag.piece.label}</span>
          </span>
        ) : null}
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

/** A suggested workspace's apps on the desk: dashed, as apps that will start are drawn, and not in
 * zones yet, since Mosaïk lays them out for their task once they are open. */
export function AppsToStart({
  name,
  apps,
  icons,
  area,
  tallest = 44,
}: {
  name: string;
  apps: string[];
  icons: (string | null)[];
  /** The main screen's work area, for the picture's shape; null while Mosaïk has not said. */
  area: Rect | null;
  tallest?: number;
}) {
  return (
    <div
      className="desk relative grid place-items-center overflow-hidden rounded-[18px] p-6 forced-colors:border forced-colors:border-[CanvasText]"
      style={deskSize(area ?? SOME_SCREEN, tallest)}
    >
      <ul aria-label={`Apps of “${name}”`} className="flex max-w-[34rem] flex-wrap justify-center gap-2">
        {apps.map((app, i) => (
          <li
            key={`${app}-${String(i)}`}
            className="flex min-h-10 items-center gap-2 rounded-[9px] border border-dashed border-[var(--desk-label)] px-3 text-small font-semibold text-[var(--desk-text)]"
          >
            <AppIcon icon={icons[i] ?? null} size={18} />
            {app}
          </li>
        ))}
      </ul>
    </div>
  );
}
