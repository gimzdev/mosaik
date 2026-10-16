import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { App } from './App';
import type { Command, Notice, Reply, Settings } from './protocol';
import {
  appsOf,
  coding,
  done,
  freshSettings,
  preview,
  setUp,
  settingsReply,
  setsReply,
  workspace,
  workspacesReply,
} from './test-fixtures';

const engine = vi.hoisted(() => ({
  request: vi.fn<(c: Command) => Promise<Reply>>(),
  appIcons: vi.fn((keys: string[]) => Promise.resolve(Object.fromEntries(keys.map((k) => [k, null])))),
  showSwitcher: vi.fn(),
  takeNotices: vi.fn<() => Promise<Notice[]>>(() => Promise.resolve([])),
  onNotices: vi.fn(() => () => undefined),
  onWorkspacesChanged: vi.fn<(callback: () => void) => () => void>(() => () => undefined),
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

/** The engine's "workspaces changed" event, to every part of the window that listens for it. */
let listeners: (() => void)[] = [];

afterEach(cleanup);
beforeEach(() => {
  engine.request.mockReset();
  engine.takeNotices.mockReset();
  engine.takeNotices.mockResolvedValue([]);
  engine.showSwitcher.mockReset();
  delete document.documentElement.dataset.theme;
  localStorage.clear();
  listeners = [];
  engine.onWorkspacesChanged.mockImplementation((callback) => {
    listeners.push(callback);
    return () => {
      listeners = listeners.filter((l) => l !== callback);
    };
  });
});

/** Commands after which the engine says the workspaces changed (crates/mosaik-engine/src/thread.rs). */
const CHANGES: ReadonlySet<Command['cmd']> = new Set(['sort_windows', 'switch', 'save', 'set_zones', 'open_set']);

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
    { id: 'z1', name: '1', color: 'lapis', monitor: 'm1', area: { left: 0, top: 0, right: 0.5, bottom: 1 } },
    { id: 'z2', name: '2', color: 'verdigris', monitor: 'm1', area: { left: 0.5, top: 0, right: 1, bottom: 1 } },
  ],
  revision: '1',
};

/** The engine for a Mosaïk set up already, with one workspace; `extra` answers first. */
function answer(settings: Settings, extra: (c: Command) => Reply | null = () => null) {
  engine.request.mockImplementation((c) => {
    const reply = extra(c) ?? usual(c);
    if (CHANGES.has(c.cmd) && reply.reply !== 'error') {
      setTimeout(() => {
        for (const l of listeners) l();
      }, 0);
    }
    return Promise.resolve(reply);
  });
  function usual(c: Command): Reply {
    switch (c.cmd) {
      case 'settings':
        return settingsReply(settings);
      case 'set_settings':
        return done('Settings saved.');
      case 'workspaces':
        return workspacesReply([workspace({ id: 'ship', name: 'Ship' })]);
      case 'sets':
        return setsReply([]);
      case 'preview_arrange':
        return preview(coding);
      case 'zones':
        return shipZones;
      case 'apps':
        return appsOf();
      case 'hello':
        return { reply: 'hello', app_version: '1.2.0', protocol: 7 };
      default:
        return done('Done.');
    }
  }
}

describe('Mosaïk’s window', () => {
  it('opens on Home, with Settings one click away and back', async () => {
    answer(setUp);
    render(<App />);
    const nav = await screen.findByRole('navigation', { name: 'Mosaïk' });
    expect(within(nav).getByRole('button', { name: 'Home' })).toHaveAttribute('aria-current', 'page');
    expect(await screen.findByRole('heading', { name: 'Your screen' })).toBeInTheDocument();
    await userEvent.click(within(nav).getByRole('button', { name: 'Settings' }));
    expect(await screen.findByRole('heading', { name: 'Settings', level: 1 })).toBeInTheDocument();
    expect(within(nav).getByRole('button', { name: 'Settings' })).toHaveAttribute('aria-current', 'page');
    await userEvent.click(within(nav).getByRole('button', { name: 'Home' }));
    expect(await screen.findByRole('heading', { name: 'Workspaces' })).toBeInTheDocument();
  });

  it('has no tour: the first time, it sorts the windows into workspaces and starts with Windows', async () => {
    const asked: Command[] = [];
    answer(freshSettings, (c) => {
      asked.push(c);
      return null;
    });
    render(<App />);
    expect(await screen.findByRole('heading', { name: 'Your screen' })).toBeInTheDocument();
    await waitFor(() => {
      expect(asked.some((c) => c.cmd === 'set_settings')).toBe(true);
    });
    expect(asked.some((c) => c.cmd === 'sort_windows')).toBe(true);
    const saved = asked.find((c) => c.cmd === 'set_settings');
    expect(saved?.cmd === 'set_settings' && saved.settings.onboarding_done).toBe(true);
    expect(saved?.cmd === 'set_settings' && saved.settings.start_with_windows).toBe(true);
    // Sorted before the settings are saved, so the workspaces it made are there at once.
    expect(asked.findIndex((c) => c.cmd === 'sort_windows')).toBeLessThan(
      asked.findIndex((c) => c.cmd === 'set_settings'),
    );
  });

  it('says which workspaces the first run made of the windows open, with no word of welcome to put away', async () => {
    // The owner, of the welcome Home had: "this shit has to go".
    let sorted = false;
    answer(freshSettings, (c) => {
      if (c.cmd === 'sort_windows') {
        sorted = true;
        return done('Your open windows make 1 workspace: Coding.');
      }
      if (c.cmd === 'workspaces') {
        return workspacesReply([
          workspace({ id: 'ship', name: 'Ship' }),
          ...(sorted ? [workspace({ id: 'coding', name: 'Coding' })] : []),
        ]);
      }
      return null;
    });
    render(<App />);
    const workspaces = await screen.findByRole('region', { name: 'Workspaces' });
    expect(
      await within(workspaces).findByText('Made from the windows you had open. Delete any you don’t need.'),
    ).toBeInTheDocument();
    expect(screen.queryByText(/Mosaïk tidies your windows/)).not.toBeInTheDocument();
    const [only, ...more] = screen.getAllByRole('button', { name: 'Got it' });
    expect(more).toHaveLength(0);
    if (!only) throw new Error('the note of the workspaces made, with its button');
    await userEvent.click(only);
    expect(screen.queryByRole('button', { name: 'Got it' })).not.toBeInTheDocument();
  });

  it('starts with Windows by itself only when installed: a build run from a folder is left as it is', async () => {
    const asked: Command[] = [];
    answer(freshSettings, (c) => {
      asked.push(c);
      return c.cmd === 'settings' ? settingsReply(freshSettings, { packaged: false }) : null;
    });
    render(<App />);
    await waitFor(() => {
      expect(asked.some((c) => c.cmd === 'set_settings')).toBe(true);
    });
    const saved = asked.find((c) => c.cmd === 'set_settings');
    expect(saved?.cmd === 'set_settings' && saved.settings.onboarding_done).toBe(true);
    expect(saved?.cmd === 'set_settings' && saved.settings.start_with_windows).toBe(false);
  });

  it('does nothing of the first run once Mosaïk is set up', async () => {
    const asked: Command[] = [];
    answer(setUp, (c) => {
      asked.push(c);
      return null;
    });
    render(<App />);
    await screen.findByRole('heading', { name: 'Your screen' });
    await waitFor(() => {
      expect(asked.some((c) => c.cmd === 'preview_arrange')).toBe(true);
    });
    expect(asked.some((c) => c.cmd === 'sort_windows' || c.cmd === 'set_settings')).toBe(false);
  });

  it('is dark unless the user chose otherwise', async () => {
    answer(setUp);
    render(<App />);
    await screen.findByRole('heading', { name: 'Your screen' });
    expect(document.documentElement.dataset.theme).toBe('dark');
    cleanup();
    answer({ ...setUp, theme: 'system' });
    render(<App />);
    await screen.findByRole('heading', { name: 'Your screen' });
    expect(document.documentElement.dataset.theme).toBe('system');
  });

  it('keeps Mosaïk’s notes behind a button, never over the page', async () => {
    engine.takeNotices.mockResolvedValue([
      { title: 'FancyZones is running', body: 'It moves windows too.' },
      { title: 'Mosaïk cannot move this window', body: 'Task Manager runs as administrator.' },
    ]);
    answer(setUp);
    render(<App />);
    const notes = await screen.findByRole('button', { name: 'Notes from Mosaïk: 2' });
    expect(screen.queryByText('FancyZones is running')).not.toBeInTheDocument();
    await userEvent.click(notes);
    const list = screen.getByRole('region', { name: 'Notes' });
    expect(within(list).getByText('FancyZones is running')).toBeInTheDocument();
    await userEvent.click(within(list).getByRole('button', { name: 'Dismiss: FancyZones is running' }));
    expect(screen.getByRole('button', { name: 'Notes from Mosaïk: 1' })).toBeInTheDocument();
    await userEvent.click(
      within(screen.getByRole('region', { name: 'Notes' })).getByRole('button', { name: 'Clear all' }),
    );
    expect(screen.queryByRole('button', { name: /Notes from Mosaïk/ })).not.toBeInTheDocument();
  });

  it('opens the switcher from its bar, with the shortcut that really opens it', async () => {
    answer(setUp, (c) =>
      c.cmd === 'settings'
        ? settingsReply(setUp, { hotkey: { shortcut: 'Win+Alt+S', registered: true, problem: 'taken' } })
        : null,
    );
    render(<App />);
    const button = await screen.findByRole('button', { name: 'Switch workspace (Win plus Alt plus S)' });
    expect(button).toHaveTextContent(/^Switch workspace/);
    await userEvent.click(button);
    expect(engine.showSwitcher).toHaveBeenCalledTimes(1);
  });

  it('opens a workspace’s page and its zone editor from its card, and comes back', async () => {
    answer(setUp);
    render(<App />);
    await userEvent.click(await screen.findByRole('button', { name: 'More for “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Edit' }));
    expect(await screen.findByRole('heading', { name: 'Ship', level: 1 })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Edit zones' }));
    expect(await screen.findByRole('heading', { name: 'Ship', level: 1 })).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: /^Zone 1,/ })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Back' }));
    // Back where the editor was opened from: the workspace's page.
    expect(await screen.findByRole('button', { name: 'Rename “Ship”' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Home' }));
    expect(await screen.findByRole('heading', { name: 'Your screen' })).toBeInTheDocument();
  });

  it('opens the zone editor when the side menu asks', async () => {
    let ask: () => void = () => undefined;
    engine.onZonesToEdit.mockImplementation(((callback: () => void) => {
      ask = callback;
      return () => undefined;
    }) as never);
    engine.takeZonesToEdit.mockResolvedValueOnce(null).mockResolvedValueOnce('ship');
    answer(setUp);
    render(<App />);
    await screen.findByRole('heading', { name: 'Your screen' });
    ask();
    expect(await screen.findByRole('button', { name: /^Zone 1,/ })).toBeInTheDocument();
  });

  it('asks before leaving unsaved zones for another page', async () => {
    answer(setUp);
    render(<App />);
    await userEvent.click(await screen.findByRole('button', { name: 'More for “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Edit zones' }));
    await userEvent.click(await screen.findByRole('button', { name: /^Zone 1,/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Split side by side' }));
    await userEvent.click(screen.getByRole('button', { name: 'Settings' }));
    expect(await screen.findByRole('dialog', { name: 'Leave without saving?' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Discard changes' }));
    expect(await screen.findByRole('heading', { name: 'Settings', level: 1 })).toBeInTheDocument();
  });

  it('says plainly when Mosaïk is not answering, and tries again', async () => {
    let tries = 0;
    engine.request.mockImplementation((c) => {
      if (c.cmd === 'settings') {
        tries += 1;
        if (tries === 1) {
          return Promise.resolve({ reply: 'error', code: 'busy', message: 'Mosaïk did not answer.', hint: null });
        }
        return Promise.resolve(settingsReply(setUp));
      }
      if (c.cmd === 'workspaces') return Promise.resolve(workspacesReply([]));
      if (c.cmd === 'sets') return Promise.resolve(setsReply([]));
      if (c.cmd === 'preview_arrange') return Promise.resolve(preview(coding));
      return Promise.resolve(done('Done.'));
    });
    render(<App />);
    expect(await screen.findByRole('heading', { name: 'Mosaïk isn’t answering' })).toBeInTheDocument();
    expect(screen.getByRole('alert')).toHaveTextContent('Mosaïk did not answer.');
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByRole('heading', { name: 'Your screen' })).toBeInTheDocument();
  });
});
