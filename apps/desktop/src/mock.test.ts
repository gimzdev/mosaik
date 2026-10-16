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
});
