export type Zone = 'lapis' | 'verdigris' | 'saffron' | 'madder';
export type Kind = 'editor' | 'terminal' | 'browser' | 'chat' | 'design' | 'notes' | 'paper' | 'chart' | 'stream';
/** [column, row, columns, rows] on a 12 x 8 grid */
export type Slot = [number, number, number, number];
export interface Win { id: string; kind: Kind; variant?: string; tag: string; zone: Zone; slot: Slot }
export interface Workspace { id: string; label: string; name: string; role: string; summary: string; about: string; windows: Win[] }
export interface Rect { left: number; top: number; width: number; height: number }

export const ZONE: Record<Zone, { bg: string; fg: string }> = {
  lapis: { bg: '#2B44D9', fg: '#FFFFFF' },
  verdigris: { bg: '#12786A', fg: '#FFFFFF' },
  saffron: { bg: '#F4B13A', fg: '#161B25' },
  madder: { bg: '#C93450', fg: '#FFFFFF' },
};
export const TILES = ['#2B44D9', '#F4B13A', '#12786A', '#C93450'];

const w = (id: string, kind: Kind, tag: string, zone: Zone, slot: Slot, variant?: string): Win => ({ id, kind, tag, zone, slot, variant });

export const WORKSPACES: Workspace[] = [
  {
    id: 'release',
    label: 'Release',
    name: 'Ship the release',
    role: 'Developers',
    summary: 'Editor, terminal, pull request and team chat, switched as one.',
    about: 'Editor, terminal, pull request and team chat, all in place the moment you pick up a task.',
    windows: [
      w('ed', 'editor', 'Code', 'lapis', [0, 0, 7, 5]),
      w('tm', 'terminal', 'Terminal', 'verdigris', [0, 5, 7, 3]),
      w('pr', 'browser', 'Review', 'saffron', [7, 0, 5, 5], 'pr'),
      w('ch', 'chat', 'Team', 'madder', [7, 5, 5, 3], 'team'),
    ],
  },
  {
    id: 'brand',
    label: 'Brand',
    name: 'Brand refresh',
    role: 'Designers',
    summary: 'Canvas, references, the brief and the client thread, side by side.',
    about: 'The canvas, your references, the brief and the client thread, side by side instead of buried.',
    windows: [
      w('cv', 'design', 'Canvas', 'lapis', [0, 0, 8, 8]),
      w('rf', 'browser', 'References', 'saffron', [8, 0, 4, 3], 'refs'),
      w('br', 'notes', 'Brief', 'verdigris', [8, 3, 4, 2], 'brief'),
      w('cl', 'chat', 'Client', 'madder', [8, 5, 4, 3], 'client'),
    ],
  },
  {
    id: 'thesis',
    label: 'Thesis',
    name: 'Thesis, chapter 3',
    role: 'Researchers',
    summary: 'The paper you are reading, the draft you are writing and the data behind both.',
    about: 'The paper you are reading, the draft you are writing and the data behind both, one switch from the next project.',
    windows: [
      w('pp', 'paper', 'Reading', 'madder', [0, 0, 4, 8]),
      w('dr', 'notes', 'Draft', 'verdigris', [4, 0, 4, 8], 'draft'),
      w('dt', 'chart', 'Data', 'lapis', [8, 0, 4, 4], 'sst'),
      w('sr', 'browser', 'Sources', 'saffron', [8, 4, 4, 4], 'sources'),
    ],
  },
  {
    id: 'stream',
    label: 'Stream',
    name: 'Friday stream',
    role: 'Streamers',
    summary: 'Broadcast software, live chat and your dashboards, ready before you go live.',
    about: 'Broadcast software, live chat and dashboards, arranged before you go live and again for the next show.',
    windows: [
      w('ob', 'stream', 'Broadcast', 'lapis', [0, 0, 8, 5]),
      w('lc', 'chat', 'Chat', 'madder', [8, 0, 4, 8], 'live'),
      w('st', 'chart', 'Stats', 'verdigris', [0, 5, 4, 3], 'viewers'),
      w('db', 'browser', 'Dashboard', 'saffron', [4, 5, 4, 3], 'dash'),
    ],
  },
];

