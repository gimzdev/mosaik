import { describe, expect, it } from 'vitest';

import type { ZoneEdit } from '../protocol';
import { GRID, MAX_ZONES, MIN_ZONE, applyPreset, fromZones, problem, remove, resize, split, toZones } from './model';

const zone = (id: string, left: number, top: number, right: number, bottom: number, monitor = 'm1'): ZoneEdit => ({
  id,
  name: id.toUpperCase(),
  color: 'lapis',
  monitor,
  area: { left, top, right, bottom },
});

// Code on the left, Browser over Terminal on the right.
const layout = () =>
  fromZones([zone('z1', 0, 0, 0.6, 1), zone('z2', 0.6, 0, 1, 0.5), zone('z3', 0.6, 0.5, 1, 1), zone('o1', 0, 0, 1, 1, 'm2')]);

const area = (drafts: ReturnType<typeof layout>, key: string) => drafts.find((d) => d.key === key)?.area;

describe('the zone editor', () => {
  it('moves both zones that share a line', () => {
    const next = resize(layout(), 'z1', 'right', 0.5);
    expect(area(next, 'z1')?.right).toBe(0.5);
    expect(area(next, 'z2')?.left).toBe(0.5);
    expect(area(next, 'z3')?.left).toBe(0.5);
    // The other monitor's zone is not touched.
    expect(area(next, 'o1')).toEqual({ left: 0, top: 0, right: 1, bottom: 1 });
  });

  it('only moves the zones along the line it touches', () => {
    const next = resize(layout(), 'z2', 'bottom', 0.25);
    expect(area(next, 'z2')?.bottom).toBe(0.25);
    expect(area(next, 'z3')?.top).toBe(0.25);
    expect(area(next, 'z1')).toEqual({ left: 0, top: 0, right: 0.6, bottom: 1 });
  });

  it('never makes a zone smaller than the minimum', () => {
    const next = resize(layout(), 'z1', 'right', 0.99);
    expect(area(next, 'z2')?.left).toBeCloseTo(1 - MIN_ZONE);
    const back = resize(layout(), 'z1', 'right', -1);
    expect(area(back, 'z1')?.right).toBeCloseTo(MIN_ZONE);
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

  it('refuses splits that would be too small or too many', () => {
    const tiny = fromZones([zone('z1', 0, 0, 0.08, 1)]);
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
});
