'use client';
/* The demo desktops: the interactive one in the hero, the feature illustrations and the workspace thumbnails. */
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { AppBody, AppWindow, EditorApp, ZoneFrame } from '@/components/apps';
import { Icon } from '@/components/ui';
import { DESK_H, DESK_H_TALL, TILES, WORKSPACES, ZONE, deskLayout, slotBox, type Win, type Workspace, type Zone } from '@/lib/data';

/** A 1280 px wide canvas scaled to its container in pure CSS, so it is right from the first paint. */
export const Scaled = ({ tall, className = '', children }: { tall?: boolean; className?: string; children: ReactNode }) => (
  <div className={`desk ${tall ? 'desk-tall' : ''} ${className}`}>
    <div className="desk-in">{children}</div>
  </div>
);

export const TileGlyph = ({ colors, size = 18 }: { colors: string[]; size?: number }) => (
  <svg viewBox="0 0 16 16" width={size} height={size} aria-hidden="true" className="shrink-0">
    <rect x="1" y="1" width="8" height="8" rx="1.6" fill={colors[0]} />
    <rect x="10.5" y="1" width="4.5" height="4.5" rx="1.2" fill={colors[1]} />
    <rect x="10.5" y="7" width="4.5" height="8" rx="1.2" fill={colors[2]} />
    <rect x="1" y="10.5" width="8" height="4.5" rx="1.2" fill={colors[3]} />
  </svg>
);

/** Dark wallpaper: a faint field of tiles with two soft pools of colour. */
const Wallpaper = () => <div className="wallpaper absolute inset-0" />;

/** Mosaïk's bar shows the workspace and its zones; without Mosaïk it is a plain system bar. */
function MenuBar({ ws, plain }: { ws: Workspace; plain: boolean }) {
  return (
    <div className="absolute inset-x-0 top-0 z-[30] h-[42px] bg-black/35 text-[13px] text-white/75 backdrop-blur-[4px]">
      <div className="absolute inset-0 flex items-center justify-between px-5 transition-opacity duration-300" style={{ opacity: plain ? 0 : 1 }}>
        <div className="flex items-center gap-5">
          <span className="flex items-center gap-2 font-semibold text-white">
            <TileGlyph colors={TILES} />
            {ws.name}
          </span>
          <span className="flex items-center gap-4">
            {ws.windows.map((w) => (
              <span key={w.id} className="flex items-center gap-1.5">
                <span className="h-2 w-2 rounded-full" style={{ background: ZONE[w.zone].bg }} />
                {w.tag}
              </span>
            ))}
          </span>
        </div>
        <span>{ws.windows.length} windows · saved</span>
      </div>
      <div className="absolute inset-0 flex items-center justify-between px-5 transition-opacity duration-300" style={{ opacity: plain ? 1 : 0 }}>
        <span className="flex items-center gap-2 text-white/60">
          <span className="h-3 w-3 rounded-[3px] bg-white/35" />
          {ws.windows.length} windows open
        </span>
        <span>10:42</span>
      </div>
    </div>
  );
}

function Taskbar({ visible }: { visible: boolean }) {
  return (
    <div className="absolute inset-x-0 bottom-0 z-[30] flex h-[40px] items-center justify-between bg-[#0B0D12]/90 px-4 transition-opacity duration-300" style={{ opacity: visible ? 1 : 0 }} aria-hidden="true">
      <span className="flex items-center gap-2.5">
        <span className="grid h-[22px] w-[22px] grid-cols-2 gap-[2px]">
          {TILES.map((c) => (
            <span key={c} className="rounded-[2px] bg-white/55" />
          ))}
        </span>
        <span className="ml-3 flex items-center gap-2">
          {['#2B44D9', '#12786A', '#F4B13A', '#C93450'].map((c, i) => (
            <span key={c} className={`h-[22px] w-[34px] rounded-[5px] ${i ? 'bg-white/[0.07]' : 'bg-white/15'}`}>
              <span className="mx-auto mt-[6px] block h-[10px] w-[10px] rounded-[3px]" style={{ background: c }} />
            </span>
          ))}
        </span>
      </span>
      <span className="text-[12px] text-white/60">10:42</span>
    </div>
  );
}

