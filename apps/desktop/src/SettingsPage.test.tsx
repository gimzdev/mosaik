import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Saved, SaveSettings } from './App';
import type { Command, HotkeyState, Reply, Settings, SideMenuState } from './protocol';
import { SettingsPage } from './SettingsPage';
import { registered, said, setUp, shown } from './test-fixtures';
import { Announcer } from './ui';

const engine = vi.hoisted(() => ({
  request: vi.fn<(c: Command) => Promise<Reply>>(),
  openLink: vi.fn<(link: string) => Promise<string | null>>(() => Promise.resolve(null)),
  openLogs: vi.fn<() => Promise<string | null>>(() => Promise.resolve(null)),
  openLicences: vi.fn<() => Promise<string | null>>(() => Promise.resolve(null)),
  rescueWindows: vi.fn(() =>
    Promise.resolve({ text: 'Nothing was hidden: every window is where it was.', error: false }),
  ),
}));
vi.mock('./engine', () => engine);

afterEach(cleanup);
beforeEach(() => {
  engine.request.mockReset();
  engine.request.mockImplementation((c) =>
    Promise.resolve(
      c.cmd === 'hello'
        ? { reply: 'hello', app_version: '1.2.0', protocol: 7 }
        : c.cmd === 'check_hotkey'
          ? { reply: 'hotkey', hotkey: c.hotkey, problem: null, warnings: [] }
          : { reply: 'done', message: 'Done.' },
    ),
  );
  engine.openLink.mockClear();
  engine.rescueWindows.mockClear();
});

/** Saves as the engine would: what was asked, unless `adjust` changes it on the way. */
function saver(adjust: (s: Settings) => Saved | null = () => null) {
  return vi.fn<SaveSettings>((next) =>
    Promise.resolve(adjust(next) ?? { ok: true, message: 'Settings saved.', settings: next }),
  );
}

function page(
  save: ReturnType<typeof saver>,
  options: { settings?: Settings; hotkey?: HotkeyState; sideMenu?: SideMenuState | null; readOnly?: boolean } = {},
) {
  render(
    <Announcer>
      <SettingsPage
        settings={options.settings ?? setUp}
        hotkey={options.hotkey ?? registered}
        sideMenu={options.sideMenu ?? { side: 'right', moved: false }}
        readOnly={options.readOnly ?? false}
        saveSettings={save}
      />
    </Announcer>,
  );
}

const saved = (save: ReturnType<typeof saver>) => save.mock.calls.map(([s]) => s);

