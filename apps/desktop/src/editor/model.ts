// The zone editor's changes, as plain functions on a draft: the lines between zones and moving
// them, splitting, removing, presets. The engine checks the result again when it is saved
// (crates/mosaik-core/src/editing.rs); the limits here match it.

import { type RatioRect, ZONE_COLORS, type ZoneColor, type ZoneEdit } from '../protocol';

/** The smallest a zone may be, as a fraction of its monitor's work area on each side. */
export const MIN_ZONE = 0.05;
/** The most zones on one monitor. */
export const MAX_ZONES = 16;
/** Lines land on a grid of 48ths of the work area. */
const STEPS = 48;
export const GRID = 1 / STEPS;

/** A zone being edited: `key` names it in the editor, `id` in the workspace (null when new). */
export interface Draft extends ZoneEdit {
  key: string;
}

const EPSILON = 1e-6;
const near = (a: number, b: number) => Math.abs(a - b) < EPSILON;

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

/** Rounds to the grid, so neighbours line up exactly. */
export function snap(v: number): number {
  return Math.round(v * STEPS) / STEPS;
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

/** The zones numbered 1, 2, 3… in reading order, as Mosaïk numbers them when they are saved
 * (`Layout::number_zones`): monitor by monitor (in `monitors`' order), then row by row from the
 * top, then from the left; a zone whose top is within 2% of another's is in its row. A zone's
 * number says where it is, never what it holds. */
export function numbered(drafts: Draft[], monitors: readonly string[]): Draft[] {
  const rank = (d: Draft) => {
    const i = monitors.indexOf(d.monitor);
    return i < 0 ? monitors.length : i;
  };
  const order = drafts
    .map((d, i) => ({ d, i }))
    .sort(
      (x, y) =>
        rank(x.d) - rank(y.d) ||
        (Math.abs(x.d.area.top - y.d.area.top) < 0.02 ? x.d.area.left - y.d.area.left : x.d.area.top - y.d.area.top) ||
        x.i - y.i,
    );
  const numbers = new Map(order.map(({ d }, n) => [d.key, String(n + 1)]));
  return drafts.map((d) => {
    const name = numbers.get(d.key) ?? d.name;
    return name === d.name ? d : { ...d, name };
  });
}

/** The colour least used on the monitor, so neighbours differ. */
function freeColor(drafts: Draft[], monitor: string): ZoneColor {
  const counts = new Map<ZoneColor, number>(ZONE_COLORS.map((c) => [c, 0]));
  for (const d of drafts) if (d.monitor === monitor) counts.set(d.color, (counts.get(d.color) ?? 0) + 1);
  let best: ZoneColor = 'lapis';
  let fewest = Number.POSITIVE_INFINITY;
  for (const c of ZONE_COLORS) {
    const n = counts.get(c) ?? 0;
    if (n < fewest) {
      best = c;
      fewest = n;
    }
  }
  return best;
}

/** A line between zones (or between a zone and empty space) that can be moved. */
export interface Line {
  /** A stable name while it moves: its axis and the zones on each side. */
  id: string;
  /** `x`: a vertical line, moved left and right; `y`: a horizontal one. */
  axis: 'x' | 'y';
  /** Where it is, as a fraction of the work area. */
  at: number;
  /** Its extent along the other axis. */
  from: number;
  to: number;
  /** Keys of the zones before it (left or above) and after it (right or below). */
  before: string[];
  after: string[];
}

type Side = 'before' | 'after';
interface Edge {
  zone: Draft;
  side: Side;
  from: number;
  to: number;
}

const START: Record<Line['axis'], 'left' | 'top'> = { x: 'left', y: 'top' };
const END: Record<Line['axis'], 'right' | 'bottom'> = { x: 'right', y: 'bottom' };
const ACROSS_START: Record<Line['axis'], 'left' | 'top'> = { x: 'top', y: 'left' };
const ACROSS_END: Record<Line['axis'], 'right' | 'bottom'> = { x: 'bottom', y: 'right' };

/** The lines of one monitor: every zone edge not on the monitor's border, joined with the
 * edges at the same place whose extents overlap, on either side. Moving a line moves all of
 * them, so zones that tile stay tiled with no overlap and no gap. */
export function lines(drafts: Draft[], monitor: string): Line[] {
  const here = drafts.filter((d) => d.monitor === monitor);
  const out: Line[] = [];
  for (const axis of ['x', 'y'] as const) {
    const edges = new Map<number, Edge[]>();
    const add = (at: number, e: Edge) => {
      const known = [...edges.keys()].find((k) => near(k, at));
      const list = edges.get(known ?? at) ?? [];
      list.push(e);
      edges.set(known ?? at, list);
    };
    for (const z of here) {
      const a = z.area;
      const span = { from: a[ACROSS_START[axis]], to: a[ACROSS_END[axis]] };
      if (a[END[axis]] < 1 - EPSILON) add(a[END[axis]], { zone: z, side: 'before', ...span });
      if (a[START[axis]] > EPSILON) add(a[START[axis]], { zone: z, side: 'after', ...span });
    }
    for (const [at, list] of edges) {
      // Edges whose extents overlap belong to one line, however many hops apart.
      const groups: Edge[][] = [];
      for (const e of [...list].sort((p, q) => p.from - q.from)) {
        const joined = groups.filter((g) => g.some((o) => e.from < o.to - EPSILON && e.to > o.from + EPSILON));
        const merged = [e, ...joined.flat()];
        for (const g of joined) groups.splice(groups.indexOf(g), 1);
        groups.push(merged);
      }
      for (const g of groups) {
        const before = g
          .filter((e) => e.side === 'before')
          .map((e) => e.zone.key)
          .sort();
        const after = g
          .filter((e) => e.side === 'after')
          .map((e) => e.zone.key)
          .sort();
        out.push({
          id: `${axis}:${before.join(',')}|${after.join(',')}`,
          axis,
          at,
          from: Math.min(...g.map((e) => e.from)),
          to: Math.max(...g.map((e) => e.to)),
          before,
          after,
        });
      }
    }
  }
  return out;
}

/** How far a line can go: each zone it moves keeps the minimum size, and it stops at zones
 * beside it that it does not move. */
export function lineRange(drafts: Draft[], line: Line, monitor: string): { min: number; max: number } {
  let min = 0;
  let max = 1;
  const s = START[line.axis];
  const e = END[line.axis];
  for (const d of drafts) {
    if (d.monitor !== monitor) continue;
    const a = d.area;
    if (line.before.includes(d.key)) min = Math.max(min, a[s] + MIN_ZONE);
    else if (line.after.includes(d.key)) max = Math.min(max, a[e] - MIN_ZONE);
    else {
      const overlaps = a[ACROSS_START[line.axis]] < line.to - EPSILON && a[ACROSS_END[line.axis]] > line.from + EPSILON;
      if (!overlaps) continue;
      if (a[e] <= line.at + EPSILON) min = Math.max(min, a[e]);
      if (a[s] >= line.at - EPSILON) max = Math.min(max, a[s]);
    }
  }
  return { min, max };
}

/** Moves a line to `to` (a fraction of the work area), as far as it can go. */
export function moveLine(drafts: Draft[], line: Line, monitor: string, to: number): Draft[] {
  const { min, max } = lineRange(drafts, line, monitor);
  if (min > max + EPSILON) return drafts;
  const at = clamp(to, min, max);
  if (near(at, line.at)) return drafts;
  const before = new Set(line.before);
  const after = new Set(line.after);
  return drafts.map((d) => {
    if (before.has(d.key)) return { ...d, area: { ...d.area, [END[line.axis]]: at } };
    if (after.has(d.key)) return { ...d, area: { ...d.area, [START[line.axis]]: at } };
    return d;
  });
}

/** What a screen reader calls a line: "Line between zone 1 and zone 2", or for a longer one
 * "Line right of zone 1 and zone 3, left of zone 2". */
export function lineLabel(drafts: Draft[], line: Line): string {
  const names = (keys: string[]) => {
    const list = keys.map((k) => {
      const name = drafts.find((d) => d.key === k)?.name.trim() ?? '';
      return name === '' ? 'a zone' : `zone ${name}`;
    });
    return list.length <= 1 ? (list[0] ?? '') : `${list.slice(0, -1).join(', ')} and ${list.at(-1) ?? ''}`;
  };
  const [first, second] = line.axis === 'x' ? ['right of', 'left of'] : ['below', 'above'];
  if (line.before.length === 1 && line.after.length === 1) {
    return `Line between ${names(line.before)} and ${names(line.after)}`;
  }
  const parts = [
    line.before.length ? `${first} ${names(line.before)}` : '',
    line.after.length ? `${second} ${names(line.after)}` : '',
  ].filter(Boolean);
  return `Line ${parts.join(', ')}`;
}

/** Splits a zone in two, side by side (`columns`) or one above the other (`rows`). The new half
 * gets a colour of its own (and a number once `numbered`). Unchanged when either half would be
 * too small once on the grid, or the monitor is full. */
export function split(drafts: Draft[], key: string, direction: 'columns' | 'rows'): Draft[] {
  const z = drafts.find((d) => d.key === key);
  if (!z) return drafts;
  if (drafts.filter((d) => d.monitor === z.monitor).length >= MAX_ZONES) return drafts;
  const a = z.area;
  const [lo, hi] = direction === 'columns' ? [a.left, a.right] : [a.top, a.bottom];
  const middle = snap((lo + hi) / 2);
  if (middle - lo < MIN_ZONE - EPSILON || hi - middle < MIN_ZONE - EPSILON) return drafts;
  const first: RatioRect = direction === 'columns' ? { ...a, right: middle } : { ...a, bottom: middle };
  const second: RatioRect = direction === 'columns' ? { ...a, left: middle } : { ...a, top: middle };
  const added: Draft = {
    key: newKey(),
    id: null,
    name: '',
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

export function recolor(drafts: Draft[], key: string, color: ZoneColor): Draft[] {
  return drafts.map((d) => (d.key === key ? { ...d, color } : d));
}

/** The same presets as `mosaik_core::presets::Preset`, by the same ids and shapes. */
export type Preset =
  | 'full'
  | 'halves'
  | 'thirds'
  | 'main-side'
  | 'side-main'
  | 'quarters'
  | 'half-stack'
  | 'stack-half'
  | 'rows'
  | 'three-rows';

export const PRESETS: readonly { id: Preset; label: string }[] = [
  { id: 'full', label: 'Full screen' },
  { id: 'halves', label: 'Halves' },
  { id: 'thirds', label: 'Thirds' },
  { id: 'main-side', label: 'Main + side' },
  { id: 'side-main', label: 'Side + main' },
  { id: 'quarters', label: 'Quarters' },
  { id: 'half-stack', label: 'Half + stack' },
  { id: 'stack-half', label: 'Stack + half' },
  { id: 'rows', label: 'Top + bottom' },
  { id: 'three-rows', label: 'Three rows' },
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
  'half-stack': [
    { left: 0, top: 0, right: 0.5, bottom: 1 },
    { left: 0.5, top: 0, right: 1, bottom: 0.5 },
    { left: 0.5, top: 0.5, right: 1, bottom: 1 },
  ],
  'stack-half': [
    { left: 0, top: 0, right: 0.5, bottom: 0.5 },
    { left: 0.5, top: 0, right: 1, bottom: 1 },
    { left: 0, top: 0.5, right: 0.5, bottom: 1 },
  ],
  rows: [
    { left: 0, top: 0, right: 1, bottom: 0.5 },
    { left: 0, top: 0.5, right: 1, bottom: 1 },
  ],
  'three-rows': [
    { left: 0, top: 0, right: 1, bottom: 1 / 3 },
    { left: 0, top: 1 / 3, right: 1, bottom: 2 / 3 },
    { left: 0, top: 2 / 3, right: 1, bottom: 1 },
  ],
};

/** The shape of a preset, for drawing it. */
export function presetShape(preset: Preset): readonly RatioRect[] {
  return SHAPES[preset];
}

/** Replaces one monitor's zones with a preset. Existing zones are reused in reading order (they
 * keep their ids and colours, so their windows stay in them); extra ones are removed. */
export function applyPreset(drafts: Draft[], monitor: string, preset: Preset): Draft[] {
  const here = drafts
    .filter((d) => d.monitor === monitor)
    .sort((x, y) => x.area.top - y.area.top || x.area.left - y.area.left);
  const others = drafts.filter((d) => d.monitor !== monitor);
  const made: Draft[] = [];
  SHAPES[preset].forEach((area, i) => {
    const existing = here[i];
    if (existing) {
      made.push({ ...existing, area });
    } else {
      const all = [...others, ...made];
      made.push({ key: newKey(), id: null, name: '', color: freeColor(all, monitor), monitor, area });
    }
  });
  return [...others, ...made];
}

/** Why the draft cannot be saved, or null. (Zones per monitor are limited where they are made:
 * splitting refuses past the limit.) */
export function problem(drafts: Draft[]): string | null {
  for (const d of drafts) {
    const a = d.area;
    if (a.right - a.left < MIN_ZONE - EPSILON || a.bottom - a.top < MIN_ZONE - EPSILON) {
      return `Zone ${d.name} is too small.`;
    }
  }
  return null;
}

/** A percentage for people, to a tenth: 1/3 → "33.3%". */
export function percent(v: number): string {
  return `${String(Math.round(v * 1000) / 10)}%`;
}
