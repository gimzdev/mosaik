// A stand-in engine for the development server and the browser smoke tests: the same commands
// and replies as Mosaïk's engine, on sample data kept in memory. Never part of a release build
// (engine.ts only loads it when `import.meta.env.DEV`).
//
// URL switches for trying states: `?welcome=1` (first run), `?taken=1` (the switcher shortcut is
// taken by another app), `?empty=1` (no workspaces), `?second=1` (a second, portrait monitor),
// `?nowindows=1` (nothing open on screen), `?broken=1` (the engine does not answer).

import { PRESETS, type Preset, presetShape } from './editor/model';
import type {
  ArrangePreview,
  Command,
  HotkeyState,
  MonitorState,
  PreviewWindow,
  Rect,
  Reply,
  Settings,
  WorkspaceSummary,
  ZoneColor,
  ZoneEdit,
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
  version: 4,
  switcher_hotkey: 'Win+Alt+Space',
  snap: { enabled: true, invert_shift: false, near: 48, resize_zones: true },
  frames: {
    show_frames: true,
    show_tags: true,
    tags_in_title_bar: true,
    style: { gap: 4, tag_band: 28, border: 2 },
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

interface Workspace extends WorkspaceSummary {
  zoneList: ZoneEdit[];
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

/** The windows on the main screen, front to back, as `preview_arrange` reports them. */
interface OnScreen {
  label: string;
  /** What Arrange names its zone. */
  zone: string;
  now: Rect;
}
let onScreen: OnScreen[] =
  params.get('nowindows') === '1'
    ? []
    : [
        { label: 'Visual Studio Code', zone: 'Code', now: { left: 180, top: 120, right: 1500, bottom: 980 } },
        { label: 'Google Chrome', zone: 'Browser', now: { left: 900, top: 260, right: 2300, bottom: 1240 } },
        { label: 'Windows Terminal', zone: 'Terminal', now: { left: 1400, top: 760, right: 2200, bottom: 1300 } },
        { label: 'Slack', zone: 'Chat', now: { left: 300, top: 640, right: 1100, bottom: 1250 } },
      ];
/** Where the windows were before the last Arrange, for Undo. */
let beforeArrange: OnScreen[] | null = null;

const ZONE_COLORS: ZoneColor[] = ['lapis', 'verdigris', 'saffron', 'madder', 'slate'];

/** The layout Arrange gives a screen for this many windows (`Preset::for_windows`). */
function autoPreset(windows: number): Preset {
  if (windows <= 1) return 'full';
  if (windows === 2) return 'halves';
  if (windows === 3) return 'half-stack';
  return 'quarters';
}

/** What Arrange would do with `preset` (or its own choice), as the engine reports it. */
function preview(preset: Preset | undefined): ArrangePreview {
  const work = monitors[0]?.work_area ?? { left: 0, top: 0, right: 1920, bottom: 1032 };
  const w = work.right - work.left;
  const h = work.bottom - work.top;
  // Arrange makes no zones on a screen without windows; a picked layout always has its own.
  const shape = preset ? presetShape(preset) : onScreen.length ? presetShape(autoPreset(onScreen.length)) : [];
  const rects = shape.map((a) => ({
    left: Math.round(work.left + a.left * w),
    top: Math.round(work.top + a.top * h),
    right: Math.round(work.left + a.right * w),
    bottom: Math.round(work.top + a.bottom * h),
  }));
  // The windows used last take the largest zones; the rest become tabs of the first.
  const order = rects
    .map((r, i) => ({ i, area: (r.right - r.left) * (r.bottom - r.top) }))
    .sort((a, b) => b.area - a.area || a.i - b.i)
    .map((x) => x.i);
  const inset = (r: Rect): Rect => ({ left: r.left + 2, top: r.top + 2, right: r.right - 2, bottom: r.bottom - 2 });
  const windows: PreviewWindow[] = onScreen.map((s, n) => {
    const zone = order[n];
    if (zone !== undefined) return { label: s.label, now: s.now, then: inset(rects[zone] as Rect), tab: false };
    const first = order[0];
    return { label: s.label, now: s.now, then: first === undefined ? null : inset(rects[first] as Rect), tab: true };
  });
  return {
    work_area: work,
    zones: rects.map((rect, i) => {
      const n = order.indexOf(i);
      return {
        name: onScreen[n]?.zone ?? (i === 0 ? 'Zone' : `Zone ${String(i + 1)}`),
        color: ZONE_COLORS[i % ZONE_COLORS.length] ?? 'slate',
        rect,
      };
    }),
    windows,
    elsewhere: 0,
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
  onScreen = onScreen.map((s, i) => ({ ...s, now: p.windows[i]?.then ?? s.now }));
  const tabs = p.windows.filter((x) => x.tab).length;
  const what = known
    ? `in the “${known.label}” layout`
    : `in ${String(p.zones.length)} ${p.zones.length === 1 ? 'zone' : 'zones'}`;
  const n = onScreen.length;
  return done(
    `Arranged ${String(n)} ${n === 1 ? 'window' : 'windows'} ${what}${
      tabs ? `; ${tabs === 1 ? '1 window as a tab' : `${String(tabs)} windows as tabs`}` : ''
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
    workspaces: workspaces.map(({ id, name, windows, zones, hotkey: key }) => ({
      id,
      name,
      windows,
      zones,
      hotkey: key,
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
    return Promise.resolve(answer(command));
  },
};

function answer(command: Command): Reply {
  switch (command.cmd) {
    case 'hello':
      return { reply: 'hello', app_version: '1.1.0-dev', protocol: 4 };
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
        protocol: 4,
        safe_mode: false,
        watchdog: true,
        monitors,
        hotkey: hotkey(),
      };
    case 'save':
    case 'show_switcher':
    case 'show_settings':
    case 'quit':
      return done('Done.');
  }
}
