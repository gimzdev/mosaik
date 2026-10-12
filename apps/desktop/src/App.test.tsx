import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import wire from '../../../crates/mosaik-ipc/fixtures/wire.json';
import { App } from './App';
import { type Command, type HotkeyState, type Reply, type Settings, isReply } from './protocol';

const engine = vi.hoisted(() => ({
  request: vi.fn<(c: Command) => Promise<Reply>>(),
  takeNotices: vi.fn(() => Promise.resolve([])),
  onNotices: vi.fn(() => () => undefined),
  onWorkspacesChanged: vi.fn(() => () => undefined),
  openLink: vi.fn(() => Promise.resolve(null)),
  openLogs: vi.fn(() => Promise.resolve(null)),
  openLicences: vi.fn(() => Promise.resolve(null)),
  rescueWindows: vi.fn(() => Promise.resolve({ text: 'ok', error: false })),
  styleTitleBar: vi.fn(),
  guardClose: vi.fn(() => () => undefined),
}));
vi.mock('./engine', () => engine);

afterEach(cleanup);
beforeEach(() => {
  engine.request.mockReset();
});

// The settings the engine sends on a first run (onboarding not done), from the shared fixture.
const fixture = wire.replies.find((r) => r.reply === 'settings');
if (!isReply(fixture) || fixture.reply !== 'settings') throw new Error('fixture has no settings reply');
const firstRun: Settings = fixture.settings;
const registered: HotkeyState = { shortcut: 'Win+Alt+Space', registered: true, problem: null };

const settingsReply = (settings: Settings, hotkey = registered): Reply => ({ reply: 'settings', settings, hotkey });
const noWorkspaces: Reply = { reply: 'workspaces', workspaces: [], active: null };
const ship: Reply = {
  reply: 'workspaces',
  workspaces: [{ id: 'ship', name: 'Ship', windows: 2, zones: 2, hotkey: null }],
  active: null,
};
const shipZones: Reply = {
  reply: 'zones',
  workspace: 'ship',
  name: 'Ship',
  monitors: [
    {
      id: 'm1',
      name: 'Screen',
      primary: true,
      bounds: { left: 0, top: 0, right: 1920, bottom: 1080 },
      work_area: { left: 0, top: 0, right: 1920, bottom: 1032 },
      dpi: 96,
      scale_percent: 100,
    },
  ],
  zones: [
    { id: 'z1', name: 'Code', color: 'lapis', monitor: 'm1', area: { left: 0, top: 0, right: 0.5, bottom: 1 } },
    { id: 'z2', name: 'Browser', color: 'verdigris', monitor: 'm1', area: { left: 0.5, top: 0, right: 1, bottom: 1 } },
  ],
  revision: '1',
};

