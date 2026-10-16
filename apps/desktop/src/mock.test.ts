import { describe, expect, it } from 'vitest';

import { mock } from './mock';
import type { ArrangePreview, Reply } from './protocol';

const pictured = (r: Reply): ArrangePreview => {
  if (r.reply !== 'preview') throw new Error(`no picture: ${r.reply}`);
  return r;
};

/** Where a window of the picture goes. */
const zoneOf = (p: ArrangePreview, label: string) =>
  p.zones.find((z) => z.id === p.windows.find((w) => w.label === label)?.zone)?.rect;

describe('the stand-in engine', () => {
  it('takes a window over a whole half for filling no quarter of it, as Mosaïk does', async () => {
    // The fourth review of ADR 0032: it measured how much of the zone a window covered, and so
    // took Visual Studio Code, over the left half, for filling its bottom quarter. Mosaïk
    // measures the overlap against the larger of the two (`arrange::covering`): Code takes the
    // top quarter, Windows Terminal, behind it, the bottom one
    // (`a_window_over_a_whole_half_fills_no_quarter_of_it`).
    expect((await mock.request({ cmd: 'arrange', preset: 'halves' })).reply).toBe('done');
    const quarters = pictured(await mock.request({ cmd: 'preview_arrange', preset: 'quarters' }));
    const work = quarters.work_area;
    const middle = { x: (work.left + work.right) / 2, y: (work.top + work.bottom) / 2 };
    const code = zoneOf(quarters, 'Visual Studio Code');
    const terminal = zoneOf(quarters, 'Windows Terminal');
    if (!code || !terminal) throw new Error('both are in a quarter');
    expect([code.left, code.top]).toEqual([work.left, work.top]);
    expect(code.right).toBeLessThanOrEqual(middle.x);
    expect(terminal.left).toBe(work.left);
    expect(terminal.top).toBeGreaterThanOrEqual(middle.y);
  });
  it('puts a window dropped onto a zone behind its windows, and moves no other, as Mosaïk does', async () => {
    // ADR 0033 (`a_window_dropped_onto_a_zone_joins_it_behind_its_windows`).
    const base = pictured(await mock.request({ cmd: 'preview_arrange', preset: 'halves' }));
    const zone = (p: ArrangePreview, label: string) =>
      p.zones.findIndex((z) => z.id === p.windows.find((w) => w.label === label)?.zone);
    const [first, second] = base.windows;
    if (!first || !second) throw new Error('the stand-in has windows');
    // A window of the other half, dropped onto the first one's half itself.
    const into = zone(base, first.label);
    const mover = base.windows.find((w) => zone(base, w.label) !== into);
    if (!mover) throw new Error('both halves hold a window');
    const moved = pictured(
      await mock.request({ cmd: 'preview_arrange', preset: 'halves', moves: [{ window: mover.window, zone: into }] }),
    );
    const there = (p: ArrangePreview, z: number) => p.windows.filter((w) => zone(p, w.label) === z).map((w) => w.label);
    expect(there(moved, into)).toEqual([...there(base, into), mover.label]);
    for (const w of base.windows) {
      if (w !== mover) expect(zone(moved, w.label)).toBe(zone(base, w.label));
    }
    expect(moved.zones.map((z) => z.color)).toEqual(base.zones.map((z) => z.color));
  });

  it('keeps every window in its zone when a line between zones is dragged, as Mosaïk does', async () => {
    // ADR 0033 (`random_lines_dragged_move_no_window_to_another_zone`): the stand-in found the
    // windows in the zones as stretched, so a window filling one of them took it.
    for (const preset of ['thirds', 'quarters', 'main-side', 'side-main', 'half-stack', 'stack-half'] as const) {
      const base = pictured(await mock.request({ cmd: 'preview_arrange', preset }));
      const lines = [...new Set(base.zones.flatMap((z) => [z.area?.left ?? 0, z.area?.right ?? 1]))].filter(
        (v) => v > 0.01 && v < 0.99,
      );
      for (const line of lines) {
        for (const x of [0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8]) {
          const areas = base.zones.map((z) => {
            const a = z.area ?? { left: 0, top: 0, right: 1, bottom: 1 };
            const f = (v: number) => (Math.abs(v - line) < 1e-9 ? x : v);
            return { left: f(a.left), top: a.top, right: f(a.right), bottom: a.bottom };
          });
          const stretched = pictured(await mock.request({ cmd: 'preview_arrange', preset, areas }));
          for (const w of base.windows) {
            const now = stretched.windows.find((v) => v.window === w.window);
            expect(now?.zone, `${preset}, the line at ${String(line)} dragged to ${String(x)}: ${w.label}`).toBe(
              w.zone,
            );
          }
          expect(stretched.zones.map((z) => z.area)).toEqual(areas);
        }
      }
    }
  });
});
