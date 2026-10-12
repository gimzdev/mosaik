import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Command, Reply } from './protocol';
import { Feedback } from './ui';
import { Workspaces } from './Workspaces';

const engine = vi.hoisted(() => ({
  request: vi.fn<(c: Command) => Promise<Reply>>(),
  onWorkspacesChanged: vi.fn<(f: () => void) => () => void>(() => () => undefined),
}));
vi.mock('./engine', () => engine);

afterEach(cleanup);

const list = (names: string[]): Reply => ({
  reply: 'workspaces',
  workspaces: names.map((name, i) => ({
    id: `w${String(i)}`,
    name,
    windows: 3,
    zones: 1,
    hotkey: i === 0 ? 'Win+Alt+J' : null,
  })),
  active: 'w0',
});

function page() {
  return render(
    <Feedback>
      <Workspaces shortcut="Win+Alt+Space" onEditZones={vi.fn()} />
    </Feedback>,
  );
}

describe('the workspaces page', () => {
  beforeEach(() => {
    engine.request.mockReset();
    engine.onWorkspacesChanged.mockClear();
  });

  it('explains how to save one when there is none', async () => {
    engine.request.mockResolvedValue(list([]));
    page();
    expect(await screen.findByRole('heading', { name: 'No workspaces yet' })).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Open the switcher' }));
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'show_switcher' });
  });

  it('lists workspaces with what they hold and opens one', async () => {
    engine.request.mockImplementation((c) =>
      Promise.resolve(c.cmd === 'switch' ? { reply: 'done', message: 'Opened “Brand”.' } : list(['Ship', 'Brand'])),
    );
    page();
    const rows = await screen.findAllByRole('listitem');
    expect(rows).toHaveLength(2);
    const first = rows[0];
    if (!first) throw new Error('no rows');
    expect(within(first).getByText('Open now')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Open “Brand”' }));
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'switch', workspace: 'w1' });
    expect(await screen.findByText('Opened “Brand”.')).toBeTruthy();
  });

  it('arranges the windows on screen, and undoes it', async () => {
    engine.request.mockImplementation((c) => {
      if (c.cmd === 'arrange') return Promise.resolve({ reply: 'done', message: 'Arranged 4 windows in 3 zones.' });
      if (c.cmd === 'undo_arrange')
        return Promise.resolve({ reply: 'done', message: 'Windows are back where they were.' });
      return Promise.resolve(list(['Ship']));
    });
    page();
    await screen.findAllByRole('listitem');
    expect(screen.queryByRole('button', { name: 'Undo arrange' })).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Arrange windows' }));
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'arrange' });
    expect(await screen.findByText('Arranged 4 windows in 3 zones.')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Undo arrange' }));
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'undo_arrange' });
    expect(await screen.findByText('Windows are back where they were.')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Undo arrange' })).toBeNull();
  });

  it('says why the windows could not be arranged', async () => {
    engine.request.mockImplementation((c) =>
      Promise.resolve(
        c.cmd === 'arrange'
          ? {
              reply: 'error',
              code: 'failed',
              message: 'There are no windows on screen to arrange.',
              hint: 'Open the apps you work with, then arrange them.',
            }
          : list([]),
      ),
    );
    page();
    await screen.findByRole('heading', { name: 'No workspaces yet' });
    await userEvent.click(screen.getByRole('button', { name: 'Arrange windows' }));
    expect(
      await screen.findByText(
        'There are no windows on screen to arrange. Open the apps you work with, then arrange them.',
      ),
    ).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Undo arrange' })).toBeNull();
  });

  it('reads them again when they change elsewhere', async () => {
    engine.request.mockResolvedValue(list(['Ship']));
    page();
    await screen.findAllByRole('listitem');
    const changed = engine.onWorkspacesChanged.mock.calls[0]?.[0];
    if (!changed) throw new Error('the page does not listen for changes');
    engine.request.mockResolvedValue(list(['Ship', 'Saved from the switcher']));
    changed();
    expect(await screen.findByText('Saved from the switcher')).toBeTruthy();
  });

  it('asks before deleting', async () => {
    engine.request.mockImplementation((c) =>
      Promise.resolve(c.cmd === 'delete_workspace' ? { reply: 'done', message: 'Deleted “Ship”.' } : list(['Ship'])),
    );
    page();
    await userEvent.click(await screen.findByRole('button', { name: 'More for “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Delete…' }));
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
    page();
    await userEvent.click(await screen.findByRole('button', { name: 'More for “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Rename…' }));
    const field = screen.getByRole('textbox', { name: 'New name' });
    expect(document.activeElement).toBe(field);
    await userEvent.clear(field);
    await userEvent.type(field, 'brand');
    await userEvent.click(screen.getByRole('button', { name: 'Save name' }));
    expect((await screen.findAllByRole('alert')).map((a) => a.textContent)).toContain(
      'Another workspace is called “Brand”.',
    );
  });

  it('records a shortcut for a workspace with Mosaïk’s own shortcuts held', async () => {
    engine.request.mockImplementation((c) =>
      Promise.resolve(c.cmd === 'workspaces' ? list(['Ship']) : { reply: 'done', message: 'ok' }),
    );
    page();
    await userEvent.click(await screen.findByRole('button', { name: 'More for “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Change shortcut…' }));
    expect(await screen.findByRole('button', { name: 'Press the new shortcut' })).toBeTruthy();
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'hold_hotkeys', hold: true });
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('button', { name: 'Press the new shortcut' })).toBeNull();
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'hold_hotkeys', hold: false });
  });
});