describe('Settings', () => {
  it('starts with what a beginner changes first, in that order', () => {
    page(saver());
    const names = screen
      .getAllByRole('switch')
      .map((s) => s.getAttribute('id'))
      .map((id) => document.querySelector(`label[for="${id ?? ''}"]`)?.textContent);
    expect(names).toEqual([
      'Start with Windows',
      'Snap windows to zones',
      'Show zone numbers on windows',
      'Coloured borders',
    ]);
    const general = screen.getByRole('region', { name: 'General' });
    expect(within(general).getByRole('radiogroup', { name: 'Theme' })).toBeInTheDocument();
    expect(within(general).getByText('Shortcut to switch workspace')).toBeInTheDocument();
    expect(within(general).getByText('Shows your workspaces over any app.')).toBeInTheDocument();
    expect(
      within(screen.getByRole('region', { name: 'Side menu' }))
        .getAllByRole('radio')
        .map((r) => r.getAttribute('value')),
    ).toEqual(['right', 'left', 'off']);
  });

  it('saves a switch at once, and says so to screen readers', async () => {
    const save = saver();
    page(save);
    await userEvent.click(screen.getByRole('switch', { name: 'Snap windows to zones' }));
    expect(saved(save)).toContainEqual({ ...setUp, snap: { ...setUp.snap, enabled: !setUp.snap.enabled } });
    await waitFor(() => {
      expect(said()).toBe('Settings saved.');
    });
  });

  it('says where to turn starting with Windows on when Windows keeps it off', async () => {
    const message = 'Windows keeps Mosaïk from starting with it. Turn it on in Task Manager, Startup apps.';
    const save = saver((next) => ({ ok: true, message, settings: { ...next, start_with_windows: false } }));
    page(save, { settings: { ...setUp, start_with_windows: false } });
    await userEvent.click(screen.getByRole('switch', { name: 'Start with Windows' }));
    expect(saved(save)[0]?.start_with_windows).toBe(true);
    await waitFor(() => {
      expect(screen.getAllByRole('alert').map((a) => a.textContent)).toContain(message);
    });
  });

  it('says why a change was refused', async () => {
    const save = saver((next) => ({
      ok: false,
      message: 'Settings could not be saved: the disk is full.',
      settings: next,
    }));
    page(save);
    await userEvent.click(screen.getByRole('switch', { name: 'Coloured borders' }));
    await waitFor(() => {
      expect(screen.getAllByRole('alert').map((a) => a.textContent)).toContain(
        'Settings could not be saved: the disk is full.',
      );
    });
  });

  it('is dark by default, and changes theme in one click', async () => {
    const save = saver();
    page(save);
    const theme = screen.getByRole('radiogroup', { name: 'Theme' });
    expect(within(theme).getByRole('radio', { name: 'Dark' })).toBeChecked();
    await userEvent.click(within(theme).getByRole('radio', { name: 'Light' }));
    expect(saved(save)).toContainEqual({ ...setUp, theme: 'light' });
  });

  it('moves the side menu to an edge, or turns it off', async () => {
    const save = saver();
    page(save);
    const side = screen.getByRole('radiogroup', { name: 'Side menu' });
    expect(within(side).getByRole('radio', { name: 'Right' })).toBeChecked();
    await userEvent.click(within(side).getByRole('radio', { name: 'Left' }));
    expect(saved(save)).toContainEqual({ ...setUp, edge: { ...setUp.edge, enabled: true, side: 'left' } });
    await userEvent.click(within(side).getByRole('radio', { name: 'Off' }));
    expect(saved(save)).toContainEqual({ ...setUp, edge: { ...setUp.edge, enabled: false } });
  });

  it('says why the side menu is on the other edge for now', () => {
    page(saver(), { sideMenu: { side: 'left', moved: true } });
    expect(
      screen.getByText(
        'On the left edge for now: the pointer doesn’t stop at the right edge of your main screen, because another screen is there.',
      ),
    ).toBeInTheDocument();
  });

  it('shows zone numbers on windows or not, in the title bar or above it as chosen under More settings', async () => {
    const save = saver();
    const settings = { ...setUp, frames: { ...setUp.frames, show_tags: true, tags_in_title_bar: true } };
    page(save, { settings });
    const numbers = screen.getByRole('switch', { name: 'Show zone numbers on windows' });
    expect(numbers).toBeChecked();
    expect(numbers).toHaveAccessibleDescription(
      'Each window shows its zone’s number. Drop a window on a number to make tabs.',
    );
    await userEvent.click(numbers);
    expect(saved(save)[0]?.frames).toMatchObject({ show_tags: false, tags_in_title_bar: true });
    await userEvent.click(screen.getByRole('button', { name: 'More settings' }));
    const tags = screen.getByRole('radiogroup', { name: 'Zone tags' });
    expect(
      within(tags)
        .getAllByRole('radio')
        .map((r) => r.getAttribute('value')),
    ).toEqual(['title', 'above']);
    expect(within(tags).getByRole('radio', { name: 'Title bar' })).toBeChecked();
    await userEvent.click(within(tags).getByRole('radio', { name: 'Above' }));
    expect(saved(save)[1]?.frames).toMatchObject({ show_tags: true, tags_in_title_bar: false });
  });

  it('shows the shortcut that switches workspace, and why it could not be used', () => {
    page(saver(), {
      hotkey: { shortcut: 'Win+Alt+Space', registered: false, problem: 'Another app uses Win+Alt+Space.' },
    });
    expect(screen.getByRole('button', { name: 'Add a shortcut: Shortcut to switch workspace' })).toBeInTheDocument();
    expect(screen.getAllByRole('alert').map((a) => a.textContent)).toContain('Another app uses Win+Alt+Space.');
  });

  it('saves a new shortcut to switch workspace', async () => {
    const save = saver();
    page(save);
    await userEvent.click(screen.getByRole('button', { name: 'Change: Shortcut to switch workspace' }));
    await userEvent.keyboard('{Meta>}{Alt>}m{/Alt}{/Meta}');
    await userEvent.click(await screen.findByRole('button', { name: 'Save shortcut' }));
    expect(saved(save)).toContainEqual({ ...setUp, switcher_hotkey: 'Win+Alt+M' });
  });

  it('keeps the rest under More settings, closed until asked', async () => {
    const save = saver();
    page(save);
    const more = screen.getByRole('button', { name: 'More settings' });
    expect(more).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByRole('switch', { name: 'Start a workspace’s closed apps' })).not.toBeInTheDocument();
    expect(screen.queryByRole('radiogroup', { name: 'Holding Shift while dragging' })).not.toBeInTheDocument();
    await userEvent.click(more);
    expect(more).toHaveAttribute('aria-expanded', 'true');
    await userEvent.click(screen.getByRole('switch', { name: 'Start a workspace’s closed apps' }));
    expect(saved(save)[0]?.switching.launch_missing).toBe(!setUp.switching.launch_missing);
    await userEvent.click(
      within(screen.getByRole('radiogroup', { name: 'Tabs not shown' })).getByRole('radio', { name: 'Minimized' }),
    );
    expect(saved(save)[1]?.tab_hiding).toBe('minimize');
    await userEvent.click(
      within(screen.getByRole('radiogroup', { name: 'Holding Shift while dragging' })).getByRole('radio', {
        name: 'Snaps',
      }),
    );
    expect(saved(save)[2]?.snap.invert_shift).toBe(true);
  });

  it('shows settings from a newer Mosaïk without letting them change', () => {
    page(saver(), { readOnly: true });
    expect(
      screen.getByText('These settings were saved by a newer Mosaïk. Update Mosaïk to change them here.'),
    ).toBeInTheDocument();
    expect(screen.getByRole('switch', { name: 'Start with Windows' })).toBeDisabled();
    expect(screen.getByRole('radio', { name: 'Dark' })).toBeDisabled();
  });

  it('ends with About: the version, Patreon, and rescuing windows', async () => {
    page(saver());
    expect(await screen.findByText('Mosaïk 1.2.0')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Support on Patreon' }));
    expect(engine.openLink).toHaveBeenCalledWith('patreon');
    await userEvent.click(screen.getByRole('button', { name: 'Rescue windows' }));
    expect(await shown('Nothing was hidden: every window is where it was.')).toBeInTheDocument();
    expect(said()).toBe('Nothing was hidden: every window is where it was.');
  });

  it('says when a link could not be opened', async () => {
    engine.openLink.mockResolvedValueOnce('Windows has no browser to open the link with.');
    page(saver());
    await userEvent.click(screen.getByRole('button', { name: 'Website' }));
    await waitFor(() => {
      expect(screen.getAllByRole('alert').map((a) => a.textContent)).toContain(
        'Windows has no browser to open the link with.',
      );
    });
  });
});
