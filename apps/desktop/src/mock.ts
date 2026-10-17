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
          name: 'Client work',
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
          name: 'Side project',
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
          name: 'Brand design',
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
/** The tab showing in a zone's group (`workspace/zone`), once two of its tabs traded places: it
 * stays the one showing (ADR 0032). Else a group shows its first tab. */
const showing = new Map<string, string>();

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
  // Its zones are on screen now, not Move windows' ones.
  arranged = null;
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
  /** The zone of the last Move windows it is in (in `arranged.zones`), by its place there. */
  zone?: number;
}

/** What the last Move windows put on the main screen, while it is on screen (ADR 0032): the layout
 * it used (a picked one, or the layout of the task), and its zones, each with the colour its
 * windows wear and whether it put a window in it. */
interface Arranged {
  preset: Preset | undefined;
  task: Task | null;
  zones: { area: RatioRect; color: ZoneColor; filled: boolean }[];
}
let arranged: Arranged | null = null;
/** What was on screen before the last Move windows or workspace opened, for Undo. */
let arrangedBefore: Arranged | null = null;
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

/** How much the frame `frame` and `zone` overlap, over the larger of the two, from 0 to 1, as the
 * engine measures it (`arrange::covering`): a window over a whole half does not fill a quarter
 * of it. */
function covering(frame: Rect, zone: Rect): number {
  const w = Math.min(frame.right, zone.right) - Math.max(frame.left, zone.left);
  const h = Math.min(frame.bottom, zone.bottom) - Math.max(frame.top, zone.top);
  const larger = Math.max(
    (frame.right - frame.left) * (frame.bottom - frame.top),
    (zone.right - zone.left) * (zone.bottom - zone.top),
  );
  return w > 0 && h > 0 && larger > 0 ? (w * h) / larger : 0;
}

/** A window is in a zone already when it covers most of it, as the engine finds it (`FILLS`). */
const fills = (frame: Rect, zone: Rect) => covering(frame, zone) >= 0.9;

/** The zones of the last Move windows, when they are the layout again (the engine's step 0, ADR
 * 0032): the same layout asked for, every window still in the zone it was put in, and, for the
 * layout of a task, none of the zones it filled empty. */
function screenAgain(preset: Preset | undefined, work: Rect): Arranged | null {
  const a = arranged;
  if (!a || a.preset !== preset || onScreen.length === 0) return null;
  const inItsZone = onScreen.every((s) => {
    const z = s.zone === undefined ? undefined : a.zones[s.zone];
    return z !== undefined && fills(s.now, px(z.area, work));
  });
  const held = (i: number) => onScreen.some((s) => s.zone === i);
  const kept = preset !== undefined || a.zones.every((z, i) => !z.filled || held(i));
  return inItsZone && kept ? a : null;
}

/** What Arrange would do with `preset` (or its own choice), with the windows the user moved and
 * the zones as resized on the picture (`areas`, taken when there is one for each zone), as the
 * engine reports it (`arrange::plan_moved`, its `Stretched` zones): a window dropped onto another
 * trades places with it, and one dropped onto a zone joins it, behind its windows (ADR 0033); the
 * windows are found in the zones as the layout made them, so a line dragged moves none; a screen
 * Move windows arranged stays as it is, and each zone wears the colour its window wears, which it
 * keeps as windows join or leave it (`arrange::zone_colors`). */
