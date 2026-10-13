// A stand-in engine for the development server and the browser smoke tests: the same commands
// and replies as Mosaïk's engine, on sample data kept in memory. Never part of a release build
// (engine.ts only loads it when `import.meta.env.DEV`).
//
// URL switches for trying states: `?welcome=1` (first run), `?taken=1` (the switcher shortcut is
// taken by another app), `?empty=1` (no workspaces), `?second=1` (a second, portrait monitor),
// `?nowindows=1` (nothing open on screen), `?broken=1` (the engine does not answer).

import { PRESETS, type Preset, presetShape } from './editor/model';
import {
  type ArrangePreview,
  type Command,
  type HotkeyState,
  type MonitorState,
  type PreviewWindow,
  type RatioRect,
  type Rect,
  type Reply,
  type Settings,
  TASK_WORDS,
  type Task,
  type WorkspaceApp,
  type WorkspaceSummary,
  type ZoneColor,
  type ZoneEdit,
} from './protocol';

const params = new URLSearchParams(window.location.search);

const monitors: MonitorState[] = [
  {
    id: 'm1',
    name: 'DELL U2720Q',
    primary: true,
    bounds: { left: 0, top: 0, right: 2560, bottom: 1440 },
    work_area: { left: 0, top: 0, right: 2560, bottom: 1392 },
    dpi: 120,
    scale_percent: 125,
  },
  ...(params.get('second') === '1'
    ? [
        {
          id: 'm2',
          name: 'Portrait',
          primary: false,
          bounds: { left: 2560, top: -240, right: 3640, bottom: 1680 },
          work_area: { left: 2560, top: -240, right: 3640, bottom: 1680 },
          dpi: 96,
          scale_percent: 100,
        },
      ]
    : []),
];

const settings: Settings = {
  version: 5,
  switcher_hotkey: 'Win+Alt+Space',
  snap: { enabled: true, invert_shift: false, near: 48, resize_zones: true },
  frames: {
    show_frames: false,
    show_tags: true,
    tags_in_title_bar: true,
    style: { gap: 0, tag_band: 28, border: 2 },
  },
  tab_hiding: null,
  switching: { minimize_others: true, launch_missing: true },
  edge: { enabled: true, side: 'right', position: 300 },
  theme: 'system',
  start_with_windows: false,
  onboarding_done: params.get('welcome') !== '1',
  rules: { user: [], learned: [] },
  notices_shown: [],
};

function hotkey(): HotkeyState {
  return params.get('taken') === '1'
    ? { shortcut: 'Win+Alt+S', registered: true, problem: `${settings.switcher_hotkey} is taken; using Win+Alt+S.` }
    : { shortcut: settings.switcher_hotkey, registered: true, problem: null };
}

/** A workspace as the stand-in keeps it: its apps' names come from its list of apps. */
interface Workspace extends Omit<WorkspaceSummary, 'apps'> {
  zoneList: ZoneEdit[];
  appList: WorkspaceApp[];
}

const workspaces: Workspace[] =
  params.get('empty') === '1'
    ? []
    : [
        {
          id: 'ship',
          name: 'Ship the release',
          windows: 4,
          zones: 3,
          hotkey: 'Win+Alt+J',
          appList: [
            { id: 's1', label: 'Visual Studio Code', zone: 'z1', tab: false },
            { id: 's2', label: 'Google Chrome', zone: 'z2', tab: false },
            { id: 's3', label: 'Windows Terminal', zone: 'z3', tab: true },
            { id: 's4', label: 'Slack', zone: 'z3', tab: true },
          ],
          zoneList: [
            { id: 'z1', name: 'Code', color: 'lapis', monitor: 'm1', area: { left: 0, top: 0, right: 0.6, bottom: 1 } },
            {
              id: 'z2',
              name: 'Browser',
              color: 'verdigris',
              monitor: 'm1',
              area: { left: 0.6, top: 0, right: 1, bottom: 0.5 },
            },
            {
              id: 'z3',
              name: 'Terminal',
              color: 'saffron',
              monitor: 'm1',
              area: { left: 0.6, top: 0.5, right: 1, bottom: 1 },
            },
          ],
        },
        {
          id: 'brand',
          name: 'Brand refresh',
          windows: 2,
          zones: 2,
          hotkey: null,
          appList: [
            { id: 's1', label: 'Figma', zone: 'z1', tab: false },
            { id: 's2', label: 'Notepad', zone: 'z2', tab: false },
          ],
          zoneList: [
            {
              id: 'z1',
              name: 'Figma',
              color: 'madder',
              monitor: 'm1',
              area: { left: 0, top: 0, right: 0.5, bottom: 1 },
            },
            {
              id: 'z2',
              name: 'Notes',
              color: 'slate',
              monitor: 'm1',
              area: { left: 0.5, top: 0, right: 1, bottom: 1 },
            },
          ],
        },
      ];