/* ── the interactive desktop ───────────────────────────────────────────── */
type Move = { x: number; y: number; sx?: number; sy?: number };
type Look = { m: Move; mt: Move; o: number; t: string };
const px = (n: number) => `${n}px`;
const EASE = 'cubic-bezier(.22,1,.36,1)';
const EXIT = 'cubic-bezier(.4,0,1,1)';
const HOME: Move = { x: 0, y: 0 };
const nudge = (m: Move, dy: number): Move => ({ ...m, y: m.y + dy });
const reduced = () => typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches;

type Phase = 'enter' | 'idle' | 'out';

/** Both desk heights at once: CSS picks the tall one on phones, so nothing jumps after load. */
function geometry(index: number) {
  const [a, b] = [deskLayout(index, DESK_H), deskLayout(index, DESK_H_TALL)];
  const main = a.main;
  const toFull = (L: typeof a): Move => {
    const r = L.rects[main];
    return { x: L.full.left + L.full.width / 2 - (r.left + r.width / 2), y: L.full.top + L.full.height / 2 - (r.top + r.height / 2), sx: L.full.width / r.width, sy: L.full.height / r.height };
  };
  const inv = (m: Move): Move => ({ x: -m.x, y: -m.y, sx: 1 / (m.sx ?? 1), sy: 1 / (m.sy ?? 1) });
  return { a, b, main, full: toFull(a), fullT: toFull(b), home: inv(toFull(a)), homeT: inv(toFull(b)) };
}

function vars(r: { left: number; top: number; width: number; height: number }, rt: typeof r, look: Look, z: number): CSSProperties {
  return {
    '--l': px(r.left), '--w': px(r.width), '--t': px(r.top), '--h': px(r.height), '--tt': px(rt.top), '--ht': px(rt.height),
    '--x': px(look.m.x), '--y': px(look.m.y), '--yt': px(look.mt.y), '--sx': look.m.sx ?? 1, '--sy': look.m.sy ?? 1, '--syt': look.mt.sy ?? 1,
    opacity: look.o, transition: look.t, zIndex: z,
  } as CSSProperties;
}

