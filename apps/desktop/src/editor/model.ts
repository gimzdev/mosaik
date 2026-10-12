// The zone editor's changes, as plain functions on a draft: split, remove, resize (zones that
// share the edge move with it), presets. The engine checks the result again when it is saved
// (crates/mosaik-core/src/editing.rs); the limits here match it.

import type { RatioRect, ZoneColor, ZoneEdit } from '../protocol';

/** The smallest a zone may be, as a fraction of its monitor's work area on each side. */
export const MIN_ZONE = 0.05;
/** The most zones on one monitor. */
export const MAX_ZONES = 16;
/** Dragged edges land on a grid of 48ths of the work area. */
const STEPS = 48;
export const GRID = 1 / STEPS;
/** The longest zone name. */
export const MAX_NAME = 60;

export const COLORS: readonly ZoneColor[] = ['lapis', 'verdigris', 'saffron', 'madder', 'slate'];

export type Edge = 'left' | 'right' | 'top' | 'bottom';

/** A zone being edited: `key` names it in the editor, `id` in the workspace (null when new). */
export interface Draft extends ZoneEdit {
  key: string;
}

const EPSILON = 1e-6;

export function fromZones(zones: ZoneEdit[]): Draft[] {
  return zones.map((z, i) => ({ ...z, key: z.id ?? `new-${String(i)}` }));
}

export function toZones(drafts: Draft[]): ZoneEdit[] {
  return drafts.map(({ id, name, color, monitor, area }) => ({ id, name, color, monitor, area }));
}

let counter = 0;
function newKey(): string {
  counter += 1;
  return `new-${String(Date.now())}-${String(counter)}`;
}