let active: string | null = workspaces[0]?.id ?? null;
let revision = 1;

/** The sets the stand-in offers: the apps of a developer's Start menu that go together. */
const SETS: { id: string; name: string; apps: string[] }[] =
  params.get('empty') === '1'
    ? []
    : [
        {
          id: 'coding',
          name: 'Coding',
          apps: ['Visual Studio Code', 'Terminal', 'Google Chrome', 'Claude', 'File Explorer'],
        },
        { id: 'playing', name: 'Games', apps: ['Steam', 'Battle.net', 'Discord'] },
      ];

/** Opens a set: its workspace, made the first time with a zone per app. */
function openSet(id: string): Reply {
  const set = SETS.find((s) => s.id === id);
  if (!set) {
    return {
      reply: 'error',
      code: 'not_found',
      message: `There is no set called “${id}”.`,
      hint: 'The sets are listed by `mosaikctl sets`.',
    };
  }
  let w = find(set.name);
  if (!w) {
    const n = set.apps.length;
    w = {
      id: set.id,
      name: set.name,
      windows: n,
      zones: n,
      hotkey: null,
      appList: set.apps.map((label, i) => ({ id: `s${String(i + 1)}`, label, zone: `z${String(i + 1)}`, tab: false })),
      zoneList: set.apps.map((label, i) => ({
        id: `z${String(i + 1)}`,
        name: label,
        color: ZONE_COLORS[i % ZONE_COLORS.length] ?? 'slate',
        monitor: 'm1',
        area: { left: i / n, top: 0, right: (i + 1) / n, bottom: 1 },
      })),
    };
    workspaces.push(w);
  }
  active = w.id;
  return done(`Opened “${w.name}”.`);
}

/** Tells the page the workspaces changed, as Mosaïk does with its own event, once the reply
 *  is in. */
function changed() {
  setTimeout(() => {
    window.dispatchEvent(new Event('mosaik:workspaces:changed'));
  }, 0);
}

/** The windows on the main screen, front to back, as `preview_arrange` reports them. */
interface OnScreen {
  label: string;
  /** What it is, as `arrange::Kind` says. */
  kind: 'code' | 'browser' | 'terminal' | 'chat';
  /** What Arrange names its zone. */
  zone: string;
  now: Rect;
}
let onScreen: OnScreen[] =
  params.get('nowindows') === '1'
    ? []
    : [
        {
          label: 'Visual Studio Code',
          kind: 'code',
          zone: 'Code',
          now: { left: 180, top: 120, right: 1500, bottom: 980 },
        },
        {
          label: 'Google Chrome',
          kind: 'browser',
          zone: 'Browser',
          now: { left: 900, top: 260, right: 2300, bottom: 1240 },
        },
        {
          label: 'Windows Terminal',
          kind: 'terminal',
          zone: 'Terminal',
          now: { left: 1400, top: 760, right: 2200, bottom: 1300 },
        },
        { label: 'Slack', kind: 'chat', zone: 'Chat', now: { left: 300, top: 640, right: 1100, bottom: 1250 } },
      ];
/** Where the windows were before the last Arrange, for Undo. */
let beforeArrange: OnScreen[] | null = null;

const ZONE_COLORS: ZoneColor[] = ['lapis', 'verdigris', 'saffron', 'madder', 'slate'];

/** The layout Arrange gives a screen of windows of no particular task (`task::layout`): the
 * window used last takes the most room, up to three sit beside it. */
function autoShape(windows: number): RatioRect[] {
  const share = [1, 0.6, 0.58, 0.55][Math.min(windows, 4) - 1] ?? 1;
  const beside = Math.min(windows, 4) - 1;
  const main = { left: 0, top: 0, right: share, bottom: 1 };
  const rows = beside === 2 ? [0, 0.55, 1] : Array.from({ length: beside + 1 }, (_, k) => k / Math.max(1, beside));
  return [
    main,
    ...Array.from({ length: beside }, (_, k) => ({
      left: share,
      top: rows[k] ?? 0,
      right: 1,
      bottom: rows[k + 1] ?? 1,
    })),
  ];
}