export function Stage() {
  const [sel, setSel] = useState(0); // the workspace picked
  const [shown, setShown] = useState(0); // the workspace on screen
  const [ordered, setOrdered] = useState(false);
  const [phase, setPhase] = useState<Phase>('idle');
  const want = useRef(0);
  const intro = useRef<number | undefined>(undefined);
  const stage = useRef<HTMLDivElement>(null);

  // One orchestrated moment: the desktop starts messy, then every window finds its zone.
  useEffect(() => {
    intro.current = window.setTimeout(() => setOrdered(true), reduced() ? 0 : 1400);
    return () => window.clearTimeout(intro.current);
  }, []);

  // New windows mount at their starting pose; flush that style, then let them travel.
  useLayoutEffect(() => {
    if (phase !== 'enter') return;
    void stage.current?.offsetWidth;
    setPhase('idle');
  }, [phase]);

  const choose = useCallback(
    (i: number) => {
      setSel(i);
      want.current = i;
      if (phase === 'out' || (i === shown && phase === 'idle')) return;
      setPhase('out');
      window.setTimeout(() => {
        setShown(want.current);
        setPhase('enter');
      }, reduced() ? 0 : ordered ? 380 : 170);
    },
    [phase, shown, ordered],
  );

  const setState = (next: boolean) => {
    window.clearTimeout(intro.current);
    setOrdered(next);
  };

  const ws = WORKSPACES[shown];
  const g = geometry(shown);
  const topZ = Math.max(...g.a.z);

  const windows = ws.windows.flatMap((w, i) => {
    const st = `${i * (ordered ? 0.06 : 0.05)}s`;
    const pose = g.a.poses[i];
    const poseT = { x: pose.x, y: g.b.poses[i].y };
    const z = g.a.z[i];
    const dim = ordered || z === topZ ? 0 : 0.2;

    if (i !== g.main) {
      const look: Look =
        phase === 'enter'
          ? { m: ordered ? pose : nudge(pose, 26), mt: ordered ? poseT : nudge(poseT, 26), o: 0, t: 'none' }
          : phase === 'out'
            ? ordered
              ? { m: pose, mt: poseT, o: 0, t: `transform .3s ${EXIT} ${i * 0.025}s, opacity .075s linear ${0.225 + i * 0.025}s` }
              : { m: nudge(pose, 10), mt: nudge(poseT, 10), o: 0, t: 'transform .16s ease-in, opacity .16s ease-in' }
            : ordered
              ? { m: HOME, mt: HOME, o: 1, t: `transform .8s ${EASE} ${st}, opacity .1s ease-out ${st}` }
              : { m: pose, mt: poseT, o: 1, t: `transform .8s ${EASE} ${st}, opacity .24s ease-out ${st}` };
      return [
        <div key={`${ws.id}-${w.id}`} className="win" style={vars(g.a.rects[i], g.b.rects[i], look, z)}>
          <div className="relative h-full pt-[28px]">
            <AppWindow win={w} plain={!ordered} />
            <div className="dim inset-x-0 bottom-0 top-[28px] rounded-[10px]" style={{ opacity: dim }} />
          </div>
        </div>,
      ];
    }

    // The main window is the one people keep maximised. It exists at both sizes and the two cross-fade while one
    // travels to the other's rectangle, so its text is never stretched or re-flowed on screen.
    const tiled: Look =
      phase === 'out'
        ? { m: g.full, mt: g.fullT, o: 0, t: `transform .28s ${EXIT}, opacity .28s ${EXIT}` }
        : phase === 'enter' || !ordered
          ? { m: g.full, mt: g.fullT, o: 0, t: phase === 'enter' ? 'none' : `transform .8s ${EASE} ${st}, opacity .22s ease-out` }
          : { m: HOME, mt: HOME, o: 1, t: `transform .8s ${EASE} ${st}, opacity .25s ease-out .18s` };
    const max: Look =
      phase === 'enter'
        ? ordered
          ? { m: g.home, mt: g.homeT, o: 0, t: 'none' }
          : { m: nudge(HOME, 26), mt: nudge(HOME, 26), o: 0, t: 'none' }
        : phase === 'out'
          ? ordered
            ? { m: g.home, mt: g.homeT, o: 0, t: 'opacity .12s' }
            : { m: nudge(HOME, 10), mt: nudge(HOME, 10), o: 0, t: 'transform .16s ease-in, opacity .16s ease-in' }
          : ordered
            ? { m: g.home, mt: g.homeT, o: 0, t: `transform .8s ${EASE} ${st}, opacity .22s ease-out` }
            : { m: HOME, mt: HOME, o: 1, t: `transform .8s ${EASE} ${st}, opacity .25s ease-out .18s` };
    return [
      <div key={`${ws.id}-${w.id}`} className="win pointer-events-none" style={vars(g.a.rects[i], g.b.rects[i], tiled, z)}>
        <div className="h-full pt-[28px]">
          <AppWindow win={w} plain={!ordered} />
        </div>
      </div>,
      <div key={`${ws.id}-${w.id}-max`} className="win pointer-events-none" style={vars(g.a.full, g.b.full, max, z)}>
        <div className="relative h-full">
          <AppWindow win={w} plain flat />
          <div className="dim inset-0" style={{ opacity: dim }} />
        </div>
      </div>,
    ];
  });

  return (
    <div>
      <div ref={stage} className="stage-shadow overflow-hidden rounded-[14px] sm:rounded-[20px]" role="group" aria-label="Interactive demo of a Mosaïk desktop">
        <Scaled tall>
          <Wallpaper />
          <MenuBar ws={ws} plain={!ordered} />
          <Taskbar visible={!ordered} />
          {windows}
        </Scaled>
      </div>

      <div className="mt-5 flex flex-col gap-5 lg:flex-row lg:items-center lg:justify-between">
        <div role="group" aria-label="Workspaces" className="flex flex-wrap gap-2">
          {WORKSPACES.map((w, i) => (
            <button
              key={w.id}
              type="button"
              onClick={() => choose(i)}
              aria-pressed={i === sel}
              className={`flex min-h-[44px] cursor-pointer items-center gap-2.5 rounded-full border px-4 text-[14px] font-semibold transition-colors ${
                i === sel ? 'border-ink bg-ink text-paper' : 'border-ink/20 text-ink hover:border-ink/60'
              }`}
            >
              <TileGlyph colors={w.windows.map((x) => ZONE[x.zone].bg)} size={16} />
              {w.label}
            </button>
          ))}
        </div>
        <div className="inline-flex shrink-0 self-start rounded-full border border-ink/20 p-1 lg:self-auto" role="group" aria-label="Desktop state">
          {[false, true].map((v) => (
            <button
              key={String(v)}
              type="button"
              onClick={() => setState(v)}
              aria-pressed={ordered === v}
              className={`min-h-[38px] cursor-pointer rounded-full px-4 text-sm font-semibold transition-colors ${ordered === v ? 'bg-lapis text-white' : 'text-slate hover:text-ink'}`}
            >
              {v ? 'With Mosaïk' : 'Without Mosaïk'}
            </button>
          ))}
        </div>
      </div>
      <p className="mt-4 max-w-xl text-[15px] leading-relaxed text-slate" aria-live="polite">
        <span className="font-semibold text-ink">{WORKSPACES[sel].name}.</span> {WORKSPACES[sel].summary}
      </p>
    </div>
  );
}

