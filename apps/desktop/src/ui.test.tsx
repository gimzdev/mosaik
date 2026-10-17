import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { StrictMode, useEffect, useLayoutEffect, useRef } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { MenuButton, PageTitle } from './ui';

afterEach(cleanup);

const rect = (top: number, height: number) =>
  ({ top, bottom: top + height, left: 0, right: 180, width: 180, height, x: 0, y: top, toJSON: () => ({}) }) as DOMRect;

/** The menu's button at `top` (32 px tall), its list `list` px tall, in a window `window` px tall. */
function place(top: number, list: number, window: number) {
  vi.spyOn(globalThis, 'innerHeight', 'get').mockReturnValue(window);
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
    if (this.getAttribute('aria-label') === 'More for “Ship”') return rect(top, 32);
    if (this.dataset.side !== undefined) return rect(0, list);
    return rect(0, 0);
  });
}

async function open() {
  render(
    <MenuButton
      label="More for “Ship”"
      items={['Change its apps…', 'Edit zones', 'Add shortcut…', 'Rename…', 'Delete…'].map((label) => ({
        label,
        onSelect: () => undefined,
      }))}
    >
      ⋯
    </MenuButton>,
  );
  const button = screen.getByRole('button', { name: 'More for “Ship”' });
  await userEvent.click(button);
  const list = document.getElementById(button.getAttribute('aria-controls') ?? '');
  if (!list) throw new Error('the menu did not open');
  return list;
}

describe('a menu', () => {
  it('opens below its button when there is room', async () => {
    place(100, 180, 600);
    expect((await open()).dataset.side).toBe('bottom');
  });

  it('opens above its button near the bottom of the window, so its items are in view', async () => {
    place(540, 180, 600);
    expect((await open()).dataset.side).toBe('top');
  });

  it('scrolls into view when it has room neither above nor below', async () => {
    const scrolled = vi.spyOn(Element.prototype, 'scrollIntoView');
    place(60, 180, 240);
    const list = await open();
    expect(scrolled).toHaveBeenCalled();
    expect(scrolled.mock.contexts[0]).toBe(list);
  });
});

describe('a page’s title', () => {
  it('has the focus as the page is drawn, not a moment later', () => {
    // Job 0177 on the PC, and 1 run in 12 here: a test pressed Home on a line of a workspace's
    // picture as the page appeared, and the title, taking the focus a moment later (once the page
    // was on screen), took the key. What comes after it on the page is drawn with the focus on it.
    let seen: Element | null = null;
    function After() {
      useLayoutEffect(() => {
        seen = document.activeElement;
      });
      return null;
    }
    render(
      <main>
        <PageTitle>Ship</PageTitle>
        <After />
      </main>,
    );
    expect(seen).toBe(screen.getByRole('heading', { name: 'Ship' }));
  });

  it('takes it once: moved on from it as the page opens, it stays where it was moved', () => {
    // The smoke test of the keyboard, 1 run in 25 with the title focused as the page is drawn:
    // React's checks in development run a page's effects a second time once it is on screen, and
    // the title took the focus back from where Shift+Tab had just put it.
    function MovedOn() {
      const moved = useRef(false);
      useEffect(() => {
        if (moved.current) return;
        moved.current = true;
        screen.getByRole('button', { name: 'Switch workspace' }).focus();
      }, []);
      return <button type="button">Switch workspace</button>;
    }
    render(
      <StrictMode>
        <main>
          <PageTitle>Ship</PageTitle>
          <MovedOn />
        </main>
      </StrictMode>,
    );
    expect(screen.getByRole('button', { name: 'Switch workspace' })).toHaveFocus();
  });
});
