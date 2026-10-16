import { describe, expect, it } from 'vitest';

import { listDrop } from './AppsList';
import type { WorkspaceApp, WorkspaceApps } from './protocol';

/** The owner's Coding (image of 2026-10-16): Visual Studio Code in zone 1; Claude, File Explorer
 * and Google Chrome in zone 2; MSYS2 and Terminal in zone 3; and an app in no zone. */
const app = (id: string, label: string, zone: string | null): WorkspaceApp => ({
  id,
  label,
  icon: null,
  zone,
  tab: false,
});
const coding: WorkspaceApps = {
  workspace: 'coding',
  name: 'Coding',
  apps: [
    app('s1', 'Visual Studio Code', 'z1'),
    app('s2', 'Claude', 'z2'),
    app('s3', 'File Explorer', 'z2'),
    app('s4', 'Google Chrome', 'z2'),
    app('s5', 'MSYS2 terminal', 'z3'),
    app('s6', 'Terminal', 'z3'),
    app('s7', 'Notepad', null),
  ],
  zones: [
    { id: 'z1', name: '1', color: 'lapis' },
    { id: 'z2', name: '2', color: 'saffron' },
    { id: 'z3', name: '3', color: 'verdigris' },
  ],
  windows: [],
};
const [code, claude, explorer, chrome, msys, terminal, notepad] = coding.apps;
const [z1, z2, z3] = coding.zones;

describe('an app dragged in the list of a workspace’s apps', () => {
  it('trades places with the app it is let go onto, or joins the zone', () => {
    // The owner: "the apps in image 1 must be able to be dragged and droped from zones" (ADR 0033).
    if (!code || !claude || !explorer || !chrome || !msys || !terminal || !notepad || !z1 || !z2 || !z3) {
      throw new Error('the fixture has seven apps and three zones');
    }
    // Onto an app of another zone: the two trade zones.
    expect(listDrop(coding, chrome, { app: msys })).toEqual({
      command: { cmd: 'move_app', workspace: 'coding', app: 's4', zone: 'z3', swap_with: 's5' },
      text: 'Google Chrome and MSYS2 terminal traded zones.',
    });
    // Onto an app of its own zone: their places front to back (Chrome in front of Claude).
    expect(listDrop(coding, chrome, { app: claude })).toEqual({
      command: { cmd: 'move_app', workspace: 'coding', app: 's4', zone: 'z2', swap_with: 's2' },
      text: 'Google Chrome and Claude traded places in zone 2.',
    });
    // Onto a zone: it joins it.
    expect(listDrop(coding, terminal, { zone: z1 })).toEqual({
      command: { cmd: 'move_app', workspace: 'coding', app: 's6', zone: 'z1' },
      text: 'Terminal goes in zone 1.',
    });
    // An app in no zone, onto an app: it goes in that app's zone.
    expect(listDrop(coding, notepad, { app: explorer })).toEqual({
      command: { cmd: 'move_app', workspace: 'coding', app: 's7', zone: 'z2' },
      text: 'Notepad goes in zone 2.',
    });
    // Nothing changes: onto itself, its own zone, or an app in no zone.
    expect(listDrop(coding, code, { app: code })).toBeNull();
    expect(listDrop(coding, code, { zone: z1 })).toBeNull();
    expect(listDrop(coding, code, { app: notepad })).toBeNull();
  });
});
