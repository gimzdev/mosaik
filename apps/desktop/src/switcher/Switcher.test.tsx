import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { WorkspaceSummary } from '../protocol';
import { Switcher, indexForKey, keyLabelFor } from './Switcher';

const workspaces: WorkspaceSummary[] = [
  { id: 'ship', name: 'Ship the release', windows: 4, zones: 3, hotkey: null },
  { id: 'brand', name: 'Brand refresh', windows: 3, zones: 2, hotkey: null },
  { id: 'mail', name: 'Mail and calendar', windows: 1, zones: 1, hotkey: null },
];

function setup(list = workspaces) {
  const handlers = { onOpen: vi.fn(), onSave: vi.fn(), onClose: vi.fn() };
  render(<Switcher workspaces={list} active="brand" shortcut="Win+Alt+Space" {...handlers} />);
  return handlers;
}

afterEach(cleanup);

describe('keys', () => {
  it('maps 1-9 then F1 and up to positions', () => {
    expect(indexForKey('1')).toBe(0);
    expect(indexForKey('9')).toBe(8);
    expect(indexForKey('F1')).toBe(9);
    expect(indexForKey('F4')).toBe(12);
    expect(indexForKey('0')).toBeNull();
    expect(indexForKey('a')).toBeNull();
    expect(keyLabelFor(0)).toBe('1');
    expect(keyLabelFor(9)).toBe('F1');
  });
});

describe('Switcher', () => {
  it('opens a workspace with its number key', async () => {
    const h = setup();
    await userEvent.keyboard('2');
    expect(h.onOpen).toHaveBeenCalledWith(workspaces[1]);
  });

  it('moves with the arrows and opens with Enter', async () => {
    const h = setup();
    await userEvent.keyboard('{ArrowDown}{ArrowDown}{Enter}');
    expect(h.onOpen).toHaveBeenCalledWith(workspaces[2]);
  });

  it('filters by typing, and digits are text while filtering', async () => {
    const h = setup();
    await userEvent.keyboard('r');
    // "Ship the release", "Brand refresh" and "Mail and calendar" all contain an r.
    expect(screen.getAllByRole('option')).toHaveLength(3);
    await userEvent.keyboard('{Backspace}m');
    expect(screen.getAllByRole('option').map((o) => o.textContent)).toEqual([
      expect.stringContaining('Mail and calendar'),
    ]);
    await userEvent.keyboard('ail1');
    expect(h.onOpen).not.toHaveBeenCalled();
    expect(screen.getByRole('combobox')).toHaveProperty('value', 'mail1');
  });

  it('saves with S and closes with Esc', async () => {
    const h = setup();
    await userEvent.keyboard('s');
    expect(h.onSave).toHaveBeenCalledOnce();
    await userEvent.keyboard('{Escape}');
    expect(h.onClose).toHaveBeenCalledOnce();
  });

  it('Esc clears the filter before closing', async () => {
    const h = setup();
    await userEvent.keyboard('zz{Escape}');
    expect(h.onClose).not.toHaveBeenCalled();
    expect(screen.getByRole('combobox')).toHaveProperty('value', '');
  });

  it('says what to do when there are no workspaces', () => {
    setup([]);
    expect(screen.getByText('No workspaces yet')).toBeTruthy();
    expect(screen.getByText(/press S to save/)).toBeTruthy();
  });

  it('labels the list for screen readers and marks the selection', () => {
    setup();
    const list = screen.getByRole('listbox', { name: 'Workspaces' });
    expect(list).toBeTruthy();
    const options = screen.getAllByRole('option');
    expect(options[0]?.getAttribute('aria-selected')).toBe('true');
    expect(screen.getByRole('combobox').getAttribute('aria-activedescendant')).toBe(options[0]?.id);
  });
});
