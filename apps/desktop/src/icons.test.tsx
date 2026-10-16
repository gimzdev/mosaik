import { cleanup, render, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { AppIcon } from './icons';

const engine = vi.hoisted(() => ({
  appIcons: vi.fn<(keys: string[]) => Promise<Record<string, string | null>>>(),
}));
vi.mock('./engine', () => engine);

afterEach(cleanup);

describe('apps’ icons', () => {
  it('reads a long list a part at a time, one read after another, and shows every icon', async () => {
    // Job 0172's tour: Add an app lists every app of the Start menu (over 200 on the owner's PC)
    // and asked for all their icons in one read, which Mosaïk cuts at 96 keys
    // (`mosaik_win::icons::MOST_PER_REQUEST`): the others were taken for apps with no icon.
    const keys = Array.from({ length: 230 }, (_, i) => `app:Long.List.${String(i)}`);
    let underWay = 0;
    let most = 0;
    const asked: string[][] = [];
    engine.appIcons.mockImplementation(async (part) => {
      asked.push(part);
      underWay += 1;
      most = Math.max(most, underWay);
      await new Promise((resolve) => setTimeout(resolve, 5));
      underWay -= 1;
      // As Mosaïk answers: the first 96 keys of a read.
      return Object.fromEntries(part.slice(0, 96).map((k) => [k, `data:image/png;base64,${k}`]));
    });
    const { container } = render(
      <div>
        {keys.map((k) => (
          <AppIcon key={k} icon={k} />
        ))}
      </div>,
    );
    await waitFor(() => {
      expect(container.querySelectorAll('img')).toHaveLength(keys.length);
    });
    expect(asked.flat().sort()).toEqual([...keys].sort());
    expect(Math.max(...asked.map((p) => p.length))).toBeLessThanOrEqual(24);
    expect(most).toBe(1);
    // The first ones on the page come first.
    expect(asked[0]).toEqual(keys.slice(0, 24));
  });
});