/** Rounds to the grid used for dragging, so neighbours line up exactly. */
export function snap(v: number): number {
  return Math.round(v * STEPS) / STEPS;
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

const near = (a: number, b: number) => Math.abs(a - b) < EPSILON;

/** A name not used by any zone yet: "Zone 1", "Zone 2"… */
export function freeName(drafts: Draft[]): string {
  const used = new Set(drafts.map((d) => d.name.trim().toLowerCase()));
  for (let n = 1; ; n += 1) {
    const name = `Zone ${String(n)}`;
    if (!used.has(name.toLowerCase())) return name;
  }
}

/** The colour least used on the monitor, so neighbours differ. */
function freeColor(drafts: Draft[], monitor: string): ZoneColor {
  const counts = new Map<ZoneColor, number>(COLORS.map((c) => [c, 0]));
  for (const d of drafts) if (d.monitor === monitor) counts.set(d.color, (counts.get(d.color) ?? 0) + 1);
  let best: ZoneColor = 'lapis';
  let fewest = Number.POSITIVE_INFINITY;
  for (const c of COLORS) {
    const n = counts.get(c) ?? 0;
    if (n < fewest) {
      best = c;
      fewest = n;
    }
  }
  return best;
}

/** Splits a zone in two, side by side (`columns`) or one above the other (`rows`). The new half
 * gets a name and colour of its own. Unchanged when the halves would be too small or the monitor
 * is full. */
export function split(drafts: Draft[], key: string, direction: 'columns' | 'rows'): Draft[] {
  const z = drafts.find((d) => d.key === key);
  if (!z) return drafts;
  if (drafts.filter((d) => d.monitor === z.monitor).length >= MAX_ZONES) return drafts;
  const a = z.area;
  const size = direction === 'columns' ? a.right - a.left : a.bottom - a.top;
  if (size / 2 < MIN_ZONE - EPSILON) return drafts;
  const middle = direction === 'columns' ? snap((a.left + a.right) / 2) : snap((a.top + a.bottom) / 2);
  const first: RatioRect = direction === 'columns' ? { ...a, right: middle } : { ...a, bottom: middle };
  const second: RatioRect = direction === 'columns' ? { ...a, left: middle } : { ...a, top: middle };
  const added: Draft = {
    key: newKey(),
    id: null,
    name: freeName(drafts),
    color: freeColor(drafts, z.monitor),
    monitor: z.monitor,
    area: second,
  };
  return drafts.flatMap((d) => (d.key === key ? [{ ...d, area: first }, added] : [d]));
}

/** Removes a zone. Its windows keep their place on screen, as places of their own. */
export function remove(drafts: Draft[], key: string): Draft[] {
  return drafts.filter((d) => d.key !== key);
}

export function update(drafts: Draft[], key: string, change: Partial<Pick<Draft, 'name' | 'color'>>): Draft[] {
  return drafts.map((d) => (d.key === key ? { ...d, ...change } : d));
}

/** Moves one edge of a zone to `to` (a fraction of the work area). Zones on the same monitor
 * whose opposite edge lies on the moved edge move with it, so tiled zones stay tiled. The move
 * stops where any zone it changes would get smaller than the minimum. */
export function resize(drafts: Draft[], key: string, edge: Edge, to: number): Draft[] {
  const z = drafts.find((d) => d.key === key);
  if (!z) return drafts;
  const a = z.area;
  const from = a[edge];
  const opposite: Record<Edge, Edge> = { left: 'right', right: 'left', top: 'bottom', bottom: 'top' };
  const horizontal = edge === 'left' || edge === 'right';
  // Zones whose matching edge is on the same line and that touch this zone along it.
  const overlaps = (b: RatioRect) =>
    horizontal ? b.top < a.bottom - EPSILON && b.bottom > a.top + EPSILON : b.left < a.right - EPSILON && b.right > a.left + EPSILON;
  const neighbours = drafts.filter(
    (d) => d.key !== key && d.monitor === z.monitor && near(d.area[opposite[edge]], from) && overlaps(d.area),
  );
  // How far the edge may go: every zone keeps the minimum size, and stays on the monitor.
  let lo = 0;
  let hi = 1;
  const keepMin = (r: RatioRect, moving: Edge) => {
    if (moving === 'left') hi = Math.min(hi, r.right - MIN_ZONE);
    if (moving === 'right') lo = Math.max(lo, r.left + MIN_ZONE);
    if (moving === 'top') hi = Math.min(hi, r.bottom - MIN_ZONE);
    if (moving === 'bottom') lo = Math.max(lo, r.top + MIN_ZONE);
  };
  keepMin(a, edge);
  for (const n of neighbours) keepMin(n.area, opposite[edge]);
  if (lo > hi + EPSILON) return drafts;
  const target = clamp(to, lo, hi);
  if (near(target, from)) return drafts;
  const moved = new Set(neighbours.map((n) => n.key));
  return drafts.map((d) => {
    if (d.key === key) return { ...d, area: { ...d.area, [edge]: target } };
    if (moved.has(d.key)) return { ...d, area: { ...d.area, [opposite[edge]]: target } };
    return d;
  });
}

export type Preset = 'full' | 'halves' | 'thirds' | 'main-side' | 'side-main' | 'quarters';

export const PRESETS: readonly { id: Preset; label: string }[] = [
  { id: 'full', label: 'Whole screen' },
  { id: 'halves', label: 'Halves' },
  { id: 'thirds', label: 'Thirds' },
  { id: 'main-side', label: 'Main and side' },
  { id: 'side-main', label: 'Side and main' },
  { id: 'quarters', label: 'Quarters' },
];

const SHAPES: Record<Preset, RatioRect[]> = {
  full: [{ left: 0, top: 0, right: 1, bottom: 1 }],
  halves: [
    { left: 0, top: 0, right: 0.5, bottom: 1 },
    { left: 0.5, top: 0, right: 1, bottom: 1 },
  ],
  thirds: [
    { left: 0, top: 0, right: 1 / 3, bottom: 1 },
    { left: 1 / 3, top: 0, right: 2 / 3, bottom: 1 },
    { left: 2 / 3, top: 0, right: 1, bottom: 1 },
  ],
  'main-side': [
    { left: 0, top: 0, right: 2 / 3, bottom: 1 },
    { left: 2 / 3, top: 0, right: 1, bottom: 1 },
  ],
  'side-main': [
    { left: 0, top: 0, right: 1 / 3, bottom: 1 },
    { left: 1 / 3, top: 0, right: 1, bottom: 1 },
  ],
  quarters: [
    { left: 0, top: 0, right: 0.5, bottom: 0.5 },
    { left: 0.5, top: 0, right: 1, bottom: 0.5 },
    { left: 0, top: 0.5, right: 0.5, bottom: 1 },
    { left: 0.5, top: 0.5, right: 1, bottom: 1 },
  ],
};

/** Replaces one monitor's zones with a preset. Existing zones are reused in order (they keep
 * their ids, names and colours, so their windows stay in them); extra ones are removed. */
export function applyPreset(drafts: Draft[], monitor: string, preset: Preset): Draft[] {
  const here = drafts
    .filter((d) => d.monitor === monitor)
    .sort((x, y) => x.area.left - y.area.left || x.area.top - y.area.top);
  const others = drafts.filter((d) => d.monitor !== monitor);
  const made: Draft[] = [];
  SHAPES[preset].forEach((area, i) => {
    const existing = here[i];
    if (existing) {
      made.push({ ...existing, area });
    } else {
      const all = [...others, ...made];
      made.push({ key: newKey(), id: null, name: freeName(all), color: freeColor(all, monitor), monitor, area });
    }
  });
  return [...others, ...made];
}

/** Why the draft cannot be saved, or null. */
export function problem(drafts: Draft[]): string | null {
  for (const d of drafts) {
    const name = d.name.trim();
    if (!name) return 'Every zone needs a name.';
    if (Array.from(name).length > MAX_NAME) return `Zone names are ${String(MAX_NAME)} characters at most.`;
    const a = d.area;
    if (a.right - a.left < MIN_ZONE - EPSILON || a.bottom - a.top < MIN_ZONE - EPSILON) {
      return `Zone “${name}” is too small.`;
    }
  }
  const perMonitor = new Map<string, number>();
  for (const d of drafts) perMonitor.set(d.monitor, (perMonitor.get(d.monitor) ?? 0) + 1);
  for (const n of perMonitor.values()) if (n > MAX_ZONES) return `A monitor has more than ${String(MAX_ZONES)} zones.`;
  return null;
}

/** A percentage for people: 0.333… → "33%". */
export function percent(v: number): string {
  return `${String(Math.round(v * 1000) / 10)}%`;
}
