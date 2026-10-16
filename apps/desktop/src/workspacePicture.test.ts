import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Command, Reply } from './protocol';
import { opening, openingZones, zonesReply } from './test-fixtures';
import { resizeZones } from './workspacePicture';

const engine = vi.hoisted(() => ({ request: vi.fn<(c: Command) => Promise<Reply>>() }));
vi.mock('./engine', () => engine);

beforeEach(() => {
  engine.request.mockReset();
});

describe('resizing a workspace’s zones from its picture', () => {
  it('sends nothing that would put one zone over another, or make one too small', async () => {
    const asked: Command[] = [];
    engine.request.mockImplementation((c) => {
      asked.push(c);
      return Promise.resolve(zonesReply(openingZones));
    });
    const over = await resizeZones('ship', null, opening, [
      { left: 0, top: 0, right: 0.7, bottom: 1 },
      { left: 0.6, top: 0, right: 1, bottom: 1 },
    ]);
    expect(over.done).toBe(false);
    expect(over.text).toBe('Zones 1 and 2 would overlap, so nothing was resized.');
    const small = await resizeZones('ship', null, opening, [
      { left: 0, top: 0, right: 0.98, bottom: 1 },
      { left: 0.98, top: 0, right: 1, bottom: 1 },
    ]);
    expect(small.done).toBe(false);
    expect(small.text).toBe('Zone 2 would be too small, so nothing was resized.');
    expect(asked.filter((c) => c.cmd === 'set_zones')).toEqual([]);
  });
});