/* Every desktop is drawn on a fixed canvas and scaled to fit, so all sizes are plain pixels. */
export const DESK_W = 1280;
export const DESK_H = 800;
export const DESK_H_TALL = 960; // phones get a taller desktop
export const TAG = 28; // room above a window for its zone tag
export const BAR = 42; // menu bar
const TASKBAR = 40; // system taskbar, shown without Mosaïk
const [TOP, BOTTOM, SIDE, PAD] = [62, 22, 26, 12];

const r1 = (n: number) => Math.round(n * 10) / 10;

export function slotBox([c, r, cols, rows]: Slot, deskH: number): Rect {
  const colW = (DESK_W - SIDE * 2) / 12;
  const rowH = (deskH - TOP - BOTTOM) / 8;
  return { left: r1(SIDE + c * colW + PAD / 2), top: r1(TOP + r * rowH + TAG), width: r1(cols * colW - PAD), height: r1(rows * rowH - TAG - PAD / 2) };
}

/** Where a maximised window sits: everything between the menu bar and the taskbar. */
export const fullRect = (deskH: number): Rect => ({ left: 0, top: BAR, width: DESK_W, height: deskH - BAR - TASKBAR });

function mulberry32(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export interface DeskLayout {
  /** each window's frame, including the strip above it for the zone tag */
  rects: Rect[];
  /** how far each window moves for the messy desktop */
  poses: { x: number; y: number }[];
  z: number[];
  /** the window people keep maximised without Mosaïk: the biggest one */
  main: number;
  full: Rect;
}

/**
 * How most people really work: one window maximised, the rest small windows parked on top of it along the
 * right edge and the bottom, overlapping it and each other a little. Windows keep their real size, so their
 * text never re-flows or re-scales while they move.
 */
export function deskLayout(index: number, deskH: number): DeskLayout {
  const rnd = mulberry32((index + 1) * 7919 + 13);
  const jit = (n: number) => Math.round((rnd() - 0.5) * 2 * n);
  const rects = WORKSPACES[index].windows.map((win) => {
    const b = slotBox(win.slot, deskH);
    return { left: b.left, top: b.top - TAG, width: b.width, height: b.height + TAG };
  });
  const order = rects.map((_, i) => i).sort((a, b) => rects[b].width * rects[b].height - rects[a].width * rects[a].height);
  const placed = rects.map((r) => ({ left: r.left, top: r.top }));
  const maxB = deskH - TASKBAR - 12;
  // Each desktop is messy in its own way. Fractions place a window inside the free range: [across, down].
  const WHERE: Record<string, [number, number]> = {
    tm: [1, 1], pr: [0.5, 0.1], ch: [0.02, 0.8], // developer: terminal bottom right, review mid-screen, chat tucked bottom left
    rf: [0.62, 0.02], br: [0.1, 0.55], cl: [1, 1], // designer: palettes floating around the canvas
    dr: [0, 0.3], dt: [0.88, 0], sr: [0.55, 1], // researcher: tall draft on the left, data up top, sources low
    lc: [1, 0.35], st: [0.03, 1], db: [0.3, 0.18], // streamer: chat down the right, dashboards on the left
  };
  order.slice(1).forEach((i) => {
    const r = rects[i];
    const [fx, fy] = WHERE[WORKSPACES[index].windows[i].id] ?? [1, 0.5];
    placed[i] = {
      left: Math.round(16 + fx * Math.max(DESK_W - r.width - 32, 0)) + (fx > 0 && fx < 1 ? jit(10) : 0),
      top: Math.round(56 + fy * Math.max(maxB - r.height - 56, 0)),
    };
  });
  return {
    rects,
    poses: rects.map((r, i) => ({ x: Math.round(placed[i].left - r.left), y: Math.round(placed[i].top - r.top) })),
    z: rects.map((_, i) => (i === order[0] ? 2 : 3 + order.indexOf(i))),
    main: order[0],
    full: fullRect(deskH),
  };
}