/** The layout Arrange gives the windows on screen (`task::layout`, for these kinds): with an
 * editor it is coding: the editor takes the most room, the terminal goes under it, the others
 * sit beside them; with none, the window used last takes the most room and up to three sit
 * beside it. The zones, and the zone each window (front to back) takes. */
function autoLayout(): { task: Task; shape: RatioRect[]; takes: (number | undefined)[] } {
  const code = onScreen.findIndex((s) => s.kind === 'code');
  if (code < 0) {
    const shape = autoShape(onScreen.length);
    return { task: 'general', shape, takes: onScreen.map((_, n) => (n < shape.length ? n : undefined)) };
  }
  const below = onScreen.findIndex((s) => s.kind === 'terminal');
  const sides = onScreen
    .map((_, n) => n)
    .filter((n) => n !== code && n !== below)
    .slice(0, 3);
  const share = [1, 0.6, 0.58, 0.55][sides.length] ?? 0.55;
  const split = below >= 0 ? 0.68 : 1;
  const shape: RatioRect[] = [{ left: 0, top: 0, right: share, bottom: split }];
  const takes: (number | undefined)[] = onScreen.map(() => undefined);
  takes[code] = 0;
  if (below >= 0) {
    takes[below] = shape.length;
    shape.push({ left: 0, top: split, right: share, bottom: 1 });
  }
  const rows = sides.length === 2 ? [0, 0.55, 1] : Array.from({ length: sides.length + 1 }, (_, k) => k / sides.length);
  sides.forEach((n, k) => {
    takes[n] = shape.length;
    shape.push({ left: share, top: rows[k] ?? 0, right: 1, bottom: rows[k + 1] ?? 1 });
  });
  return { task: 'coding', shape, takes };
}

/** What Arrange would do with `preset` (or its own choice), as the engine reports it. */
function preview(preset: Preset | undefined): ArrangePreview {
  const work = monitors[0]?.work_area ?? { left: 0, top: 0, right: 1920, bottom: 1032 };
  const w = work.right - work.left;
  const h = work.bottom - work.top;
  // Arrange makes no zones on a screen without windows; a picked layout always has its own.
  const auto = preset || onScreen.length === 0 ? null : autoLayout();
  const shape = preset ? presetShape(preset) : (auto?.shape ?? []);
  const rects = shape.map((a) => ({
    left: Math.round(work.left + a.left * w),
    top: Math.round(work.top + a.top * h),
    right: Math.round(work.left + a.right * w),
    bottom: Math.round(work.top + a.bottom * h),
  }));
  // A picked layout: the windows used last take the largest zones; the rest are minimised.
  const order = rects
    .map((r, i) => ({ i, area: (r.right - r.left) * (r.bottom - r.top) }))
    .sort((a, b) => b.area - a.area || a.i - b.i)
    .map((x) => x.i);
  const takes = (n: number) => (auto ? auto.takes[n] : order[n]);
  const windows: PreviewWindow[] = onScreen.map((s, n) => {
    const zone = takes(n);
    const rect = zone === undefined ? undefined : rects[zone];
    return rect === undefined
      ? { label: s.label, now: s.now, then: null, minimized: true }
      : { label: s.label, now: s.now, then: rect, minimized: false };
  });
  return {
    work_area: work,
    zones: rects.map((rect, i) => {
      const n = onScreen.findIndex((_, k) => takes(k) === i);
      return {
        name: onScreen[n]?.zone ?? (i === 0 ? 'Zone' : `Zone ${String(i + 1)}`),
        color: ZONE_COLORS[i % ZONE_COLORS.length] ?? 'slate',
        rect,
      };
    }),
    windows,
    elsewhere: 0,
    task: auto?.task ?? null,
  };
}

