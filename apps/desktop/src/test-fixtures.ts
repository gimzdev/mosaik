// Replies the tests' stand-in engine sends, built from the shared fixture where it has one
// (crates/mosaik-ipc/fixtures/wire.json, written by the Rust side). Only the tests import this.

import { screen } from '@testing-library/react';

import wire from '../../../crates/mosaik-ipc/fixtures/wire.json';
import {
  type AppSet,
  type ArrangePreview,
  type HotkeyState,
  type Reply,
  type Settings,
  type SideMenuState,
  type WorkspaceApps,
  type WorkspaceSummary,
  type ZoneEdit,
  isReply,
} from './protocol';

const fixture = wire.replies.find((r) => r.reply === 'settings');
if (!isReply(fixture) || fixture.reply !== 'settings') throw new Error('the fixture has no settings reply');

/** The settings of a Mosaïk that was never set up: the engine's defaults. */
export const freshSettings: Settings = fixture.settings;
/** The settings of a Mosaïk set up already. */
export const setUp: Settings = { ...freshSettings, onboarding_done: true, start_with_windows: true };

export const registered: HotkeyState = { shortcut: 'Win+Alt+Space', registered: true, problem: null };

export function settingsReply(
  settings: Settings,
  options: { hotkey?: HotkeyState; sideMenu?: SideMenuState | null; readOnly?: boolean; packaged?: boolean } = {},
): Reply {
  return {
    reply: 'settings',
    settings,
    hotkey: options.hotkey ?? registered,
    side_menu: options.sideMenu === undefined ? { side: 'right', moved: false } : options.sideMenu,
    read_only: options.readOnly ?? false,
    packaged: options.packaged ?? true,
  };
}

export const WORK = { left: 0, top: 0, right: 1920, bottom: 1032 };

/** A screen with an editor and a browser, where Arrange would put them side by side. */
export const coding: ArrangePreview = {
  work_area: WORK,
  zones: [
    { name: '1', color: 'lapis', rect: { left: 0, top: 0, right: 1152, bottom: 1032 } },
    { name: '2', color: 'verdigris', rect: { left: 1152, top: 0, right: 1920, bottom: 1032 } },
  ],
  windows: [
    {
      window: 101,
      label: 'Visual Studio Code',
      icon: 'exe:C:\\Code.exe',
      now: { left: 200, top: 100, right: 1300, bottom: 900 },
      then: { left: 0, top: 0, right: 1152, bottom: 1032 },
      minimized: false,
    },
    {
      window: 102,
      label: 'Google Chrome',
      icon: 'app:Chrome',
      now: { left: 600, top: 200, right: 1800, bottom: 1000 },
      then: { left: 1152, top: 0, right: 1920, bottom: 1032 },
      minimized: false,
    },
  ],
  elsewhere: 0,
  task: 'coding',
};

export const preview = (p: ArrangePreview): Reply => ({ reply: 'preview', ...p });

/** A workspace about to open: the editor goes to zone 1, the terminal starts in zone 2 with
 * Claude as a tab behind it, and the browser, not the workspace's, is minimised. Its zones and
 * apps by their ids, as Mosaïk says them. */
export const opening: ArrangePreview = {
  work_area: WORK,
  zones: [
    { id: 'z1', name: '1', color: 'lapis', rect: { left: 0, top: 0, right: 1152, bottom: 1032 } },
    { id: 'z2', name: '2', color: 'verdigris', rect: { left: 1152, top: 0, right: 1920, bottom: 1032 } },
  ],
  windows: [
    {
      window: 101,
      label: 'Visual Studio Code',
      icon: 'exe:C:\\Code.exe',
      now: { left: 200, top: 100, right: 1300, bottom: 900 },
      then: { left: 0, top: 0, right: 1152, bottom: 1032 },
      minimized: false,
      zone: 'z1',
      app: 's1',
    },
    {
      window: 0,
      label: 'Windows Terminal',
      icon: null,
      now: { left: 1152, top: 0, right: 1920, bottom: 1032 },
      then: { left: 1152, top: 0, right: 1920, bottom: 1032 },
      minimized: false,
      starting: true,
      tabs: [
        { label: 'Windows Terminal', icon: null, shown: true, app: 's2' },
        { label: 'Claude', icon: 'app:Claude', shown: false, app: 's3' },
      ],
      zone: 'z2',
      app: 's2',
    },
    {
      window: 102,
      label: 'Google Chrome',
      icon: 'app:Chrome',
      now: { left: 600, top: 200, right: 1800, bottom: 1000 },
      then: null,
      minimized: true,
    },
  ],
  elsewhere: 0,
  task: null,
};

