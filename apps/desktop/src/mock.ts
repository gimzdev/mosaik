// A stand-in engine for the development server and the browser smoke tests: the same commands
// and replies as Mosaïk's engine, on sample data kept in memory. Never part of a release build
// (engine.ts only loads it when `import.meta.env.DEV`).
//
// URL switches for trying states: `?first=1` (first run), `?taken=1` (the switcher shortcut is
// taken by another app), `?empty=1` (no workspaces), `?second=1` (a second, portrait monitor),
// `?nowindows=1` (nothing open on screen), `?broken=1` (the engine does not answer), `?moved=1`
// (the side menu is on the other edge), `?newer=1` (settings from a newer Mosaïk), `?notes=1`
// (Mosaïk has notes waiting), `?light=1` (the light theme).

import { PRESETS, type Preset, fromZones, numbered, presetShape, toZones } from './editor/model';
import {
  type AppSet,
  type ArrangeMove,
  type ArrangePreview,
  type Command,
  type HotkeyState,
  type MiniZone,
  type MonitorState,
  type Notice,
  type PreviewWindow,
  type PreviewZone,
  type RatioRect,
  type Rect,
  type Reply,
  type Settings,
  type Task,
  type WorkspaceApp,
  type WorkspaceSummary,
  ZONE_COLORS,
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
  theme: params.get('light') === '1' ? 'light' : 'dark',
  start_with_windows: false,
  onboarding_done: params.get('first') !== '1',
  rules: { user: [], learned: [] },
  notices_shown: [],
};

function hotkey(): HotkeyState {
  return params.get('taken') === '1'
    ? { shortcut: 'Win+Alt+S', registered: true, problem: `${settings.switcher_hotkey} is taken; using Win+Alt+S.` }
    : { shortcut: settings.switcher_hotkey, registered: true, problem: null };
}

/** A workspace as the stand-in keeps it: its apps' names come from its list of apps. */
interface Workspace extends Omit<WorkspaceSummary, 'apps' | 'icons' | 'layout'> {
  zoneList: ZoneEdit[];
  appList: WorkspaceApp[];
}