function arrange(preset: string | undefined): Reply {
  const known = PRESETS.find((p) => p.id === preset);
  if (preset !== undefined && !known) {
    return {
      reply: 'error',
      code: 'bad_request',
      message: `There is no layout called “${preset}”.`,
      hint: `The layouts are ${PRESETS.map((p) => p.id).join(', ')}.`,
    };
  }
  if (onScreen.length === 0) {
    return {
      reply: 'error',
      code: 'failed',
      message: 'There are no windows on screen to arrange.',
      hint: 'Open the apps you work with, then arrange them.',
    };
  }
  const p = preview(known?.id);
  beforeArrange = onScreen;
  // Minimised windows leave the screen, as they leave the picture.
  onScreen = onScreen.flatMap((s, i) => {
    const w = p.windows[i];
    return w?.minimized ? [] : [{ ...s, now: w?.then ?? s.now }];
  });
  const gone = p.windows.length - onScreen.length;
  const words = p.task ? TASK_WORDS[p.task] : null;
  const what = known
    ? `in the “${known.label}” layout`
    : words
      ? `for ${words}`
      : `in ${String(p.zones.length)} ${p.zones.length === 1 ? 'zone' : 'zones'}`;
  const n = onScreen.length;
  return done(
    `Arranged ${String(n)} ${n === 1 ? 'window' : 'windows'} ${what}${
      gone ? `; ${gone === 1 ? '1 window' : `${String(gone)} windows`} minimized` : ''
    }.`,
  );
}

const find = (which: string) =>
  workspaces.find((w) => w.id === which || w.name.toLowerCase() === which.trim().toLowerCase());
const done = (message: string): Reply => ({ reply: 'done', message });
const missing = (which: string): Reply => ({
  reply: 'error',
  code: 'not_found',
  message: `There is no workspace called “${which}”.`,
  hint: 'It may have been renamed or deleted meanwhile.',
});

function summary(): Reply {
  return {
    reply: 'workspaces',
    workspaces: workspaces.map(({ id, name, zones, hotkey: key, appList }) => ({
      id,
      name,
      windows: appList.length,
      zones,
      hotkey: key,
      apps: appList.map((a) => a.label),
    })),
    active,
  };
}

function zones(w: Workspace): Reply {
  return {
    reply: 'zones',
    workspace: w.id,
    name: w.name,
    monitors,
    zones: structuredClone(w.zoneList),
    revision: String(revision),
  };
}

/** A workspace's apps, and the windows on screen it does not have yet (by name, here). */
function apps(w: Workspace): Reply {
  w.windows = w.appList.length;
  return {
    reply: 'apps',
    workspace: w.id,
    name: w.name,
    apps: structuredClone(w.appList),
    zones: w.zoneList.flatMap((z) => (z.id ? [{ id: z.id, name: z.name, color: z.color }] : [])),
    windows: onScreen
      .map((s, i) => ({ window: 0x10000 + i, label: s.label, title: `${s.label} window` }))
      .filter((o) => !w.appList.some((a) => a.label === o.label)),
  };
}

function checkHotkey(text: string): Reply {
  const h = text.trim();
  const problem = /^Ctrl\+Alt\+[A-Z0-9]$/i.test(h)
    ? 'Ctrl+Alt is AltGr on many keyboards (French, German, Canadian). With a letter, digit or Space it would stop people typing characters. Add the Win key or choose another key.'
    : !/(Win|Ctrl|Alt)\+/.test(h)
      ? 'Add Win, Ctrl or Alt. A single key would be taken away from every app.'
      : null;
  return { reply: 'hotkey', hotkey: h, problem, warnings: [] };
}

export const mock = {
  request(command: Command): Promise<Reply> {
    if (params.get('broken') === '1') {
      return Promise.resolve({
        reply: 'error',
        code: 'busy',
        message: 'Mosaïk did not answer in time.',
        hint: 'Try again in a moment.',
      });
    }
    const reply = answer(command);
    if (CHANGES_WORKSPACES.has(command.cmd) && reply.reply !== 'error') changed();
    return Promise.resolve(reply);
  },
};

/** The commands after which Mosaïk says the workspaces changed (the engine's own list). */
const CHANGES_WORKSPACES: ReadonlySet<Command['cmd']> = new Set([
  'save',
  'switch',
  'set_zones',
  'rename_workspace',
  'delete_workspace',
  'set_workspace_hotkey',
  'arrange',
  'undo_arrange',
  'add_app',
  'move_app',
  'remove_app',
  'open_set',
  'sort_windows',
]);