/* ── feature illustrations ─────────────────────────────────────────────── */
const Cursor = ({ x, y }: { x: number; y: number }) => (
  <svg className="absolute z-[60]" style={{ left: x, top: y }} width="44" height="44" viewBox="0 0 24 24" aria-hidden="true">
    <path d="M5 3l14 7-6 2-2 6z" fill="#FFFFFF" stroke="#161B25" strokeWidth="1.4" strokeLinejoin="round" />
  </svg>
);

function ZoneBox({ x, y, w, h, zone, filled }: { x: number; y: number; w: number; h: number; zone: Zone; filled?: boolean }) {
  const c = ZONE[zone].bg;
  return <div className="absolute rounded-[18px]" style={{ left: x, top: y, width: w, height: h, border: `2px ${filled ? 'solid' : 'dashed'} ${c}`, background: `${c}${filled ? '33' : '14'}` }} />;
}

/** A window placed by hand on a still desktop. `tilt` rotates it as if it were being dragged. */
function Placed({ win, x, y, w, h, tilt, z }: { win: Pick<Win, 'kind' | 'variant' | 'tag' | 'zone'>; x: number; y: number; w: number; h: number; tilt?: number; z?: number }) {
  return (
    <div className="absolute" style={{ left: x, top: y, width: w, height: h, transform: tilt ? `rotate(${tilt}deg)` : undefined, zIndex: z }}>
      <ZoneFrame tag={win.tag} zone={win.zone}>
        <AppBody win={win} />
      </ZoneFrame>
    </div>
  );
}

const ART = 'rounded-[20px] bg-ink ring-1 ring-black/10';

/** Zones: named areas. A window dragged near one is pulled into it. */
export function ZonesArt() {
  return (
    <div role="img" aria-label="A desktop divided into three coloured zones. An editor and a terminal already sit in theirs, and a pull request window is being dragged toward the empty Review zone.">
      <Scaled className={ART}>
        <Wallpaper />
        <ZoneBox x={40} y={52} w={720} h={700} zone="lapis" />
        <ZoneBox x={788} y={52} w={452} h={340} zone="verdigris" />
        <ZoneBox x={788} y={410} w={452} h={342} zone="saffron" filled />
        <Placed win={{ kind: 'editor', tag: 'Code', zone: 'lapis' }} x={56} y={96} w={688} h={640} />
        <Placed win={{ kind: 'terminal', tag: 'Terminal', zone: 'verdigris' }} x={804} y={96} w={420} h={280} />
        <p className="absolute text-center text-[18px] font-semibold text-[#F4B13A]" style={{ left: 788, top: 560, width: 452 }}>
          Release to place in Review
        </p>
        <Placed win={{ kind: 'browser', variant: 'pr', tag: 'Review', zone: 'saffron' }} x={836} y={318} w={420} h={300} tilt={-5} z={40} />
        <Cursor x={1070} y={330} />
      </Scaled>
    </div>
  );
}

const TABS: [string, string][] = [['Editor', '#2B44D9'], ['Terminal', '#12786A'], ['Browser', '#F4B13A'], ['Design', '#C93450']];

/** Tabs: any app can join a tab strip, not only a browser. */
export function TabsArt() {
  return (
    <div role="img" aria-label="One window with a tab strip. Editor, terminal, browser and design tools are tabs of the same frame, and a chat window is being dragged toward a new-tab slot.">
      <Scaled className={ART}>
        <Wallpaper />
        <div className="absolute" style={{ left: 36, top: 52, width: 1090, height: 710 }}>
          <ZoneFrame tag="Code" zone="lapis">
            <div className="flex h-full flex-col bg-[#16181D]">
              <div className="flex h-[46px] shrink-0 items-end gap-1.5 bg-[#101216] px-3 text-[14px]">
                {TABS.map(([name, c], i) => (
                  <div key={name} className={`flex h-[38px] items-center gap-2.5 rounded-t-[10px] px-5 ${i ? 'text-[#8E97AA]' : 'bg-[#1E2027] font-semibold text-white'}`} style={i ? undefined : { boxShadow: `inset 0 3px 0 ${c}` }}>
                    <span className="h-3 w-3 rounded-[4px]" style={{ background: c }} />
                    {name}
                  </div>
                ))}
                <div className="mb-1.5 ml-1 flex h-[30px] items-center gap-1.5 rounded-[8px] border-2 border-dashed border-[#F4B13A] px-3 text-[13px] font-semibold text-[#F4B13A]">
                  <Icon n="plus" s={14} w={2} /> Drop to add
                </div>
              </div>
              <div className="min-h-0 flex-1">
                <EditorApp bare />
              </div>
            </div>
          </ZoneFrame>
        </div>
        <Placed win={{ kind: 'chat', variant: 'team', tag: 'Team', zone: 'madder' }} x={880} y={46} w={350} h={236} tilt={5} z={40} />
        <Cursor x={1030} y={58} />
      </Scaled>
    </div>
  );
}

