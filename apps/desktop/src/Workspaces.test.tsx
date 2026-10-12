import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Command, Reply } from './protocol';
import { Workspaces } from './Workspaces';

const engine = vi.hoisted(() => ({ request: vi.fn<(c: Command) => Promise<Reply>>() }));
vi.mock('./engine', () => engine);

afterEach(cleanup);

const list = (names: string[]): Reply => ({
  reply: 'workspaces',
  workspaces: names.map((name, i) => ({ id: `w${String(i)}`, name, windows: 3, zones: 1, hotkey: i === 0 ? 'Win+Alt+1' : null })),
  active: 'w0',
});

describe('the workspaces page', () => {
  beforeEach(() => {
    engine.request.mockReset();
  });

  it('explains how to save one when there is none', async () => {
    engine.request.mockResolvedValue(list([]));
    render(<Workspaces switcherShortcut="Win+Alt+Space" onEditZones={vi.fn()} />);
    expect(await screen.findByRole('heading', { name: 'No workspaces yet' })).toBeTruthy();
  });

  it('lists workspaces with what they hold and opens one', async () => {
    engine.request.mockImplementation((c) =>
      Promise.resolve(c.cmd === 'switch' ? { reply: 'done', message: 'Opened “Brand”.' } : list(['Ship', 'Brand'])),
    );
    render(<Workspaces switcherShortcut="Win+Alt+Space" onEditZones={vi.fn()} />);
    const rows = await screen.findAllByRole('listitem');
    expect(rows).toHaveLength(2);
    const first = rows[0];
    if (!first) throw new Error('no rows');
    expect(within(first).getByText('Open now')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Open “Brand”' }));
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'switch', workspace: 'w1' });
    expect(await screen.findByText('Opened “Brand”.')).toBeTruthy();
  });

  it('asks before deleting', async () => {
    engine.request.mockImplementation((c) =>
      Promise.resolve(c.cmd === 'delete_workspace' ? { reply: 'done', message: 'Deleted “Ship”.' } : list(['Ship'])),
    );
    render(<Workspaces switcherShortcut="Win+Alt+Space" onEditZones={vi.fn()} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Delete “Ship”' }));
    expect(engine.request).not.toHaveBeenCalledWith({ cmd: 'delete_workspace', workspace: 'w0' });
    await userEvent.click(screen.getByRole('button', { name: 'Delete workspace', hidden: true }));
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'delete_workspace', workspace: 'w0' });
  });

  it('shows why a rename was refused', async () => {
    engine.request.mockImplementation((c) =>
      Promise.resolve(
        c.cmd === 'rename_workspace'
          ? { reply: 'error', code: 'bad_request', message: 'Another workspace is called “Brand”.', hint: null }
          : list(['Ship', 'Brand']),
      ),
    );
    render(<Workspaces switcherShortcut="Win+Alt+Space" onEditZones={vi.fn()} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Rename “Ship”' }));
    const field = screen.getByRole('textbox', { name: 'New name' });
    await userEvent.clear(field);
    await userEvent.type(field, 'brand');
    await userEvent.click(screen.getByRole('button', { name: 'Save name' }));
    expect(await screen.findByRole('alert')).toHaveProperty('textContent', 'Another workspace is called “Brand”.');
  });
});