function preview(preset: Preset | undefined, moves: ArrangeMove[] = [], areas: RatioRect[] = []): ArrangePreview {
  const work = monitors[0]?.work_area ?? { left: 0, top: 0, right: 1920, bottom: 1032 };
  const again = screenAgain(preset, work);
  // Arrange makes no zones on a screen without windows; a picked layout always has its own.
  const auto = again || preset || onScreen.length === 0 ? null : autoLayout();
  const made = again ? again.zones.map((z) => z.area) : preset ? presetShape(preset) : (auto?.shape ?? []);
  const shape = areas.length > 0 && areas.length === made.length ? areas : made;
  const rects = shape.map((a) => px(a, work));
  // The zones as the layout made them, where the windows are found: a line dragged on the picture
  // moves the zones, never a window to another zone (ADR 0033).
  const madeRects = made.map((a) => px(a, work));
  // A picked layout: the windows used last take the largest zones (as the layout made them); the
  // rest are minimised.
  const order = made
    .map((a) => px(a, work))
    .map((r, i) => ({ i, area: (r.right - r.left) * (r.bottom - r.top) }))
    .sort((a, b) => b.area - a.area || a.i - b.i)
    .map((x) => x.i);
  const zoneOf: (number | undefined)[] = onScreen.map((s, n) => (again ? s.zone : auto ? auto.takes[n] : order[n]));
  const colorOf = rects.map((_, i) => i);
  // Each zone's own window: the first of its windows; the windows left over stack behind them
  // (ADR 0032, the engine's step 4).
  const own: (number | undefined)[] = rects.map((_, i) => {
    const n = zoneOf.indexOf(i);
    return n >= 0 ? n : undefined;
  });
  const stacked = onScreen.map((_, n) => {
    const z = zoneOf[n];
    return again !== null && z !== undefined && own[z] !== n;
  });
  // Windows already in a zone stay as they are on screen (the engine's step 3): of the windows
  // filling a zone, the one in front is its own window.
  madeRects.forEach((r, i) => {
    if (again) return;
    const f = onScreen.findIndex((s) => fills(s.now, r));
    if (f < 0 || own[i] === f) return;
    const given = own[i];
    const was = zoneOf[f];
    const left = was !== undefined && own[was] === f ? was : undefined;
    zoneOf[f] = i;
    own[i] = f;
    if (left !== undefined) own[left] = undefined;
    if (given !== undefined) {
      zoneOf[given] = left;
      if (left !== undefined) own[left] = given;
    }
  });
  const area = (i: number) => {
    const r = madeRects[i];
    return r ? (r.right - r.left) * (r.bottom - r.top) : 0;
  };
  // The windows left over: each in the zone it fills, else with its kind, else where there are
  // fewest, the larger zone first.
  const stack = () => {
    onScreen.forEach((s, n) => {
      if (zoneOf[n] !== undefined || rects.length === 0) return;
      const count = (i: number) => zoneOf.filter((z) => z === i).length;
      const kin = (i: number) => onScreen.some((o, m) => zoneOf[m] === i && o.kind === s.kind);
      const filled = (i: number) => {
        const r = madeRects[i];
        return r !== undefined && fills(s.now, r);
      };
      const [best] = rects
        .map((_, i) => i)
        .sort(
          (a, b) =>
            Number(filled(b)) - Number(filled(a)) ||
            Number(kin(b)) - Number(kin(a)) ||
            count(a) - count(b) ||
            area(b) - area(a) ||
            a - b,
        );
      zoneOf[n] = best;
      stacked[n] = best !== undefined;
    });
  };
  stack();
  // Front to back: each zone's own window, then those stacked behind it, used last first.
  const seq = onScreen.map((_, n) => n).sort((a, b) => Number(stacked[a]) - Number(stacked[b]));
  const ownOf = (n: number) => {
    const z = zoneOf[n];
    return z !== undefined && own[z] === n ? z : undefined;
  };
  const swapColours = (a: number, b: number) => {
    [colorOf[a], colorOf[b]] = [colorOf[b] ?? b, colorOf[a] ?? a];
  };
  // Windows `a` and `b` trade places: zones, own or stacked, and places front to back.
  const trade = (a: number, b: number) => {
    const [qa, qb, was] = [ownOf(a), ownOf(b), zoneOf[a]];
    zoneOf[a] = zoneOf[b];
    zoneOf[b] = was;
    [stacked[a], stacked[b]] = [stacked[b] ?? false, was !== undefined && (stacked[a] ?? false)];
    if (qb !== undefined) own[qb] = a;
    if (qa !== undefined) own[qa] = b;
    if (qa !== undefined && qb !== undefined && qa !== qb) swapColours(qa, qb);
    const [i, j] = [seq.indexOf(a), seq.indexOf(b)];
    seq[i] = b;
    seq[j] = a;
  };
  // A zone holding stacked windows only has the one in front for its own, as the picture shows it
  // (the engine's `own_the_lone`); then each zone's own window first.
  const ownTheLone = () => {
    rects.forEach((_, i) => {
      if (own[i] !== undefined) return;
      const n = seq.find((x) => zoneOf[x] === i);
      if (n === undefined) return;
      own[i] = n;
      stacked[n] = false;
    });
    seq.sort((a, b) => Number(stacked[a]) - Number(stacked[b]));
  };
  // The windows the user moved (the engine's step 5): dropped onto a window, the two trade
  // places; onto a zone, the moved one joins it, behind its windows, or takes it when it is empty
  // (ADR 0033).
  for (const m of moves) {
    const a = onScreen.findIndex((s) => s.window === m.window);
    if (a < 0 || m.zone >= rects.length) continue;
    const b = m.onto === undefined ? -1 : onScreen.findIndex((s) => s.window === m.onto);
    if (b >= 0 && b !== a && zoneOf[b] === m.zone) {
      trade(a, b);
      ownTheLone();
      continue;
    }
    if (zoneOf[a] === m.zone) continue;
    const qa = ownOf(a);
    if (qa !== undefined) own[qa] = undefined;
    zoneOf[a] = m.zone;
    if (own[m.zone] !== undefined) {
      stacked[a] = true;
      seq.splice(seq.indexOf(a), 1);
      seq.push(a);
    } else {
      if (qa !== undefined) swapColours(qa, m.zone);
      own[m.zone] = a;
      stacked[a] = false;
    }
    ownTheLone();
  }
  stack();
  seq.sort((a, b) => Number(stacked[a]) - Number(stacked[b]));
  // Each zone in the colour its own window wears (then a window stacked in it), which it keeps;
  // else in the plan's, each colour once (`arrange::zone_colors`).
  const worn = (n: number) => {
    const z = onScreen[n]?.zone;
    return z === undefined ? undefined : arranged?.zones[z]?.color;
  };
  const colours: (ZoneColor | undefined)[] = rects.map(() => undefined);
  for (const depth of [0, 1]) {
    const wearing = rects.flatMap((_, i) => {
      const list = seq.filter((n) => zoneOf[n] === i);
      const candidates = depth === 0 ? list.slice(0, 1) : list.slice(1);
      const n = candidates.find((x) => worn(x) !== undefined);
      const c = n === undefined ? undefined : worn(n);
      return n === undefined || c === undefined ? [] : [{ n, i, c }];
    });
    wearing.sort((x, y) => x.n - y.n || x.i - y.i);
    for (const { i, c } of wearing) {
      if (colours[i] === undefined && !colours.includes(c)) colours[i] = c;
    }
  }
  // A zone of the screen as arranged keeps its colour there, as its colour went with the moves.
  rects.forEach((_, i) => {
    const carried = again?.zones[colorOf[i] ?? i]?.color;
    if (colours[i] === undefined && carried !== undefined && !colours.includes(carried)) colours[i] = carried;
  });
  rects.forEach((_, i) => {
    if (colours[i] !== undefined) return;
    const start = colorOf[i] ?? i;
    const free = ZONE_COLORS.map((_, k) => ZONE_COLORS[(start + k) % ZONE_COLORS.length]).find(
      (c) => c !== undefined && !colours.includes(c),
    );
    colours[i] = free ?? ZONE_COLORS[start % ZONE_COLORS.length];
  });
  const windows: PreviewWindow[] = seq.flatMap((n): PreviewWindow[] => {
    const s = onScreen[n];
    if (!s) return [];
    const zone = zoneOf[n];
    const rect = zone === undefined ? undefined : rects[zone];
    return [
      rect === undefined || zone === undefined
        ? { window: s.window, label: s.label, icon: null, now: s.now, then: null, minimized: true }
        : {
            window: s.window,
            label: s.label,
            icon: null,
            now: s.now,
            then: rect,
            minimized: false,
            zone: `a${String(zone + 1)}`,
          },
    ];
  });
  const numbers = readingNumbers(shape);
  // With the user's moves, each zone keeps the colour it had before them, as the moves left the
  // colours: two zones whose own windows traded places traded theirs (`arrange::zone_colors`).
  const before = moves.length > 0 ? preview(preset, [], areas).zones : null;
  return {
    work_area: work,
    zones: rects.map((rect, i) => ({
      id: `a${String(i + 1)}`,
      ...(shape[i] ? { area: shape[i] } : {}),
      name: numbers[i] ?? String(i + 1),
      color: (before ? before[colorOf[i] ?? i]?.color : colours[i]) ?? 'slate',
      rect,
    })),
    windows,
    elsewhere: 0,
    task: again ? again.task : (auto?.task ?? null),
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
    area: z.area,
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
      const shown = unit.find((a) => a.id === showing.get(`${w.id}/${z.id ?? ''}`)) ?? unit[0];
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

/** How much of the work area `a` and `b` share. */
function shared(a: RatioRect, b: RatioRect): number {
  const w = Math.min(a.right, b.right) - Math.max(a.left, b.left);
  const h = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
  return w > 0 && h > 0 ? w * h : 0;
}

/** `w` with its main monitor's zones laid out as `preset`, every app kept in a zone, as the engine
 * does it (`editing::with_layout`, ADR 0033): each new zone takes the old zone over most of it,
 * with its id, colour and apps; the apps of an old zone left over go to the new zone over most of
 * it; a zone left empty takes the app at the back of the zone with the most apps (a zone's tabs
 * move as one group). */
function withLayout(w: Workspace, preset: Preset): Workspace {
  const areas = presetShape(preset);
  const old = w.zoneList.filter((z) => z.monitor === 'm1');
  const pairs = areas
    .flatMap((a, k) => old.map((z, i) => ({ s: shared(a, z.area), k, i })))
    .filter((p) => p.s > 0)
    .sort((x, y) => y.s - x.s || x.k - y.k || x.i - y.i);
  const took: (number | undefined)[] = areas.map(() => undefined);
  const used = new Set<number>();
  for (const { k, i } of pairs) {
    if (took[k] === undefined && !used.has(i)) {
      took[k] = i;
      used.add(i);
    }
  }
  const ids = new Set(w.zoneList.map((z) => z.id));
  const colours = took.flatMap((i) => (i === undefined ? [] : [old[i]?.color]));
  const made: ZoneEdit[] = areas.map((area, k) => {
    const i = took[k];
    const was = i === undefined ? undefined : old[i];
    if (was) return { ...was, area };
    let n = 1;
    while (ids.has(`z${String(n)}`)) n += 1;
    const id = `z${String(n)}`;
    ids.add(id);
    const color = ZONE_COLORS.find((c) => !colours.includes(c)) ?? ZONE_COLORS[colours.length % ZONE_COLORS.length];
    colours.push(color);
    return { id, name: '', color: color ?? 'slate', monitor: 'm1', area };
  });
  const moved = new Map<string, string>();
  old.forEach((z, i) => {
    if (used.has(i) || z.id === null) return;
    const to = made.reduce(
      (best, m, k) => (shared(z.area, m.area) > shared(z.area, made[best]?.area ?? m.area) ? k : best),
      0,
    );
    const target = made[to]?.id;
    if (target) moved.set(z.id, target);
  });
  const appList = w.appList.map((a) => ({ ...a, zone: a.zone === null ? null : (moved.get(a.zone) ?? a.zone) }));
  // A zone left empty takes the app at the back of the zone with the most apps.
  for (;;) {
    const units = (zone: string | null) => {
      const list = appList.filter((a) => a.zone === zone);
      const own = list.filter((a) => !a.tab).map((a) => [a]);
      const tabs = list.filter((a) => a.tab);
      return tabs.length ? [...own, tabs] : own;
    };
    const empty = made.find((z) => units(z.id).length === 0);
    const crowded = made.reduce<ZoneEdit | undefined>(
      (best, z) => (units(z.id).length >= 2 && units(z.id).length > (best ? units(best.id).length : 1) ? z : best),
      undefined,
    );
    if (!empty || !crowded) break;
    const rank = (unit: WorkspaceApp[]) => Math.min(...unit.map((a) => appList.indexOf(a)));
    const back = units(crowded.id).reduce((x, y) => (rank(y) > rank(x) ? y : x));
    for (const a of back) a.zone = empty.id;
  }
  return {
    ...w,
    zones: made.length + w.zoneList.filter((z) => z.monitor !== 'm1').length,
    zoneList: numberZones([...w.zoneList.filter((z) => z.monitor !== 'm1'), ...made]),
    appList,
  };
}

/** The workspace as it was before the last layout picked for it, and as that left it. */
let layoutUndo: { id: string; before: Workspace; after: Workspace } | null = null;

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
      message: 'There are no windows on screen to move.',
      hint: 'Open the apps you work with, then arrange them.',
    };
  }
  const p = preview(known?.id, moves, areas);
  beforeArrange = onScreen;
  activeBefore = active;
  arrangedBefore = arranged;
  // What Move windows put on screen, which stays as it is (ADR 0032): its layout and zones, each
  // in its colour.
  arranged = {
    preset: known?.id,
    task: p.task ?? null,
    zones: p.zones.map((z) => ({
      area: z.area ?? { left: 0, top: 0, right: 1, bottom: 1 },
      color: z.color,
      filled: p.windows.some((w) => w.zone === z.id && !w.minimized),
    })),
  };
  // Minimised windows leave the screen, as they leave the picture; the others are front to back
  // as the picture had them (traded in a zone, ADR 0032), each in its zone.
  const before = onScreen;
  onScreen = p.windows.flatMap((w) => {
    const s = before.find((o) => o.window === w.window);
    if (!s || w.minimized) return [];
    const zone = p.zones.findIndex((z) => z.id === w.zone);
    const next: OnScreen = { window: s.window, label: s.label, kind: s.kind, now: w.then ?? s.now };
    if (zone >= 0) next.zone = zone;
    return [next];
  });
  const gone = p.windows.length - onScreen.length;
  const n = onScreen.length;
  // As the engine says it: what moved, and what went to the taskbar. Arrange arranges and Save
  // saves (ADR 0030): no workspace is made or changed, and the one in use stays so.
  return done(
    [
      `Moved ${String(n)} ${n === 1 ? 'window' : 'windows'}${known ? ` into ${known.label}` : ''}.`,
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

/** The apps of a developer's Start menu, as the stand-in has it (`apps::start_apps`), by id. */
const START_MENU: readonly { app: string; label: string }[] = [
  { app: 'Microsoft.VisualStudioCode', label: 'Visual Studio Code' },
  { app: 'Microsoft.WindowsTerminal_8wekyb3d8bbwe!App', label: 'Windows Terminal' },
  { app: 'Chrome', label: 'Google Chrome' },
  { app: 'com.squirrel.slack.slack', label: 'Slack' },
  { app: 'com.squirrel.AnthropicClaude.claude', label: 'Claude' },
  { app: 'Microsoft.Windows.Explorer', label: 'File Explorer' },
  { app: 'Figma', label: 'Figma' },
  { app: 'Spotify', label: 'Spotify' },
  { app: 'Microsoft.WindowsNotepad_8wekyb3d8bbwe!App', label: 'Notepad' },
];

/** A workspace's apps, the windows on screen it does not have yet, and the apps of the Start
 * menu not open it does not have (by name, here, as the engine finds them by program). */
function apps(w: Workspace): Reply {
  w.windows = w.appList.length;
  const has = (label: string) => w.appList.some((a) => a.label === label);
  return {
    reply: 'apps',
    workspace: w.id,
    name: w.name,
    apps: structuredClone(w.appList),
    zones: w.zoneList.flatMap((z) => (z.id ? [{ id: z.id, name: z.name, color: z.color }] : [])),
    windows: onScreen
      .map((s, i) => ({ window: 0x10000 + i, label: s.label, title: `${s.label} window`, icon: null }))
      .filter((o) => !has(o.label)),
    installed: START_MENU.filter((a) => !has(a.label) && !onScreen.some((s) => s.label === a.label))
      .map((a) => ({ ...a, icon: null }))
      .sort((a, b) => a.label.localeCompare(b.label)),
  };
}

/** The workspace's next app id: s1, s2… */
function nextSlot(w: Workspace): string {
  let n = w.appList.length;
  do {
    n += 1;
  } while (w.appList.some((a) => a.id === `s${String(n)}`));
  return `s${String(n)}`;
}

/** Where an app with no window goes in `w` (`editing::add_installed_app`): the zone asked, else
 * the first empty one, else the one with the fewest apps, the first by number. */
function zoneForNew(w: Workspace, asked: string | null): string | null {
  if (asked !== null) return asked;
  const count = (id: string | null) => w.appList.filter((a) => a.zone === id).length;
  const byNumber = [...w.zoneList].sort((a, b) => Number(a.name) - Number(b.name));
  const empty = byNumber.find((z) => count(z.id) === 0);
  if (empty) return empty.id;
  const [quietest] = [...byNumber].sort((a, b) => count(a.id) - count(b.id));
  return quietest?.id ?? null;
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
  'add_installed_app',
  'set_workspace_layout',
  'undo_workspace_layout',
  'move_app',
  'remove_app',
  'open_set',
  'sort_windows',
]);

function answer(command: Command): Reply {
  switch (command.cmd) {
    case 'hello':
      return { reply: 'hello', app_version: '1.0.2-dev', protocol: 12 };
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
      // Undone as the engine undoes it: the screen and the workspace open as they were. The
      // workspace's zones are on screen now, not Move windows' ones.
      beforeArrange = onScreen;
      activeBefore = active;
      arrangedBefore = arranged;
      arranged = null;
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
      w.appList.push({ id: nextSlot(w), label: s.label, icon: null, zone: command.zone ?? empty, tab: false });
      return apps(w);
    }
    case 'add_installed_app': {
      const w = find(command.workspace);
      if (!w) return missing(command.workspace);
      const a = START_MENU.find((x) => x.app === command.app);
      if (!a) {
        return {
          reply: 'error',
          code: 'bad_request',
          message: 'This app is not in the Start menu any more.',
          hint: 'Pick it again from the list.',
        };
      }
      w.appList.push({ id: nextSlot(w), label: a.label, icon: null, zone: zoneForNew(w, command.zone), tab: false });
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
      // Dropped on another app, the two trade zones (`editing::swap_apps`), or in one zone their
      // places in it (ADR 0032); else it goes in the zone. A tab leaves its group, unless it stays
      // in the group's zone.
      const other = command.swap_with === undefined ? null : w.appList.find((a) => a.id === command.swap_with);
      if (other === undefined) {
        return {
          reply: 'error',
          code: 'bad_request',
          message: 'This app is not in the workspace any more.',
          hint: 'Look at the list again.',
        };
      }
      if (other && other !== app && other.zone !== null && other.zone === app.zone) {
        const key = `${w.id}/${other.zone}`;
        const shown = showing.get(key) ?? w.appList.find((a) => a.zone === app.zone && a.tab)?.id;
        const [i, j] = [w.appList.indexOf(app), w.appList.indexOf(other)];
        w.appList[i] = other;
        w.appList[j] = app;
        const [wasTab, otherTab] = [app.tab, other.tab];
        Object.assign(app, { tab: otherTab });
        Object.assign(other, { tab: wasTab });
        // Two tabs keep the one showing; a tab leaving the group gives the showing to the one
        // taking its place.
        const passed =
          wasTab !== otherTab ? (shown === app.id ? other.id : shown === other.id ? app.id : shown) : shown;
        if (passed !== undefined) showing.set(key, passed);
      } else if (other && other.zone !== app.zone) {
        // In two zones: zones, and places front to back.
        const was = app.zone;
        Object.assign(app, { zone: other.zone, tab: false });
        Object.assign(other, { zone: was, tab: false });
        const [i, j] = [w.appList.indexOf(app), w.appList.indexOf(other)];
        w.appList[i] = other;
        w.appList[j] = app;
      } else if (!other && app.zone !== command.zone) {
        // Behind the apps of the zone (`editing::move_app`, ADR 0034).
        Object.assign(app, { zone: command.zone, tab: false });
        const last = w.appList.reduce((at, a, i) => (a !== app && a.zone === command.zone ? i : at), -1);
        const from = w.appList.indexOf(app);
        if (last > from) {
          w.appList.splice(from, 1);
          w.appList.splice(last, 0, app);
        }
      }
      return apps(w);
    }
    case 'rescue':
      return done('Nothing was hidden: every window is already on screen.');
    case 'arrange':
      return arrange(command.preset, command.moves, command.areas);
    case 'preview_workspace': {
      const w = find(command.workspace);
      if (!w) return missing(command.workspace);
      const known = PRESETS.find((p) => p.id === command.preset);
      if (command.preset !== undefined && !known) return arrange(command.preset);
      return { reply: 'preview', ...previewWorkspace(known ? withLayout(w, known.id) : w) };
    }
    case 'set_workspace_layout': {
      const w = find(command.workspace);
      if (!w) return missing(command.workspace);
      const known = PRESETS.find((p) => p.id === command.preset);
      if (!known) return arrange(command.preset);
      const before = structuredClone(w);
      Object.assign(w, withLayout(w, known.id));
      layoutUndo = { id: w.id, before, after: structuredClone(w) };
      return done(`“${w.name}” is laid out in ${known.label}.`);
    }
    case 'undo_workspace_layout': {
      const w = find(command.workspace);
      if (!w) return missing(command.workspace);
      if (layoutUndo?.id !== w.id || JSON.stringify(w) !== JSON.stringify(layoutUndo.after)) {
        return { reply: 'error', code: 'not_found', message: 'There is nothing to undo.', hint: null };
      }
      Object.assign(w, layoutUndo.before);
      layoutUndo = null;
      return done(`“${w.name}” is back as it was.`);
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
      arranged = arrangedBefore;
      beforeArrange = null;
      arrangedBefore = null;
      return done('Windows are back where they were.');
    case 'state':
      return {
        reply: 'state',
        app_version: '1.0.2-dev',
        protocol: 12,
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
      // The workspace saved is the one on screen now: Move windows' zones are its.
      arranged = null;
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