const Key = ({ children, dim }: { children: ReactNode; dim?: boolean }) => (
  <span className={`rounded-[6px] border border-white/15 px-1.5 py-0.5 text-[11.5px] ${dim ? 'text-[#8E97AA]' : ''}`}>{children}</span>
);

/** Saved layouts: a workspace switcher. */
export function SwitcherArt() {
  return (
    <div className="switcher-bg relative overflow-hidden rounded-[20px] bg-ink px-5 py-10 ring-1 ring-black/10 sm:px-10 sm:py-14">
      <div className="mx-auto w-full max-w-[470px] overflow-hidden rounded-[16px] border border-white/10 bg-[#1B1E26] text-[#E6E9F0] shadow-[0_30px_70px_-20px_rgba(0,0,0,0.7)]" role="img" aria-label="A workspace switcher listing four saved workspaces with a preview of each layout. Brand refresh is selected.">
        <div className="flex h-[52px] items-center gap-3 border-b border-white/10 px-4 text-[15px] text-[#8E97AA]">
          <Icon n="search" s={16} />
          <span className="flex-1">Switch workspace</span>
          <Key>Ctrl K</Key>
        </div>
        <div className="p-2">
          {WORKSPACES.map((w, i) => (
            <div key={w.id} className={`relative flex items-center gap-4 rounded-[10px] px-3 py-2.5 ${w.id === 'brand' ? 'bg-[#272B37]' : ''}`}>
              {w.id === 'brand' && <span className="absolute inset-y-2.5 left-0 w-[3px] rounded-full bg-[#7C93FF]" />}
              <div className="relative h-[40px] w-[64px] shrink-0 overflow-hidden rounded-[6px] bg-[#12151B] ring-1 ring-white/10">
                {w.windows.map(({ id, slot: [c, r, cw, rh], zone }) => (
                  <span key={id} className="absolute rounded-[2px]" style={{ left: `${(c / 12) * 100 + 2}%`, top: `${(r / 8) * 100 + 4}%`, width: `${(cw / 12) * 100 - 3}%`, height: `${(rh / 8) * 100 - 6}%`, background: ZONE[zone].bg }} />
                ))}
              </div>
              <div className="min-w-0 flex-1">
                <p className="truncate text-[15px] font-semibold">{w.name}</p>
                <p className="text-[12.5px] text-[#8E97AA]">{w.windows.length} windows · {w.role.toLowerCase()}</p>
              </div>
              <span className="flex gap-1.5">
                <Key dim>Ctrl</Key>
                <Key dim>{i + 1}</Key>
              </span>
            </div>
          ))}
        </div>
        <div className="flex items-center gap-5 border-t border-white/10 px-4 py-2.5 text-[12px] text-[#8E97AA]">
          <span><b className="text-[#C9CEDA]">Enter</b> open</span>
          <span><b className="text-[#C9CEDA]">Ctrl S</b> save current layout</span>
          <span><b className="text-[#C9CEDA]">Esc</b> close</span>
        </div>
      </div>
    </div>
  );
}

/** A whole workspace as a still desktop, for thumbnails. */
export function DeskThumb({ index }: { index: number }) {
  return (
    <Scaled className="bg-ink">
      <Wallpaper />
      {WORKSPACES[index].windows.map((w) => {
        const b = slotBox(w.slot, DESK_H);
        return (
          <div key={w.id} className="absolute" style={{ left: b.left, top: b.top + 8, width: b.width, height: b.height }}>
            <AppWindow win={w} />
          </div>
        );
      })}
    </Scaled>
  );
}
