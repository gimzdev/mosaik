import { describe, expect, it } from 'vitest';

import type { ZoneEdit } from '../protocol';
import {
  type Draft,
  GRID,
  MAX_ZONES,
  MIN_ZONE,
  applyPreset,
  fromZones,
  lineLabel,
  lineRange,
  lines,
  moveLine,
  percent,
  problem,
  remove,
  split,
  toZones,
} from './model';

const zone = (id: string, left: number, top: number, right: number, bottom: number, monitor = 'm1'): ZoneEdit => ({
  id,
  name: id.toUpperCase(),
  color: 'lapis',
  monitor,
  area: { left, top, right, bottom },
});

// Code on the left, Browser over Terminal on the right; another monitor with one zone.
const layout = () =>
  fromZones([
    zone('z1', 0, 0, 0.6, 1),
    zone('z2', 0.6, 0, 1, 0.5),
    zone('z3', 0.6, 0.5, 1, 1),
    zone('o1', 0, 0, 1, 1, 'm2'),
  ]);

const area = (drafts: Draft[], key: string) => drafts.find((d) => d.key === key)?.area;

function lineAt(drafts: Draft[], axis: 'x' | 'y', at: number, touching: string) {
  const found = lines(drafts, 'm1').find(
    (l) => l.axis === axis && Math.abs(l.at - at) < 1e-9 && [...l.before, ...l.after].includes(touching),
  );
  if (!found) throw new Error(`no ${axis} line at ${String(at)} by ${touching}`);
  return found;
}

/** No two zones overlap, and together they cover the monitor. */
function tiles(drafts: Draft[]) {
  const here = drafts.filter((d) => d.monitor === 'm1').map((d) => d.area);
  const total = here.reduce((sum, a) => sum + (a.right - a.left) * (a.bottom - a.top), 0);
  const overlap = here.some((a, i) =>
    here.slice(i + 1).some((b) => {
      const w = Math.min(a.right, b.right) - Math.max(a.left, b.left);
      const h = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
      return w > 1e-9 && h > 1e-9;
    }),
  );
  return !overlap && Math.abs(total - 1) < 1e-9;
}