function answer(command: Command): Reply {
  switch (command.cmd) {
    case 'hello':
      return { reply: 'hello', app_version: '1.1.0-dev', protocol: 6 };
    case 'workspaces':
      return summary();
    case 'settings':
      return { reply: 'settings', settings: structuredClone(settings), hotkey: hotkey() };
    case 'set_settings':
      Object.assign(settings, structuredClone(command.settings));
      return done('Settings saved.');
    case 'check_hotkey':
      return checkHotkey(command.hotkey);
    case 'hold_hotkeys':
      return done(command.hold ? 'Mosaïk’s shortcuts are off while you record one.' : 'Mosaïk’s shortcuts are back.');
    case 'switch': {
      const w = find(command.workspace);
      if (!w) return missing(command.workspace);
      active = w.id;
      return done(`Opened “${w.name}”.`);
    }
    case 'zones': {
      const w = find(command.workspace);
      return w ? zones(w) : missing(command.workspace);
    }
    case 'set_zones': {
      const w = find(command.workspace);
      if (!w) return missing(command.workspace);
      if (command.revision !== String(revision)) {
        return {
          reply: 'error',
          code: 'bad_request',
          message: 'The monitors or these zones changed since the editor opened, so nothing was saved.',
          hint: 'Open the editor again to see them as they are now.',
        };
      }
      const taken = new Set(w.zoneList.map((z) => z.id));
      let n = 0;
      w.zoneList = command.zones.map((z) => {
        if (z.id) return z;
        do {
          n += 1;
        } while (taken.has(`z${String(n)}`));
        taken.add(`z${String(n)}`);
        return { ...z, id: `z${String(n)}` };
      });
      w.zones = w.zoneList.length;
      revision += 1;
      return zones(w);
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
      return done(
        command.hotkey ? `${command.hotkey} now switches to “${w.name}”.` : `“${w.name}” has no shortcut now.`,
      );
    }
    case 'apps': {
      const w = find(command.workspace);
      return w ? apps(w) : missing(command.workspace);
    }
    case 'add_app': {
      const w = find(command.workspace);
      if (!w) return missing(command.workspace);
      const s = onScreen[command.window - 0x10000];
      if (!s) {
        return {
          reply: 'error',
          code: 'bad_request',
          message: 'This window is not on screen any more.',
          hint: 'Pick it again from the list.',
        };
      }
      const empty = w.zoneList.find((z) => !w.appList.some((a) => a.zone === z.id))?.id ?? null;
      let n = w.appList.length;
      do {
        n += 1;
      } while (w.appList.some((a) => a.id === `s${String(n)}`));
      w.appList.push({ id: `s${String(n)}`, label: s.label, zone: command.zone ?? empty, tab: false });
      return apps(w);
    }
    case 'move_app':
    case 'remove_app': {
      const w = find(command.workspace);
      if (!w) return missing(command.workspace);
      const app = w.appList.find((a) => a.id === command.app);
      if (!app) {
        return {
          reply: 'error',
          code: 'bad_request',
          message: 'This app is not in the workspace any more.',
          hint: 'Look at the list again.',
        };
      }
      if (command.cmd === 'remove_app') w.appList.splice(w.appList.indexOf(app), 1);
      else Object.assign(app, { zone: command.zone, tab: false });
      return apps(w);
    }
    case 'rescue':
      return done('Nothing was hidden: every window is already on screen.');
    case 'arrange':
      return arrange(command.preset);
    case 'preview_arrange': {
      const known = PRESETS.find((p) => p.id === command.preset);
      if (command.preset !== undefined && !known) return arrange(command.preset);
      return { reply: 'preview', ...preview(known?.id) };
    }
    case 'undo_arrange':
      if (!beforeArrange) {
        return {
          reply: 'error',
          code: 'not_found',
          message: 'There is no Arrange to undo.',
          hint: 'Arrange can be undone for ten minutes, until a workspace is opened, saved or edited.',
        };
      }
      onScreen = beforeArrange;
      beforeArrange = null;
      return done('Windows are back where they were.');
    case 'state':
      return {
        reply: 'state',
        app_version: '1.1.0-dev',
        protocol: 6,
        safe_mode: false,
        watchdog: true,
        monitors,
        hotkey: hotkey(),
      };
    case 'sets':
      return {
        reply: 'sets',
        sets: SETS.map((set) => ({ ...set, open: 0, workspace: find(set.name)?.id ?? null })),
      };
    case 'open_set':
      return openSet(command.set);
    case 'sort_windows':
      return done('No windows open now go together as a workspace.');
    case 'save':
    case 'show_switcher':
    case 'show_settings':
    case 'quit':
      return done('Done.');
  }
}
