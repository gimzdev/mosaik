import { cleanup, render, waitFor } from '@testing-library/react';
import { useLayoutEffect } from 'react';
import { createRoot } from 'react-dom/client';
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

  it('shows an icon that came in between drawing it and asking for it', async () => {
    // The pictures of the window for the README, 2026-10-16: Suggested's sets showed only the
    // icons of apps no workspace above them has. A set drawn while a workspace's read of the same
    // icon was under way, that read coming in before the set asked for it (React asks after the
    // paint), was never drawn again: the icon stayed missing.
    const key = 'app:Drawn.Before.Read';
    let answer: (found: Record<string, string | null>) => void = () => undefined;
    engine.appIcons.mockImplementation(
      () =>
        new Promise((resolve) => {
          answer = resolve;
        }),
    );
    // Rendered as the window does, outside act(): effects run after the paint, not at once.
    const env = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean | undefined };
    const actEnvironment = env.IS_REACT_ACT_ENVIRONMENT;
    env.IS_REACT_ACT_ENVIRONMENT = false;
    const box = document.createElement('div');
    document.body.append(box);
    const root = createRoot(box);
    try {
      /** The read comes in as soon as the second icon is on the page, before its effects. */
      function ReadComesIn() {
        useLayoutEffect(() => {
          answer({ [key]: 'data:image/png;base64,AAAA' });
        }, []);
        return null;
      }
      const page = (second: boolean) => (
        <>
          <span data-row="workspace">
            <AppIcon icon={key} />
          </span>
          {second ? (
            <span data-row="set">
              <AppIcon icon={key} />
              <ReadComesIn />
            </span>
          ) : null}
        </>
      );
      root.render(page(false));
      await vi.waitFor(() => {
        expect(engine.appIcons).toHaveBeenCalledWith([key]);
      });
      root.render(page(true));
      await vi.waitFor(() => {
        expect(box.querySelector('[data-row="workspace"] img')).not.toBeNull();
        expect(box.querySelector('[data-row="set"] img')).not.toBeNull();
      });
    } finally {
      root.unmount();
      box.remove();
      env.IS_REACT_ACT_ENVIRONMENT = actEnvironment;
    }
  });
});