/** The zones of the workspace `opening` opens, as the zone editor reads them (`zones`): on the
 * main screen, and one on another. */
export const openingZones: ZoneEdit[] = [
  { id: 'z1', name: '1', color: 'lapis', monitor: 'm1', area: { left: 0, top: 0, right: 0.6, bottom: 1 } },
  { id: 'z2', name: '2', color: 'verdigris', monitor: 'm1', area: { left: 0.6, top: 0, right: 1, bottom: 1 } },
  { id: 'z9', name: '3', color: 'saffron', monitor: 'm2', area: { left: 0, top: 0, right: 1, bottom: 1 } },
];

export const zonesReply = (zones: ZoneEdit[], revision = '7'): Reply => ({
  reply: 'zones',
  workspace: 'ship',
  name: 'Ship',
  monitors: [],
  zones,
  revision,
});

export const nothingOpen: ArrangePreview = { work_area: WORK, zones: [], windows: [], elsewhere: 0, task: null };

export function workspace(over: Partial<WorkspaceSummary> & Pick<WorkspaceSummary, 'id' | 'name'>): WorkspaceSummary {
  return {
    windows: 2,
    zones: 2,
    hotkey: null,
    apps: ['Visual Studio Code', 'Google Chrome'],
    icons: ['exe:C:\\Code.exe', 'app:Chrome'],
    layout: [
      { area: { left: 0, top: 0, right: 0.6, bottom: 1 }, color: 'lapis' },
      { area: { left: 0.6, top: 0, right: 1, bottom: 1 }, color: 'verdigris' },
    ],
    ...over,
  };
}

export const workspacesReply = (list: WorkspaceSummary[], active: string | null = null): Reply => ({
  reply: 'workspaces',
  workspaces: list,
  active,
});

export function set(over: Partial<AppSet> & Pick<AppSet, 'id' | 'name' | 'apps'>): AppSet {
  return { icons: over.apps.map(() => null), open: 0, workspace: null, ...over };
}

export const setsReply = (sets: AppSet[]): Reply => ({ reply: 'sets', sets });

export function appsOf(over: Partial<WorkspaceApps> = {}): Reply {
  return {
    reply: 'apps',
    workspace: 'ship',
    name: 'Ship',
    apps: [
      { id: 's1', label: 'Visual Studio Code', icon: 'exe:C:\\Code.exe', zone: 'z1', tab: false },
      { id: 's2', label: 'Google Chrome', icon: 'app:Chrome', zone: 'z2', tab: false },
    ],
    zones: [
      { id: 'z1', name: '1', color: 'lapis' },
      { id: 'z2', name: '2', color: 'verdigris' },
    ],
    windows: [{ window: 0x2_04f2, label: 'Slack', title: 'releases - Slack', icon: null }],
    ...over,
  };
}

/** Words shown on the page, once they are there; not the same words said to screen readers by
 * the announcer, which `said` reads. */
export const shown = (text: string | RegExp) => screen.findByText(text, { ignore: '[role=status], script, style' });

/** What the announcer last said to screen readers. */
export const said = () => screen.getByRole('status').textContent.replace(/\u200b/g, '');

export const done = (message: string): Reply => ({ reply: 'done', message });
export const failed = (message: string, hint: string | null = null): Reply => ({
  reply: 'error',
  code: 'failed',
  message,
  hint,
});