const workspaces: Workspace[] =
  params.get('empty') === '1'
    ? []
    : [
        {
          id: 'ship',
          name: 'Website',
          windows: 4,
          zones: 3,
          hotkey: 'Win+Alt+J',
          appList: [
            { id: 's1', label: 'Visual Studio Code', icon: null, zone: 'z1', tab: false },
            { id: 's2', label: 'Google Chrome', icon: null, zone: 'z2', tab: false },
            { id: 's3', label: 'Windows Terminal', icon: null, zone: 'z3', tab: true },
            { id: 's4', label: 'Slack', icon: null, zone: 'z3', tab: true },
          ],
          zoneList: [
            { id: 'z1', name: '1', color: 'lapis', monitor: 'm1', area: { left: 0, top: 0, right: 0.6, bottom: 1 } },
            {
              id: 'z2',
              name: '2',
              color: 'verdigris',
              monitor: 'm1',
              area: { left: 0.6, top: 0, right: 1, bottom: 0.5 },
            },
            {
              id: 'z3',
              name: '3',
              color: 'saffron',
              monitor: 'm1',
              area: { left: 0.6, top: 0.5, right: 1, bottom: 1 },
            },
          ],
        },
        {
          // As the owner's own "Coding" (job 0158's picture): two apps and a tab group in one zone,
          // and a zone left empty.
          id: 'checks',
          name: 'Backend',
          windows: 6,
          zones: 3,
          hotkey: null,
          appList: [
            { id: 's1', label: 'Visual Studio Code', icon: null, zone: 'z1', tab: false },
            { id: 's2', label: 'Windows Terminal', icon: null, zone: 'z1', tab: true },
            { id: 's3', label: 'File Explorer', icon: null, zone: 'z1', tab: true },
            { id: 's4', label: 'MSYS2', icon: null, zone: 'z2', tab: false },
            { id: 's5', label: 'Google Chrome', icon: null, zone: 'z2', tab: true },
            { id: 's6', label: 'Claude', icon: null, zone: 'z2', tab: true },
          ],
          zoneList: [
            { id: 'z1', name: '1', color: 'lapis', monitor: 'm1', area: { left: 0, top: 0, right: 0.5, bottom: 1 } },
            {
              id: 'z2',
              name: '2',
              color: 'verdigris',
              monitor: 'm1',
              area: { left: 0.5, top: 0, right: 1, bottom: 0.5 },
            },
            {
              id: 'z3',
              name: '3',
              color: 'saffron',
              monitor: 'm1',
              area: { left: 0.5, top: 0.5, right: 1, bottom: 1 },
            },
          ],
        },
        {
          id: 'brand',
          name: 'Portfolio',
          windows: 2,
          zones: 2,
          hotkey: null,
          appList: [
            { id: 's1', label: 'Figma', icon: null, zone: 'z1', tab: false },
            { id: 's2', label: 'Notepad', icon: null, zone: 'z2', tab: false },
          ],
          zoneList: [
            {
              id: 'z1',
              name: '1',
              color: 'madder',
              monitor: 'm1',
              area: { left: 0, top: 0, right: 0.5, bottom: 1 },
            },
            {
              id: 'z2',
              name: '2',
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
        { id: 'design', name: 'Design', apps: ['Figma', 'Google Chrome', 'File Explorer'] },
        { id: 'playing', name: 'Games', apps: ['Steam', 'Battle.net', 'Discord'] },
      ];

/** A new workspace for these apps, with a zone for each, side by side. */
function madeFor(id: string, name: string, apps: string[]): Workspace {
  const n = apps.length;
  return {
    id,
    name,
    windows: n,
    zones: n,
    hotkey: null,
    appList: apps.map((label, i) => ({
      id: `s${String(i + 1)}`,
      label,
      icon: null,
      zone: `z${String(i + 1)}`,
      tab: false,
    })),
    zoneList: apps.map((_, i) => ({
      id: `z${String(i + 1)}`,
      name: String(i + 1),
      color: ZONE_COLORS[i % ZONE_COLORS.length] ?? 'slate',
      monitor: 'm1',
      area: { left: i / n, top: 0, right: (i + 1) / n, bottom: 1 },
    })),
  };
}

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
    w = madeFor(set.id, set.name, set.apps);
    workspaces.push(w);
  }
  active = w.id;
  return done(`Opened “${w.name}”.`);
}

/** Sorts the windows on screen into workspaces, as the engine's first run does: here the ones for
 * coding (an editor, a terminal, a browser), unless "Coding" exists already. None moves. */
function sortWindows(): Reply {
  const coding = onScreen.filter((s) => s.kind !== 'chat').map((s) => s.label);
  if (find('Coding') || coding.length < 2) return done('No windows open now go together as a workspace.');
  workspaces.push(madeFor('coding', 'Coding', coding));
  return done('Your open windows make 1 workspace: Coding.');
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
  /** Its handle. */
  window: number;
  label: string;
  /** What it is, as `arrange::Kind` says. */
  kind: 'code' | 'browser' | 'terminal' | 'chat';
  now: Rect;
}
let onScreen: OnScreen[] =
  params.get('nowindows') === '1'
    ? []
    : [
        {
          window: 0x30a2,
          label: 'Visual Studio Code',
          kind: 'code',
          now: { left: 180, top: 120, right: 1500, bottom: 980 },
        },
        {
          window: 0x12f8,
          label: 'Google Chrome',
          kind: 'browser',
          now: { left: 900, top: 260, right: 2300, bottom: 1240 },
        },
        {
          window: 0x5c1e,
          label: 'Windows Terminal',
          kind: 'terminal',
          now: { left: 1400, top: 760, right: 2200, bottom: 1300 },
        },
        {
          window: 0x7d04,
          label: 'Slack',
          kind: 'chat',
          now: { left: 300, top: 640, right: 1100, bottom: 1250 },
        },
      ];
/** Where the windows were before the last Arrange, for Undo. */
let beforeArrange: OnScreen[] | null = null;
/** The workspace open before the last Arrange or workspace opened, for Undo. */
let activeBefore: string | null = null;

/** Fractions of the work area in its pixels. */
function px(a: RatioRect, work: Rect): Rect {
  return {
    left: Math.round(work.left + a.left * (work.right - work.left)),
    top: Math.round(work.top + a.top * (work.bottom - work.top)),
    right: Math.round(work.left + a.right * (work.right - work.left)),
    bottom: Math.round(work.top + a.bottom * (work.bottom - work.top)),
  };
}

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

/** What Arrange would do with `preset` (or its own choice), with the windows the user moved and
 * the zones as resized on the picture (`areas`, taken when there is one for each zone), as the
 * engine reports it (`arrange::plan_moved`, `editing::stretch_zones`): a window moved trades
 * places with the window in the zone it was put in, and keeps its zone's colour. */
function preview(preset: Preset | undefined, moves: ArrangeMove[] = [], areas: RatioRect[] = []): ArrangePreview {
  const work = monitors[0]?.work_area ?? { left: 0, top: 0, right: 1920, bottom: 1032 };
  // Arrange makes no zones on a screen without windows; a picked layout always has its own.
  const auto = preset || onScreen.length === 0 ? null : autoLayout();
  const made = preset ? presetShape(preset) : (auto?.shape ?? []);
  const shape = areas.length > 0 && areas.length === made.length ? areas : made;
  const rects = shape.map((a) => px(a, work));
  // A picked layout: the windows used last take the largest zones (as the layout made them); the
  // rest are minimised.
  const order = made
    .map((a) => px(a, work))
    .map((r, i) => ({ i, area: (r.right - r.left) * (r.bottom - r.top) }))
    .sort((a, b) => b.area - a.area || a.i - b.i)
    .map((x) => x.i);
  const zoneOf: (number | undefined)[] = onScreen.map((_, n) => (auto ? auto.takes[n] : order[n]));
  const colorOf = rects.map((_, i) => i);
  for (const m of moves) {
    const n = onScreen.findIndex((s) => s.window === m.window);
    const from = zoneOf[n];
    if (n < 0 || m.zone >= rects.length || from === m.zone) continue;
    const there = zoneOf.indexOf(m.zone);
    zoneOf[n] = m.zone;
    if (from === undefined) {
      if (there >= 0) zoneOf[there] = undefined;
    } else {
      if (there >= 0) zoneOf[there] = from;
      [colorOf[from], colorOf[m.zone]] = [colorOf[m.zone] ?? m.zone, colorOf[from] ?? from];
    }
  }
  const windows: PreviewWindow[] = onScreen.map((s, n): PreviewWindow => {
    const zone = zoneOf[n];
    const rect = zone === undefined ? undefined : rects[zone];
    return rect === undefined || zone === undefined
      ? { window: s.window, label: s.label, icon: null, now: s.now, then: null, minimized: true }
      : {
          window: s.window,
          label: s.label,
          icon: null,
          now: s.now,
          then: rect,
          minimized: false,
          zone: `a${String(zone + 1)}`,
        };
  });
  const numbers = readingNumbers(shape);
  return {
    work_area: work,
    zones: rects.map((rect, i) => ({
      id: `a${String(i + 1)}`,
      name: numbers[i] ?? String(i + 1),
      color: ZONE_COLORS[(colorOf[i] ?? i) % ZONE_COLORS.length] ?? 'slate',
      rect,
    })),
    windows,
    elsewhere: 0,
    task: auto?.task ?? null,
  };
}

/** Each area's number in reading order (row by row from the top, then from the left), as Mosaïk
 * numbers zones. */
function readingNumbers(areas: readonly RatioRect[]): string[] {
  const order = areas
    .map((a, i) => ({ a, i }))
    .sort((x, y) => (Math.abs(x.a.top - y.a.top) < 0.02 ? x.a.left - y.a.left : x.a.top - y.a.top) || x.i - y.i);
  const out: string[] = [];
  order.forEach(({ i }, n) => {
    out[i] = String(n + 1);
  });
  return out;
}

/** Zones numbered as Mosaïk saves them. */
function numberZones(zones: ZoneEdit[]): ZoneEdit[] {
  return toZones(
    numbered(
      fromZones(zones),
      monitors.map((m) => m.id),
    ),
  );
}

/** What opening `w` would do on the main screen, as the engine reports it: its zones; each of its
 * apps from where it is to its zone (an app not on screen starts there), the tabs of a zone as
 * one group, which shows its first tab; an app in no zone in a place of its own (full screen
 * here); the other windows minimised. */
function previewWorkspace(w: Workspace): ArrangePreview {
  const work = monitors[0]?.work_area ?? { left: 0, top: 0, right: 1920, bottom: 1032 };
  const here = w.zoneList.filter((z) => z.monitor === 'm1');
  const zones: PreviewZone[] = here.map((z) => ({
    ...(z.id ? { id: z.id } : {}),
    name: z.name,
    color: z.color,
    rect: px(z.area, work),
  }));
  const openOf = (label: string) => onScreen.find((o) => o.label === label);
  const windows: PreviewWindow[] = [];
  for (const z of here) {
    const rect = px(z.area, work);
    // The zone's apps, in order; its tabs as one group, where the first of them is.
    const units: WorkspaceApp[][] = [];
    let group: WorkspaceApp[] | null = null;
    for (const a of w.appList.filter((x) => x.zone === z.id)) {
      if (!a.tab) {
        units.push([a]);
        continue;
      }
      if (!group) {
        group = [];
        units.push(group);
      }
      group.push(a);
    }
    for (const unit of units) {
      const shown = unit[0];
      if (!shown) continue;
      const open = openOf(shown.label);
      windows.push({
        window: open?.window ?? 0,
        label: shown.label,
        icon: shown.icon,
        now: open?.now ?? rect,
        then: rect,
        minimized: false,
        starting: !open,
        tabs: unit.length > 1 ? unit.map((a) => ({ label: a.label, icon: a.icon, shown: a === shown, app: a.id })) : [],
        ...(z.id ? { zone: z.id } : {}),
        app: shown.id,
      });
    }
  }
  for (const a of w.appList) {
    if (a.zone !== null && here.some((z) => z.id === a.zone)) continue;
    const open = openOf(a.label);
    windows.push({
      window: open?.window ?? 0,
      label: a.label,
      icon: a.icon,
      now: open?.now ?? work,
      then: work,
      minimized: false,
      starting: !open,
      tabs: [],
      app: a.id,
    });
  }
  for (const o of onScreen) {
    if (w.appList.some((a) => a.label === o.label)) continue;
    windows.push({ window: o.window, label: o.label, icon: null, now: o.now, then: null, minimized: true });
  }
  return { work_area: work, zones, windows, elsewhere: 0, task: null };
}

function arrange(preset: string | undefined, moves: ArrangeMove[] = [], areas: RatioRect[] = []): Reply {
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
  const p = preview(known?.id, moves, areas);
  beforeArrange = onScreen;
  activeBefore = active;
  // Minimised windows leave the screen, as they leave the picture.
  onScreen = onScreen.flatMap((s, i) => {
    const w = p.windows[i];
    return w?.minimized ? [] : [{ ...s, now: w?.then ?? s.now }];
  });
  const gone = p.windows.length - onScreen.length;
  const n = onScreen.length;
  // As the engine says it: what moved, and what went to the taskbar. Arrange arranges and Save
  // saves (ADR 0030): no workspace is made or changed, and the one in use stays so.
  return done(
    [
      `Arranged ${String(n)} ${n === 1 ? 'window' : 'windows'}${known ? ` in ${known.label}` : ''}.`,
      gone ? `${gone === 1 ? '1 window' : `${String(gone)} windows`} minimized.` : '',
    ]
      .filter(Boolean)
      .join(' '),
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
    workspaces: workspaces.map(({ id, name, zones, hotkey: key, appList, zoneList }) => ({
      id,
      name,
      windows: appList.length,
      zones,
      hotkey: key,
      apps: appList.map((a) => a.label),
      icons: appList.map((a) => a.icon),
      layout: zoneList.filter((z) => z.monitor === 'm1').map((z): MiniZone => ({ area: z.area, color: z.color })),
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
      .map((s, i) => ({ window: 0x10000 + i, label: s.label, title: `${s.label} window`, icon: null }))
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
      return { reply: 'hello', app_version: '1.0.2-dev', protocol: 10 };
    case 'workspaces':
      return summary();
    case 'settings':
      return {
        reply: 'settings',
        settings: structuredClone(settings),
        hotkey: hotkey(),
        side_menu: settings.edge.enabled
          ? params.get('moved') === '1'
            ? { side: settings.edge.side === 'right' ? 'left' : 'right', moved: true }
            : { side: settings.edge.side, moved: false }
          : null,
        read_only: params.get('newer') === '1',
        // The stand-in is Mosaïk as installed from the Store.
        packaged: true,
      };
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
      // Undone as the engine undoes it: the screen and the workspace open as they were.
      beforeArrange = onScreen;
      activeBefore = active;
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
      w.zoneList = numberZones(
        command.zones.map((z) => {
          if (z.id) return z;
          do {
            n += 1;
          } while (taken.has(`z${String(n)}`));
          taken.add(`z${String(n)}`);
          return { ...z, id: `z${String(n)}` };
        }),
      );
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
      w.appList.push({
        id: `s${String(n)}`,
        label: s.label,
        icon: null,
        zone: command.zone ?? empty,
        tab: false,
      });
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
      if (command.cmd === 'remove_app') {
        w.appList.splice(w.appList.indexOf(app), 1);
        return apps(w);
      }
      // Dropped on another app, the two trade zones (`editing::swap_apps`); else it goes in the
      // zone. A tab leaves its group, unless it stays in the group's zone.
      const other = command.swap_with === undefined ? null : w.appList.find((a) => a.id === command.swap_with);
      if (other === undefined) {
        return {
          reply: 'error',
          code: 'bad_request',
          message: 'This app is not in the workspace any more.',
          hint: 'Look at the list again.',
        };
      }
      if (other && other.zone !== app.zone) {
        const was = app.zone;
        Object.assign(app, { zone: other.zone, tab: false });
        Object.assign(other, { zone: was, tab: false });
      } else if (!other && app.zone !== command.zone) {
        Object.assign(app, { zone: command.zone, tab: false });
      }
      return apps(w);
    }
    case 'rescue':
      return done('Nothing was hidden: every window is already on screen.');
    case 'arrange':
      return arrange(command.preset, command.moves, command.areas);
    case 'preview_workspace': {
      const w = find(command.workspace);
      return w ? { reply: 'preview', ...previewWorkspace(w) } : missing(command.workspace);
    }
    case 'preview_arrange': {
      const known = PRESETS.find((p) => p.id === command.preset);
      if (command.preset !== undefined && !known) return arrange(command.preset);
      return { reply: 'preview', ...preview(known?.id, command.moves, command.areas) };
    }
    case 'undo_arrange':
      if (!beforeArrange) {
        return {
          reply: 'error',
          code: 'not_found',
          message: 'There is nothing to undo.',
          hint: 'Arrange, and opening a workspace, can be undone for ten minutes, until a workspace is saved or edited.',
        };
      }
      onScreen = beforeArrange;
      active = activeBefore;
      beforeArrange = null;
      return done('Windows are back where they were.');
    case 'state':
      return {
        reply: 'state',
        app_version: '1.0.2-dev',
        protocol: 10,
        safe_mode: false,
        watchdog: true,
        monitors,
        hotkey: hotkey(),
      };
    case 'sets':
      return {
        reply: 'sets',
        sets: SETS.map((set): AppSet => ({
          ...set,
          icons: set.apps.map(() => null),
          open: 0,
          workspace: find(set.name)?.id ?? null,
        })),
      };
    case 'open_set':
      return openSet(command.set);
    case 'sort_windows':
      return sortWindows();
    case 'save': {
      const name = command.workspace;
      if (name === null) {
        const w = workspaces.find((x) => x.id === active);
        return w ? done(`Saved “${w.name}”.`) : done('Saved.');
      }
      if (find(name)) return done(`Saved “${name}”.`);
      const n = onScreen.length;
      workspaces.push({
        id: name.toLowerCase().replace(/[^a-z0-9]+/g, '-'),
        name,
        windows: n,
        zones: n,
        hotkey: null,
        appList: onScreen.map((o, i) => ({
          id: `s${String(i + 1)}`,
          label: o.label,
          icon: null,
          zone: `z${String(i + 1)}`,
          tab: false,
        })),
        zoneList: autoLayout().shape.map((area, i, all) => ({
          id: `z${String(i + 1)}`,
          name: readingNumbers(all)[i] ?? String(i + 1),
          color: ZONE_COLORS[i % ZONE_COLORS.length] ?? 'slate',
          monitor: 'm1',
          area,
        })),
      });
      active = workspaces[workspaces.length - 1]?.id ?? active;
      return done(`Saved “${name}”.`);
    }
    case 'show_switcher':
    case 'show_settings':
    case 'quit':
      return done('Done.');
  }
}

/** Notes the stand-in has waiting (`?notes=1`). */
export const mockNotices: Notice[] =
  params.get('notes') === '1'
    ? [
        {
          title: 'FancyZones is running',
          body: 'It also moves windows. Turn it off in PowerToys, or Mosaïk and it will keep moving the same windows.',
        },
        {
          title: 'Mosaïk cannot move this window',
          body: 'Task Manager runs as administrator, so Windows keeps other apps from moving it.',
        },
      ]
    : [];
