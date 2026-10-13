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
  takeZonesToEdit: vi.fn<() => Promise<string | null>>(() => Promise.resolve(null)),
  onZonesToEdit: vi.fn(() => () => undefined),
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
const emptyScreen: Reply = {
  reply: 'preview',
  work_area: { left: 0, top: 0, right: 1920, bottom: 1032 },
  zones: [],
  windows: [],
  elsewhere: 0,
};
const ship: Reply = {
  reply: 'workspaces',
  workspaces: [{ id: 'ship', name: 'Ship', windows: 2, zones: 2, hotkey: null, apps: ['Code', 'Chrome'] }],
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
      if (c.cmd === 'preview_arrange') return Promise.resolve(emptyScreen);
      return Promise.resolve(settingsReply(firstRun));
    });
    render(<App />);
    expect(await screen.findByRole('heading', { name: 'Welcome to Mosaïk' })).toBeTruthy();
    // The welcome has the whole window.
    expect(screen.queryByRole('navigation')).toBeNull();
    await userEvent.click(screen.getByRole('switch', { name: 'Start Mosaïk with Windows' }));
    await userEvent.click(screen.getByRole('button', { name: 'Get started' }));
    expect(saved).toMatchObject({ cmd: 'set_settings', settings: { onboarding_done: true, start_with_windows: true } });
    // Then the home page: tidy the screen in one click.
    expect(await screen.findByRole('heading', { name: 'Tidy your screen' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Home' }).getAttribute('aria-current')).toBe('page');
    // The new page's title has the focus, for keyboard and screen-reader users.
    expect(document.activeElement).toBe(screen.getByRole('heading', { name: 'Tidy your screen' }));
  });

  it('sorts the windows open into workspaces when a new user gets started, and says so', async () => {
    const asked: string[] = [];
    engine.request.mockImplementation((c) => {
      asked.push(c.cmd);
      if (c.cmd === 'set_settings') return Promise.resolve({ reply: 'done', message: 'Settings saved.' });
      if (c.cmd === 'sort_windows') {
        return Promise.resolve({ reply: 'done', message: 'Your open windows make 2 workspaces: Coding and Games.' });
      }
      if (c.cmd === 'workspaces') return Promise.resolve(noWorkspaces);
      if (c.cmd === 'preview_arrange') return Promise.resolve(emptyScreen);
      return Promise.resolve(settingsReply(firstRun));
    });
    render(<App />);
    await userEvent.click(await screen.findByRole('button', { name: 'Get started' }));
    expect(await screen.findByText('Your open windows make 2 workspaces: Coding and Games.')).toBeTruthy();
    expect(asked.indexOf('sort_windows')).toBeGreaterThan(asked.indexOf('set_settings'));
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
      if (c.cmd === 'preview_arrange') return Promise.resolve(emptyScreen);
      return Promise.resolve(settingsReply(firstRun));
    });
    render(<App />);
    await userEvent.click(await screen.findByRole('switch', { name: 'Start Mosaïk with Windows' }));
    await userEvent.click(screen.getByRole('button', { name: 'Get started' }));
    const message = await screen.findByText(/Windows did not let Mosaïk start with it/);
    expect(message.closest('[role="alert"]')).not.toBeNull();
  });

  it('still says why Windows kept Mosaïk from starting with it when the windows are sorted too', async () => {
    engine.request.mockImplementation((c) => {
      if (c.cmd === 'set_settings') {
        return Promise.resolve({
          reply: 'done',
          message: 'Settings saved. Windows did not let Mosaïk start with it: turn it on under Startup apps.',
        });
      }
      if (c.cmd === 'sort_windows') {
        return Promise.resolve({ reply: 'done', message: 'Your open windows make 1 workspace: Coding.' });
      }
      if (c.cmd === 'workspaces') return Promise.resolve(noWorkspaces);
      if (c.cmd === 'preview_arrange') return Promise.resolve(emptyScreen);
      return Promise.resolve(settingsReply(firstRun));
    });
    render(<App />);
    await userEvent.click(await screen.findByRole('switch', { name: 'Start Mosaïk with Windows' }));
    await userEvent.click(screen.getByRole('button', { name: 'Get started' }));
    const message = await screen.findByText(/Windows did not let Mosaïk start with it/);
    expect(message.closest('[role="alert"]')).not.toBeNull();
    expect(message.textContent).toContain('Your open windows make 1 workspace: Coding.');
  });

  it('shows the shortcut that really opens the switcher', async () => {
    const fallback: HotkeyState = {
      shortcut: 'Win+Alt+S',
      registered: true,
      problem: 'Win+Alt+Space is taken; using Win+Alt+S.',
    };
    engine.request.mockImplementation((c) =>
      Promise.resolve(
        c.cmd === 'workspaces'
          ? noWorkspaces
          : c.cmd === 'preview_arrange'
            ? emptyScreen
            : settingsReply({ ...firstRun, onboarding_done: true }, fallback),
      ),
    );
    render(<App />);
    expect((await screen.findByText(/switches workspaces/)).textContent).toContain('Win plus Alt plus S');
    await userEvent.click(screen.getByRole('button', { name: 'Settings' }));
    expect(await screen.findByText('Win+Alt+Space is taken; using Win+Alt+S.')).toBeTruthy();
  });

  it('puts zone tags in title bars, above windows, or nowhere', async () => {
    const saved: Command[] = [];
    let now: Settings = { ...firstRun, onboarding_done: true };
    engine.request.mockImplementation((c) => {
      if (c.cmd === 'set_settings') {
        saved.push(c);
        now = c.settings;
        return Promise.resolve({ reply: 'done', message: 'Settings saved.' });
      }
      if (c.cmd === 'workspaces') return Promise.resolve(noWorkspaces);
      if (c.cmd === 'preview_arrange') return Promise.resolve(emptyScreen);
      return Promise.resolve(settingsReply(now));
    });
    render(<App />);
    await userEvent.click(await screen.findByRole('button', { name: 'Settings' }));
    const tags = await screen.findByRole('group', { name: 'Zone tags' });
    expect(within(tags).getByRole('radio', { name: 'In title bars' })).toHaveProperty('checked', true);
    await userEvent.click(within(tags).getByRole('radio', { name: 'Above windows' }));
    expect(saved.at(-1)).toMatchObject({ settings: { frames: { show_tags: true, tags_in_title_bar: false } } });
    await userEvent.click(within(tags).getByRole('radio', { name: 'Off' }));
    // Off hides the tags; tabs stay where the tags were.
    expect(saved.at(-1)).toMatchObject({ settings: { frames: { show_tags: false, tags_in_title_bar: false } } });
    await waitFor(() => {
      expect(within(tags).getByRole('radio', { name: 'Off' })).toHaveProperty('checked', true);
    });
  });

  it('puts the quick menu on either edge, or turns it off, in one choice', async () => {
    const saved: Command[] = [];
    let now: Settings = { ...firstRun, onboarding_done: true };
    engine.request.mockImplementation((c) => {
      if (c.cmd === 'set_settings') {
        saved.push(c);
        now = c.settings;
        return Promise.resolve({ reply: 'done', message: 'Settings saved.' });
      }
      if (c.cmd === 'workspaces') return Promise.resolve(noWorkspaces);
      if (c.cmd === 'preview_arrange') return Promise.resolve(emptyScreen);
      return Promise.resolve(settingsReply(now));
    });
    render(<App />);
    await userEvent.click(await screen.findByRole('button', { name: 'Settings' }));
    const quick = await screen.findByRole('group', { name: 'Quick menu' });
    expect(within(quick).getByRole('radio', { name: 'Right edge' })).toHaveProperty('checked', true);
    await userEvent.click(within(quick).getByRole('radio', { name: 'Off' }));
    expect(saved.at(-1)).toMatchObject({ settings: { edge: { enabled: false } } });
    await userEvent.click(within(quick).getByRole('radio', { name: 'Left edge' }));
    expect(saved.at(-1)).toMatchObject({ settings: { edge: { enabled: true, side: 'left' } } });
  });

  it('keeps the settings few people change behind More settings, and About at the end', async () => {
    engine.request.mockImplementation((c) => {
      if (c.cmd === 'workspaces') return Promise.resolve(noWorkspaces);
      if (c.cmd === 'preview_arrange') return Promise.resolve(emptyScreen);
      if (c.cmd === 'hello') return Promise.resolve({ reply: 'hello', app_version: '1.6.0', protocol: 6 });
      return Promise.resolve(settingsReply({ ...firstRun, onboarding_done: true }));
    });
    render(<App />);
    await userEvent.click(await screen.findByRole('button', { name: 'Settings' }));
    expect(await screen.findByText(/^Version 1\.6\.0\. Free forever/)).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Support on Patreon' })).toBeTruthy();
    expect(screen.queryByRole('switch', { name: 'Start apps that are not open' })).toBeNull();
    const more = screen.getByRole('button', { name: 'More settings' });
    expect(more.getAttribute('aria-expanded')).toBe('false');
    await userEvent.click(more);
    expect(more.getAttribute('aria-expanded')).toBe('true');
    expect(screen.getByRole('switch', { name: 'Start apps that are not open' })).toBeTruthy();
    expect(screen.getByRole('group', { name: 'Hidden tabs' })).toBeTruthy();
    // Two places: Home and Settings.
    expect(
      within(screen.getByRole('navigation', { name: 'Mosaïk' }))
        .getAllByRole('button')
        .map((b) => b.textContent),
    ).toEqual(['Home', 'Settings']);
  });

  it('opens the zone editor when the quick menu asks', async () => {
    engine.takeZonesToEdit.mockResolvedValueOnce('ship');
    engine.request.mockImplementation((c) => {
      if (c.cmd === 'zones') {
        return Promise.resolve({
          reply: 'zones',
          workspace: 'ship',
          name: 'Ship the release',
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
          zones: [],
          revision: 'r1',
        });
      }
      if (c.cmd === 'workspaces') return Promise.resolve(noWorkspaces);
      if (c.cmd === 'preview_arrange') return Promise.resolve(emptyScreen);
      return Promise.resolve(settingsReply({ ...firstRun, onboarding_done: true }));
    });
    render(<App />);
    expect(await screen.findByRole('heading', { name: /Ship the release/ })).toBeTruthy();
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'zones', workspace: 'ship' });
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
      Promise.resolve(
        c.cmd === 'workspaces'
          ? noWorkspaces
          : c.cmd === 'preview_arrange'
            ? emptyScreen
            : settingsReply({ ...firstRun, onboarding_done: true }),
      ),
    );
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByRole('heading', { name: 'Tidy your screen' })).toBeTruthy();
  });

  it('asks before leaving unsaved zones for another page', async () => {
    engine.request.mockImplementation((c) => {
      if (c.cmd === 'workspaces') return Promise.resolve(ship);
      if (c.cmd === 'preview_arrange') return Promise.resolve(emptyScreen);
      if (c.cmd === 'zones') return Promise.resolve(shipZones);
      return Promise.resolve(settingsReply({ ...firstRun, onboarding_done: true }));
    });
    render(<App />);
    await userEvent.click(await screen.findByRole('button', { name: 'More for “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Edit zones' }));
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
