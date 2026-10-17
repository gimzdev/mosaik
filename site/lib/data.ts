export type Zone = 'lapis' | 'verdigris' | 'saffron' | 'madder';
type Kind = 'editor' | 'terminal' | 'browser' | 'chat' | 'design' | 'notes' | 'paper' | 'chart' | 'stream';
/** [column, row, columns, rows] on a 12 x 8 grid */
type Slot = [number, number, number, number];
export interface Win { id: string; kind: Kind; variant?: string; tag: string; zone: Zone; slot: Slot }
export interface Workspace { id: string; label: string; name: string; summary: string; windows: Win[] }
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
    label: 'Code',
    name: 'Web development',
    summary: 'Editor, terminal, pull request and team chat, switched as one.',
    windows: [
      w('ed', 'editor', '1', 'lapis', [0, 0, 7, 5]),
      w('tm', 'terminal', '3', 'verdigris', [0, 5, 7, 3]),
      w('pr', 'browser', '2', 'saffron', [7, 0, 5, 5], 'pr'),
      w('ch', 'chat', '4', 'madder', [7, 5, 5, 3], 'team'),
    ],
  },
  {
    id: 'brand',
    label: 'Brand',
    name: 'Brand refresh',
    summary: 'Canvas, references, the brief and the client thread, side by side.',
    windows: [
      w('cv', 'design', '1', 'lapis', [0, 0, 8, 8]),
      w('rf', 'browser', '2', 'saffron', [8, 0, 4, 3], 'refs'),
      w('br', 'notes', '3', 'verdigris', [8, 3, 4, 2], 'brief'),
      w('cl', 'chat', '4', 'madder', [8, 5, 4, 3], 'client'),
    ],
  },
  {
    id: 'thesis',
    label: 'Thesis',
    name: 'Thesis, chapter 3',
    summary: 'The paper you are reading, the draft you are writing and the data behind both.',
    windows: [
      w('pp', 'paper', '1', 'madder', [0, 0, 4, 8]),
      w('dr', 'notes', '2', 'verdigris', [4, 0, 4, 8], 'draft'),
      w('dt', 'chart', '3', 'lapis', [8, 0, 4, 4], 'sst'),
      w('sr', 'browser', '4', 'saffron', [8, 4, 4, 4], 'sources'),
    ],
  },
  {
    id: 'stream',
    label: 'Stream',
    name: 'Friday stream',
    summary: 'Broadcast software, live chat and your dashboards, ready before you go live.',
    windows: [
      w('ob', 'stream', '1', 'lapis', [0, 0, 8, 5]),
      w('lc', 'chat', '2', 'madder', [8, 0, 4, 8], 'live'),
      w('st', 'chart', '3', 'verdigris', [0, 5, 4, 3], 'viewers'),
      w('db', 'browser', '4', 'saffron', [4, 5, 4, 3], 'dash'),
    ],
  },
];

/* Every desktop is drawn on a fixed canvas and scaled to fit, so all sizes are plain pixels. */
const DESK_W = 1280;
export const DESK_H = 800;
export const DESK_H_TALL = 960; // phones get a taller desktop
const TAG = 28; // room above a window for its zone tag
const TASKBAR = 40; // the Windows taskbar, along the bottom
const [TOP, BOTTOM, SIDE, PAD] = [22, TASKBAR + 14, 26, 12];

/** The shortcut that opens Mosaïk's switcher (the app's default). */
export const SWITCHER_KEYS = ['Win', 'Alt', 'Space'];

const r1 = (n: number) => Math.round(n * 10) / 10;

function slotBox([c, r, cols, rows]: Slot, deskH: number): Rect {
  const colW = (DESK_W - SIDE * 2) / 12;
  const rowH = (deskH - TOP - BOTTOM) / 8;
  return { left: r1(SIDE + c * colW + PAD / 2), top: r1(TOP + r * rowH + TAG), width: r1(cols * colW - PAD), height: r1(rows * rowH - TAG - PAD / 2) };
}

/** Where a maximised window sits: everything above the taskbar. */
const fullRect = (deskH: number): Rect => ({ left: 0, top: 0, width: DESK_W, height: deskH - TASKBAR });

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

interface DeskLayout {
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
      top: Math.round(TOP + fy * Math.max(maxB - r.height - TOP, 0)),
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