describe('Mosaïk’s window', () => {
  it('welcomes a new user, then remembers it did', async () => {
    let saved: Command | null = null;
    engine.request.mockImplementation((c) => {
      if (c.cmd === 'set_settings') {
        saved = c;
        return Promise.resolve({ reply: 'done', message: 'Settings saved.' });
      }
      if (c.cmd === 'workspaces') return Promise.resolve(noWorkspaces);
      return Promise.resolve(settingsReply(firstRun));
    });
    render(<App />);
    expect(await screen.findByRole('heading', { name: 'Welcome to Mosaïk' })).toBeTruthy();
    // The welcome has the whole window.
    expect(screen.queryByRole('navigation')).toBeNull();
    await userEvent.click(screen.getByRole('switch', { name: 'Start Mosaïk with Windows' }));
    await userEvent.click(screen.getByRole('button', { name: 'Get started' }));
    expect(saved).toMatchObject({ cmd: 'set_settings', settings: { onboarding_done: true, start_with_windows: true } });
    expect(await screen.findByRole('heading', { name: 'Workspaces' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Workspaces' }).getAttribute('aria-current')).toBe('page');
    // The new page's title has the focus, for keyboard and screen-reader users.
    expect(document.activeElement).toBe(screen.getByRole('heading', { name: 'Workspaces' }));
  });

  it('says why Windows kept Mosaïk from starting with it', async () => {
    engine.request.mockImplementation((c) => {
      if (c.cmd === 'set_settings') {
        // The engine saves it off (the settings read back say so) and explains.
        return Promise.resolve({
          reply: 'done',
          message: 'Settings saved. Windows did not let Mosaïk start with it: turn it on under Startup apps.',
        });
      }
      if (c.cmd === 'workspaces') return Promise.resolve(noWorkspaces);
      return Promise.resolve(settingsReply(firstRun));
    });
    render(<App />);
    await userEvent.click(await screen.findByRole('switch', { name: 'Start Mosaïk with Windows' }));
    await userEvent.click(screen.getByRole('button', { name: 'Get started' }));
    const message = await screen.findByText(/Windows did not let Mosaïk start with it/);
    expect(message.closest('[role="alert"]')).not.toBeNull();
  });

  it('shows the shortcut that really opens the switcher', async () => {
    const fallback: HotkeyState = {
      shortcut: 'Win+Alt+S',
      registered: true,
      problem: 'Win+Alt+Space is taken; using Win+Alt+S.',
    };
    engine.request.mockImplementation((c) =>
      Promise.resolve(
        c.cmd === 'workspaces' ? noWorkspaces : settingsReply({ ...firstRun, onboarding_done: true }, fallback),
      ),
    );
    render(<App />);
    const intro = (await screen.findByRole('heading', { name: 'Workspaces' })).parentElement?.parentElement;
    expect(intro?.textContent).toContain('Win plus Alt plus S');
    await userEvent.click(screen.getByRole('button', { name: 'Settings' }));
    expect(await screen.findByText('Win+Alt+Space is taken; using Win+Alt+S.')).toBeTruthy();
  });

  it('puts tags in title bars, or above windows when asked', async () => {
    let saved: Command | null = null;
    engine.request.mockImplementation((c) => {
      if (c.cmd === 'set_settings') {
        saved = c;
        return Promise.resolve({ reply: 'done', message: 'Settings saved.' });
      }
      if (c.cmd === 'workspaces') return Promise.resolve(noWorkspaces);
      return Promise.resolve(settingsReply({ ...firstRun, onboarding_done: true }));
    });
    render(<App />);
    await userEvent.click(await screen.findByRole('button', { name: 'Settings' }));
    const place = await screen.findByRole('group', { name: 'Tags and tabs' });
    expect(within(place).getByRole('radio', { name: 'In each window’s title bar' })).toHaveProperty('checked', true);
    await userEvent.click(within(place).getByRole('radio', { name: 'Above each window' }));
    expect(saved).toMatchObject({ cmd: 'set_settings', settings: { frames: { tags_in_title_bar: false } } });
  });

  it('says plainly when the engine cannot be reached, and tries again', async () => {
    engine.request.mockResolvedValueOnce({
      reply: 'error',
      code: 'failed',
      message: 'Mosaïk’s engine has stopped.',
      hint: 'Restart Mosaïk.',
    });
    render(<App />);
    const message = await screen.findByText('Mosaïk’s engine has stopped. Restart Mosaïk.');
    expect(message.getAttribute('role')).toBe('alert');
    engine.request.mockImplementation((c) =>
      Promise.resolve(c.cmd === 'workspaces' ? noWorkspaces : settingsReply({ ...firstRun, onboarding_done: true })),
    );
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByRole('heading', { name: 'Workspaces' })).toBeTruthy();
  });

  it('asks before leaving unsaved zones for another page', async () => {
    engine.request.mockImplementation((c) => {
      if (c.cmd === 'workspaces') return Promise.resolve(ship);
      if (c.cmd === 'zones') return Promise.resolve(shipZones);
      return Promise.resolve(settingsReply({ ...firstRun, onboarding_done: true }));
    });
    render(<App />);
    await userEvent.click(await screen.findByRole('button', { name: 'Edit zones of “Ship”' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Start from a preset' }));
    await userEvent.click(screen.getByRole('button', { name: 'Thirds' }));
    await userEvent.click(screen.getByRole('button', { name: 'Settings' }));
    const dialog = screen.getByRole('dialog', { name: 'Leave without saving?' });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(screen.getByRole('heading', { name: 'Ship' })).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Settings' }));
    await userEvent.click(
      within(screen.getByRole('dialog', { name: 'Leave without saving?' })).getByRole('button', {
        name: 'Discard changes',
      }),
    );
    await waitFor(() => {
      expect(screen.getByRole('heading', { name: 'Settings' })).toBeTruthy();
    });
  });
});