describe('the zone editor', () => {
  it('finds one line where zones meet, with every zone along it', () => {
    const l = lineAt(layout(), 'x', 0.6, 'z1');
    expect(l.before).toEqual(['z1']);
    expect(l.after).toEqual(['z2', 'z3']);
    expect(lines(layout(), 'm1')).toHaveLength(2);
    expect(lineLabel(layout(), l)).toBe('Line right of Z1, left of Z2 and Z3');
    expect(lineLabel(layout(), lineAt(layout(), 'y', 0.5, 'z2'))).toBe('Line between Z2 and Z3');
  });

  it('moves every zone along a line, on this monitor only', () => {
    const d = layout();
    const next = moveLine(d, lineAt(d, 'x', 0.6, 'z1'), 'm1', 0.5);
    expect(area(next, 'z1')?.right).toBe(0.5);
    expect(area(next, 'z2')?.left).toBe(0.5);
    expect(area(next, 'z3')?.left).toBe(0.5);
    expect(area(next, 'o1')).toEqual({ left: 0, top: 0, right: 1, bottom: 1 });
    expect(tiles(next)).toBe(true);
  });

  it('only moves the zones on the line it is', () => {
    const d = layout();
    const next = moveLine(d, lineAt(d, 'y', 0.5, 'z2'), 'm1', 0.25);
    expect(area(next, 'z2')?.bottom).toBe(0.25);
    expect(area(next, 'z3')?.top).toBe(0.25);
    expect(area(next, 'z1')).toEqual({ left: 0, top: 0, right: 0.6, bottom: 1 });
  });

  it('never makes a zone smaller than the minimum', () => {
    const d = layout();
    const l = lineAt(d, 'x', 0.6, 'z1');
    expect(lineRange(d, l, 'm1')).toEqual({ min: MIN_ZONE, max: 1 - MIN_ZONE });
    expect(area(moveLine(d, l, 'm1', 0.99), 'z2')?.left).toBeCloseTo(1 - MIN_ZONE);
    expect(area(moveLine(d, l, 'm1', -1), 'z1')?.right).toBeCloseTo(MIN_ZONE);
  });

  it('keeps zones tiled through a T-junction', () => {
    // Quarters; the top half of the middle line moves left, then the horizontal line moves up.
    let d = applyPreset(fromZones([]), 'm1', 'quarters');
    const [tl, tr] = d;
    if (!tl || !tr) throw new Error('quarters made fewer than four zones');
    const top = lines(d, 'm1').find((l) => l.axis === 'x' && l.before.includes(tl.key));
    if (!top) throw new Error('no line right of the top-left zone');
    expect(top.after).toEqual([tr.key]);
    d = moveLine(d, top, 'm1', 0.25);
    const middle = lines(d, 'm1').find((l) => l.axis === 'y' && l.before.includes(tl.key));
    if (!middle) throw new Error('no line below the top-left zone');
    expect(middle.before).toHaveLength(2);
    expect(middle.after).toHaveLength(2);
    d = moveLine(d, middle, 'm1', 0.4);
    expect(tiles(d)).toBe(true);
  });

  it('stops at a zone beside a gap', () => {
    // A zone on the left, a gap, a zone on the right: the left one's edge stops at the right one.
    const d = fromZones([zone('a', 0, 0, 0.4, 1), zone('b', 0.6, 0, 1, 1)]);
    const l = lineAt(d, 'x', 0.4, 'a');
    expect(l.after).toEqual([]);
    expect(area(moveLine(d, l, 'm1', 0.9), 'a')?.right).toBe(0.6);
  });

  it('splits on the grid, with a new name and colour', () => {
    const next = split(layout(), 'z1', 'columns');
    expect(next).toHaveLength(5);
    const added = next.find((d) => d.id === null);
    expect(added?.area).toEqual({ left: 14 / 48, top: 0, right: 0.6, bottom: 1 });
    expect((added?.area.left ?? 0) / GRID).toBeCloseTo(14);
    expect(added?.name).toBe('Zone 1');
    expect(added?.color).not.toBe('lapis');
    expect(area(next, 'z1')?.right).toBe(14 / 48);
  });

  it('refuses splits that would be too small once on the grid, or too many', () => {
    const tiny = fromZones([zone('z1', 0, 0, 0.1, 1)]);
    // Halves of 0.054 and 0.046 once on the grid: the second would be below the minimum.
    expect(split(tiny, 'z1', 'columns')).toBe(tiny);
    const full = fromZones(Array.from({ length: MAX_ZONES }, (_, i) => zone(`z${String(i)}`, 0, 0, 1, 1)));
    expect(split(full, 'z0', 'rows')).toBe(full);
  });

  it('keeps zones and their ids when a preset replaces a monitor', () => {
    const next = applyPreset(layout(), 'm1', 'halves');
    const here = next.filter((d) => d.monitor === 'm1');
    expect(here.map((d) => d.id)).toEqual(['z1', 'z2']);
    expect(here.map((d) => d.area.right)).toEqual([0.5, 1]);
    const quarters = applyPreset(layout(), 'm1', 'quarters').filter((d) => d.monitor === 'm1');
    expect(quarters).toHaveLength(4);
    expect(quarters.filter((d) => d.id === null)).toHaveLength(1);
    expect(next.find((d) => d.key === 'o1')).toBeDefined();
  });

  it('says why a draft cannot be saved', () => {
    expect(problem(layout())).toBeNull();
    const unnamed = layout().map((d) => (d.key === 'z1' ? { ...d, name: '  ' } : d));
    expect(problem(unnamed)).toBe('Every zone needs a name.');
    expect(problem(fromZones([zone('z1', 0, 0, 0.01, 1)]))).toContain('too small');
  });

  it('returns zones as the engine takes them', () => {
    const drafts = remove(layout(), 'z3');
    expect(toZones(drafts).map((z) => z.id)).toEqual(['z1', 'z2', 'o1']);
    expect(Object.keys(toZones(drafts)[0] ?? {})).not.toContain('key');
  });

  it('writes percentages to a tenth', () => {
    expect(percent(1 / 3)).toBe('33.3%');
    expect(percent(0.5)).toBe('50%');
  });
});
