// A stand-in engine for the development server and the browser smoke tests: the same commands
// and replies as Mosaïk's engine, on sample data kept in memory. Never part of a release build
// (engine.ts only loads it when `import.meta.env.DEV`).

import type { Command, MonitorState, Reply, Settings, WorkspaceSummary, ZoneEdit } from './protocol';

const monitor: MonitorState = {
  id: 'm1',
  name: 'DELL U2720Q',
  primary: true,
  bounds: { left: 0, top: 0, right: 2560, bottom: 1440 },
  work_area: { left: 0, top: 0, right: 2560, bottom: 1392 },
  dpi: 120,
  scale_percent: 125,
};

const settings: Settings = {
  version: 1,
  switcher_hotkey: 'Win+Alt+Space',
  snap: { enabled: true, invert_shift: false, near: 48 },
  frames: { show_frames: true, show_tags: true, style: { gap: 8, tag_band: 28, border: 2 } },
  tab_hiding: null,
  switching: { minimize_others: true, launch_missing: true },
  theme: 'system',
  start_with_windows: false,
  onboarding_done: new URLSearchParams(window.location.search).get('welcome') !== '1',
  rules: { user: [], learned: [] },
  notices_shown: [],
};

interface Workspace extends WorkspaceSummary {
  zoneList: ZoneEdit[];
}

const workspaces: Workspace[] = [
  {
    id: 'ship',
    name: 'Ship the release',
    windows: 4,
    zones: 3,
    hotkey: 'Win+Alt+1',
    zoneList: [
      { id: 'z1', name: 'Code', color: 'lapis', monitor: 'm1', area: { left: 0, top: 0, right: 0.6, bottom: 1 } },
      { id: 'z2', name: 'Browser', color: 'verdigris', monitor: 'm1', area: { left: 0.6, top: 0, right: 1, bottom: 0.5 } },
      { id: 'z3', name: 'Terminal', color: 'saffron', monitor: 'm1', area: { left: 0.6, top: 0.5, right: 1, bottom: 1 } },
    ],
  },
  {
    id: 'brand',
    name: 'Brand refresh',
    windows: 2,
    zones: 2,
    hotkey: null,
    zoneList: [
      { id: 'z1', name: 'Figma', color: 'madder', monitor: 'm1', area: { left: 0, top: 0, right: 0.5, bottom: 1 } },
      { id: 'z2', name: 'Notes', color: 'slate', monitor: 'm1', area: { left: 0.5, top: 0, right: 1, bottom: 1 } },
    ],
  },
];
let active: string | null = 'ship';

const find = (which: string) => workspaces.find((w) => w.id === which || w.name.toLowerCase() === which.trim().toLowerCase());
const done = (message: string): Reply => ({ reply: 'done', message });
const missing = (which: string): Reply => ({
  reply: 'error',
  code: 'not_found',
  message: `There is no workspace called “${which}”.`,
  hint: null,
});

function summary(): Reply {
  return {
    reply: 'workspaces',
    workspaces: workspaces.map(({ id, name, windows, zones, hotkey }) => ({ id, name, windows, zones, hotkey })),
    active,
  };
}

function checkHotkey(hotkey: string): Reply {
  const h = hotkey.trim();
  const problem = /^Ctrl\+Alt\+[A-Z0-9]$/i.test(h)
    ? 'Ctrl+Alt is AltGr on many keyboards (French, German, Canadian). With a letter, digit or Space it would stop people typing characters. Add the Win key or choose another key.'
    : !/(Win|Ctrl|Alt)\+/.test(h)
      ? 'Add Win, Ctrl or Alt. A single key would be taken away from every app.'
      : null;
  return { reply: 'hotkey', hotkey: h, problem, warnings: [] };
}

export const mock = {
  request(command: Command): Promise<Reply> {
    return Promise.resolve(answer(command));
  },
};

function answer(command: Command): Reply {
  switch (command.cmd) {
    case 'hello':
      return { reply: 'hello', app_version: '1.0.0-dev', protocol: 1 };
    case 'workspaces':
      return summary();
    case 'settings':
      return { reply: 'settings', settings: structuredClone(settings) };
    case 'set_settings':
      Object.assign(settings, structuredClone(command.settings));
      return done('Settings saved.');
    case 'check_hotkey':
      return checkHotkey(command.hotkey);
    case 'switch': {
      const w = find(command.workspace);
      if (!w) return missing(command.workspace);
      active = w.id;
      return done(`Opened “${w.name}”.`);
    }
    case 'zones': {
      const w = find(command.workspace);
      if (!w) return missing(command.workspace);
      return { reply: 'zones', workspace: w.id, name: w.name, monitors: [monitor], zones: structuredClone(w.zoneList) };
    }
    case 'set_zones': {
      const w = find(command.workspace);
      if (!w) return missing(command.workspace);
      let n = 0;
      w.zoneList = command.zones.map((z) => {
        n += 1;
        return { ...z, id: z.id ?? `z${String(10 + n)}` };
      });
      w.zones = w.zoneList.length;
      return done(`Zones of “${w.name}” saved.`);
    }
    case 'rename_workspace': {
      const w = find(command.workspace);
      if (!w) return missing(command.workspace);
      const name = command.name.trim();
      if (!name) return { reply: 'error', code: 'bad_request', message: 'Give it a name.', hint: null };
      w.name = name;
      return done(`Renamed to “${name}”.`);
    }
    case 'delete_workspace': {
      const w = find(command.workspace);
      if (!w) return missing(command.workspace);
      workspaces.splice(workspaces.indexOf(w), 1);
      if (active === w.id) active = null;
      return done(`Deleted “${w.name}”.`);
    }
    case 'set_workspace_hotkey': {
      const w = find(command.workspace);
      if (!w) return missing(command.workspace);
      if (command.hotkey) {
        const check = checkHotkey(command.hotkey);
        if (check.reply === 'hotkey' && check.problem) {
          return { reply: 'error', code: 'bad_request', message: check.problem, hint: 'Pick another shortcut.' };
        }
      }
      w.hotkey = command.hotkey;
      return done(command.hotkey ? `${command.hotkey} now switches to “${w.name}”.` : `“${w.name}” has no shortcut now.`);
    }
    case 'rescue':
      return done('0 window(s) restored');
    case 'state':
      return {
        reply: 'state',
        app_version: '1.0.0-dev',
        protocol: 1,
        safe_mode: false,
        watchdog: true,
        monitors: [monitor],
        hotkey: { shortcut: settings.switcher_hotkey, registered: true, problem: null },
      };
    case 'save':
    case 'show_switcher':
    case 'show_settings':
    case 'quit':
      return done('Done.');
  }
}
