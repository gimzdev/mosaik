import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import wire from '../../../crates/mosaik-ipc/fixtures/wire.json';
import { App } from './App';
import { type Command, type Reply, isReply } from './protocol';

const engine = vi.hoisted(() => ({
  request: vi.fn<(c: Command) => Promise<Reply>>(),
  takeNotices: vi.fn(() => Promise.resolve([])),
  onNotices: vi.fn(() => () => undefined),
  openLink: vi.fn(),
  openLogs: vi.fn(),
}));
vi.mock('./engine', () => engine);

afterEach(cleanup);

// The settings the engine sends on a first run (onboarding not done), from the shared fixture.
const firstRun = wire.replies.find((r) => r.reply === 'settings');

describe('Mosaïk’s window', () => {
  it('welcomes a new user, then remembers it did', async () => {
    if (!isReply(firstRun) || firstRun.reply !== 'settings') throw new Error('fixture has no settings reply');
    let saved: Command | null = null;
    engine.request.mockImplementation((c) => {
      if (c.cmd === 'set_settings') saved = c;
      if (c.cmd === 'workspaces') return Promise.resolve({ reply: 'workspaces', workspaces: [], active: null });
      return Promise.resolve(c.cmd === 'set_settings' ? { reply: 'done', message: 'Settings saved.' } : firstRun);
    });
    render(<App />);
    expect(await screen.findByRole('heading', { name: 'Welcome to Mosaïk' })).toBeTruthy();
    await userEvent.click(screen.getByRole('switch', { name: 'Start Mosaïk with Windows' }));
    await userEvent.click(screen.getByRole('button', { name: 'Get started' }));
    expect(saved).toMatchObject({ cmd: 'set_settings', settings: { onboarding_done: true, start_with_windows: true } });
    expect(await screen.findByRole('heading', { name: 'Workspaces' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Workspaces' }).getAttribute('aria-current')).toBe('page');
  });

  it('says plainly when the engine cannot be reached', async () => {
    engine.request.mockResolvedValue({ reply: 'error', code: 'failed', message: 'Mosaïk’s engine has stopped.', hint: 'Restart Mosaïk.' });
    render(<App />);
    expect((await screen.findByRole('alert')).textContent).toBe('Mosaïk’s engine has stopped. Restart Mosaïk.');
  });
});
