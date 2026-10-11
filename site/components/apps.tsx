/* The pretend apps drawn inside the demo desktops. Plain markup, sized in pixels on a 1280 px canvas. */
import { memo, useId, type ReactNode } from 'react';
import { TILES, ZONE, type Win } from '@/lib/data';
import { Icon, Wordmark } from '@/components/ui';

/* ── shared chrome ─────────────────────────────────────────────────────── */
const Controls = ({ className = '' }: { className?: string }) => (
  <div className="flex h-full">
    {['min', 'max', 'close'].map((n) => (
      <span key={n} className={`grid w-[44px] place-items-center ${className}`}>
        <Icon n={n} s={11} w={1.2} />
      </span>
    ))}
  </div>
);

function TitleBar({ title, dark, glyph = '#7C93FF' }: { title: string; dark?: boolean; glyph?: string }) {
  return (
    <div className={`flex h-[34px] shrink-0 select-none items-center justify-between text-[12px] ${dark ? 'bg-[#101216] text-[#8E97AA]' : 'bg-[#E9ECF1] text-[#4A5262]'}`}>
      <div className="flex min-w-0 items-center gap-2 pl-3">
        <span className="h-[13px] w-[13px] shrink-0 rounded-[4px]" style={{ background: glyph }} />
        <span className="truncate">{title}</span>
      </div>
      <Controls />
    </div>
  );
}

const Avatar = ({ name, color, size = 28 }: { name: string; color: string; size?: number }) => (
  <span className="grid shrink-0 place-items-center rounded-full font-semibold text-white" style={{ width: size, height: size, background: color, fontSize: size * 0.4 }}>
    {name[0]}
  </span>
);

/** A unique id that is safe inside url(#…) references. */
const useSvgId = () => `g${useId().replace(/[^\w-]/g, '')}`;

/* ── code editor and terminal ──────────────────────────────────────────── */
const KW = /\b(import|from|export|function|const|let|return|async|await|if|new|throw|type|else)\b/;

/** A tiny TSX highlighter. Plain text inherits the editor colour, so only tokens get a span. */
function Highlight({ line }: { line: string }) {
  return line.split(/(\/\/.*$|"[^"]*"|`[^`]*`|\b\d+\b|<\/?[A-Za-z][A-Za-z.]*|\b[A-Za-z_]+(?=\())/).map((p, i) => {
    if (i % 2 === 0) return p.split(KW).map((q, j) => (j % 2 ? <span key={`${i}.${j}`} className="sk">{q}</span> : q));
    const c = p.startsWith('//') ? 'sc' : p[0] === '"' || p[0] === '`' ? 'ss' : /^\d/.test(p) ? 'sn' : KW.test(p) ? 'sk' : 'sf';
    return <span key={i} className={c}>{p}</span>;
  });
}

const CODE = `import { useState } from "react"
import { useRouter } from "next/navigation"
import { useCart } from "@/lib/cart"
import { formatPrice } from "@/lib/money"

export function Checkout() {
  const router = useRouter()
  const { items, total } = useCart()
  const [status, setStatus] = useState<"idle" | "paying">("idle")

  async function pay() {
    setStatus("paying")
    const res = await fetch("/api/pay", {
      method: "POST",
      body: JSON.stringify({ items, total }),
    })
    if (!res.ok) throw new Error("Payment failed")
    router.push("/thanks")
  }

  return (
    <button onClick={pay} disabled={status === "paying"}>
      Pay {formatPrice(total)}
    </button>
  )
}`.split('\n');

const TREE: [number, string, boolean?][] = [
  [0, 'app', true], [1, 'checkout', true], [2, 'page.tsx'], [2, 'checkout.tsx', true], [0, 'components', true],
  [1, 'button.tsx'], [1, 'price.tsx'], [0, 'lib', true], [1, 'cart.ts'], [1, 'money.ts'], [1, 'pay.ts'],
];

export function EditorApp({ bare }: { bare?: boolean }) {
  return (
    <div className="flex h-full w-full flex-col bg-[#1E2027] text-[#C9CEDA]">
      {!bare && <TitleBar title="checkout.tsx - shop - Code" dark glyph="#2B44D9" />}
      <div className="flex min-h-0 flex-1">
        <div className="flex w-[42px] shrink-0 flex-col items-center gap-[18px] bg-[#16181D] pt-3 text-[#6B7487]">
          <span className="text-[#C9CEDA]"><Icon n="file" s={18} /></span>
          <Icon n="search" s={18} />
          <Icon n="branch" s={18} />
          <Icon n="frame" s={18} />
        </div>
        <div className="w-[168px] shrink-0 bg-[#181A20] py-2 text-[12.5px]">
          <p className="px-4 pb-2 pt-1 text-[10.5px] font-semibold tracking-[0.08em] text-[#6B7487]">SHOP</p>
          {TREE.map(([d, name, dir]) => (
            <div key={name} className={`flex h-[22px] items-center gap-1.5 truncate ${name === 'checkout.tsx' ? 'bg-[#262A36] text-white' : 'text-[#A3ABBC]'}`} style={{ paddingLeft: 16 + d * 14 }}>
              <span className="text-[#6B7487]"><Icon n={dir ? 'chevD' : 'file'} s={11} /></span>
              {name}
            </div>
          ))}
        </div>
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex h-[34px] shrink-0 items-end bg-[#16181D] text-[12.5px]">
            {['checkout.tsx', 'cart.ts', 'pay.ts'].map((t, i) => (
              <div key={t} className={`flex h-full items-center gap-2 px-4 ${i ? 'text-[#7B8499]' : 'border-t-2 border-[#7C93FF] bg-[#1E2027] text-white'}`}>
                <span className="h-2 w-2 rounded-full bg-[#7C93FF]" />
                {t}
              </div>
            ))}
          </div>
          <div className="flex h-[24px] shrink-0 items-center gap-1.5 px-4 text-[11.5px] text-[#7B8499]">
            app <Icon n="chevR" s={10} /> checkout <Icon n="chevR" s={10} /> checkout.tsx
          </div>
          <div className="relative min-h-0 flex-1 overflow-hidden py-1 font-mono text-[12.5px] leading-[20px]">
            {CODE.map((l, i) => (
              <div key={i} className={`flex ${i === 16 ? 'bg-[#272B38]' : ''}`}>
                <span className="w-[46px] shrink-0 select-none pr-4 text-right text-[#4C5468]">{i + 1}</span>
                <span className="whitespace-pre"><Highlight line={l} /></span>
              </div>
            ))}
            <div className="absolute inset-y-1 right-2 w-[54px] space-y-[3px] opacity-60">
              {CODE.map((l, i) => (
                <div key={i} className="h-[3px] rounded-full bg-[#4C5468]" style={{ width: Math.max(8, Math.min(54, l.length * 1.1)), marginLeft: l.search(/\S|$/) * 1.2 }} />
              ))}
            </div>
          </div>
        </div>
      </div>
      <div className="flex h-[24px] shrink-0 items-center justify-between bg-[#101216] px-3 text-[11.5px] text-[#8E97AA]">
        <span className="flex items-center gap-1.5"><Icon n="branch" s={12} /> feat/checkout</span>
        <span>Ln 17, Col 40 &nbsp;&nbsp; TypeScript React</span>
      </div>
    </div>
  );
}

/* class names: ss green, sv violet, sn amber, sf blue, sm grey; unmarked text is the terminal's own colour */
const PROMPT: [string, string][] = [['sf', 'shop'], ['sm', ' on '], ['sv', ' feat/checkout'], ['ss', ' ❯ ']];
const TERM: [string, string][][] = [
  [...PROMPT, ['', 'npm run dev']],
  [['', '']],
  [['', '  ▲ Next.js 14.2.35']],
  [['sm', '  - Local:        '], ['sf', 'http://localhost:3000']],
  [['ss', ' ✓ '], ['', 'Ready in 842ms']],
  [['sn', ' ○ '], ['sm', 'Compiling /checkout ...']],
  [['ss', ' ✓ '], ['', 'Compiled /checkout in 311ms']],
  [['sm', ' GET /checkout '], ['ss', '200'], ['sm', ' in 38ms']],
  [['sm', ' POST /api/pay '], ['ss', '200'], ['sm', ' in 112ms']],
  [['sm', ' GET /thanks '], ['ss', '200'], ['sm', ' in 21ms']],
  PROMPT,
];

export function TerminalApp() {
  return (
    <div className="flex h-full w-full flex-col bg-[#0F1115] text-[#D9DEE9]">
      <div className="flex h-[34px] shrink-0 select-none items-stretch justify-between bg-[#101216] text-[12px] text-[#8E97AA]">
        <div className="flex items-end pl-2">
          <div className="flex h-[28px] items-center gap-2 rounded-t-[8px] bg-[#0F1115] px-4 text-[#D9DEE9]">
            <span className="text-[#5FD3B6]">&gt;_</span> PowerShell
          </div>
          <div className="flex h-[28px] items-center gap-2 px-4">Ubuntu</div>
          <div className="flex h-[28px] items-center px-2"><Icon n="plus" s={12} /></div>
        </div>
        <Controls />
      </div>
      <div className="flex min-h-0 flex-1 flex-col justify-end overflow-hidden px-4 py-3 font-mono text-[12.5px] leading-[20px]">
        {TERM.map((segs, i) => (
          <div key={i} className="whitespace-pre">
            {segs.map(([c, t], j) => (c ? <span key={j} className={c}>{t}</span> : t))}
            {i === TERM.length - 1 && <span className="blink inline-block h-[14px] w-[7px] translate-y-[2px] bg-[#D9DEE9]" />}
          </div>
        ))}
      </div>
    </div>
  );
}

/* ── browser ───────────────────────────────────────────────────────────── */
/** A small mosaic swatch: one stroked path per colour instead of a rect per tile. */
function Mosaic({ seed, colors, cols }: { seed: number; colors: string[]; cols: number }) {
  const paths: Record<string, string> = {};
  let a = seed * 9301 + 49297;
  for (let i = 0; i < cols * cols; i++) {
    a = (a * 9301 + 49297) % 233280;
    const c = colors[Math.floor((a / 233280) * colors.length)];
    paths[c] = `${paths[c] ?? ''}M${(i % cols) * 10 + 2.5} ${Math.floor(i / cols) * 10 + 2.5}h5v5h-5z`;
  }
  return (
    <svg viewBox={`0 0 ${cols * 10 + 1} ${cols * 10 + 1}`} className="block h-full w-full" preserveAspectRatio="xMidYMid slice" strokeWidth="3" strokeLinejoin="round" aria-hidden="true">
      <rect width="100%" height="100%" fill="#E8E4DA" />
      {Object.entries(paths).map(([c, d]) => (
        <path key={c} d={d} fill={c} stroke={c} />
      ))}
    </svg>
  );
}

function Pull() {
  const diff = [' export function total(items: Item[]) {', '-  return items.reduce((s, i) => s + i.price, 0)', '+  const sum = items.reduce((s, i) => s + i.price * i.qty, 0)', '+  return Math.round(sum * 100) / 100', ' }', ' ', '+export const SHIPPING_FREE_OVER = 75'];
  return (
    <div className="h-full px-6 py-5">
      <p className="text-[22px] font-semibold leading-tight tracking-[-0.01em]">
        Add one-tap checkout <span className="font-normal text-[#8A93A3]">#482</span>
      </p>
      <div className="mt-2.5 flex items-center gap-2.5 text-[12.5px] text-[#5B6474]">
        <span className="rounded-full bg-[#1F8A5B] px-2.5 py-[3px] font-semibold text-white">Open</span>
        <span>
          <b className="text-[#1B1F29]">ana</b> wants to merge 3 commits into <code className="rounded-[4px] bg-[#EEF0F4] px-1.5">main</code> from{' '}
          <code className="rounded-[4px] bg-[#EEF0F4] px-1.5">feat/checkout</code>
        </span>
      </div>
      <div className="mt-4 flex gap-6 border-b border-[#E3E6EB] text-[13px] text-[#5B6474]">
        {['Conversation 4', 'Commits 3', 'Checks 6', 'Files changed 5'].map((t, i) => (
          <span key={t} className={`pb-2.5 ${i === 3 ? 'border-b-2 border-[#2B44D9] font-semibold text-[#1B1F29]' : ''}`}>{t}</span>
        ))}
      </div>
      <div className="mt-4 overflow-hidden rounded-[8px] border border-[#E3E6EB]">
        <div className="flex items-center justify-between bg-[#F6F7F9] px-3.5 py-2 text-[12.5px]">
          <span className="font-semibold">lib/cart.ts</span>
          <span><b className="text-[#1F8A5B]">+14</b> <b className="text-[#C93450]">−3</b></span>
        </div>
        <div className="font-mono text-[12px] leading-[21px]">
          {diff.map((l, i) => {
            const s = l[0];
            return (
              <div key={i} className={`flex whitespace-pre ${s === '+' ? 'bg-[#E6F6EC]' : s === '-' ? 'bg-[#FCE9EC]' : ''}`}>
                <span className="w-[34px] shrink-0 select-none text-right text-[#9AA3B2]">{i + 41}</span>
                <span className={`w-[22px] shrink-0 text-center ${s === '+' ? 'text-[#1F8A5B]' : s === '-' ? 'text-[#C93450]' : 'text-transparent'}`}>{s}</span>
                <span>{l.slice(1)}</span>
              </div>
            );
          })}
        </div>
      </div>
      <div className="mt-4 flex items-center gap-2.5 rounded-[8px] border border-[#BFE3CF] bg-[#F1FAF5] px-3.5 py-2.5 text-[13px]">
        <span className="grid h-5 w-5 place-items-center rounded-full bg-[#1F8A5B] text-white"><Icon n="check" s={12} w={2} /></span>
        <b>All checks have passed</b> <span className="text-[#5B6474]">6 successful checks</span>
      </div>
    </div>
  );
}

const REFS: [string, string[]][] = [
  ['Roman floor, Ostia', TILES],
  ['Byzantine gold', ['#1B2130', '#C9A24A', '#8C3B2E']],
  ['Glazed ceramics', ['#12786A', '#E8E4DA', '#2B44D9']],
  ['Tessera study', ['#C93450', '#F4B13A', '#EDEFEA', '#1B2130']],
  ['Lapis & bone', ['#2B44D9', '#EDEFEA', '#1B2130']],
  ['Courtyard pattern', ['#12786A', '#F4B13A', '#C93450']],
];
const Refs = () => (
  <div className="h-full bg-[#F6F7F9] px-5 py-4">
    <div className="flex items-end justify-between">
      <p className="text-[18px] font-semibold tracking-[-0.01em]">Tessellation, references</p>
      <p className="text-[12px] text-[#8A93A3]">24 pins</p>
    </div>
    <div className="mt-3.5 grid grid-cols-3 gap-3">
      {REFS.map(([cap, c], i) => (
        <div key={cap}>
          <div className="aspect-[4/3] overflow-hidden rounded-[8px] border border-black/5">
            <Mosaic seed={i + 3} colors={c} cols={i % 2 ? 10 : 7} />
          </div>
          <p className="mt-1.5 truncate text-[11.5px] text-[#5B6474]">{cap}</p>
        </div>
      ))}
    </div>
  </div>
);

const RESULTS = [
  ['Ocean heat content: a regional reassessment', 'journals.example.org › climate › 2024', 'Upper-ocean heat content rose in every basin between 2005 and 2022, with the largest gains in the subtropical Atlantic.'],
  ['Sea surface temperature records, 1980 to today', 'data.example.gov › sst › monthly', 'Monthly gridded anomalies relative to the 1991 to 2020 mean, with uncertainty estimates for each cell.'],
  ['Methods for estimating deep-ocean warming', 'arxiv.example.org › abs › 2403.01177', 'We compare three interpolation schemes for sparse float data and quantify the bias each introduces below 700 m.'],
];

const Sources = () => (
  <div className="h-full px-6 py-5">
    <div className="flex h-[38px] items-center gap-2.5 rounded-full border border-[#DADFE6] px-4 text-[13px] shadow-xs">
      <Icon n="search" s={14} /> ocean heat content trend 0-2000 m
    </div>
    <p className="mt-3 text-[11.5px] text-[#8A93A3]">About 1,240,000 results</p>
    <div className="mt-3 space-y-4">
      {RESULTS.map(([t, u, s]) => (
        <div key={t}>
          <p className="text-[11.5px] text-[#4A5262]">{u}</p>
          <p className="text-[16px] leading-snug text-[#2B44D9]">{t}</p>
          <p className="mt-0.5 text-[12.5px] leading-snug text-[#5B6474]">{s}</p>
        </div>
      ))}
    </div>
  </div>
);

const Dash = () => (
  <div className="h-full bg-[#F6F7F9] px-5 py-4">
    <p className="text-[16px] font-semibold">Creator dashboard</p>
    <div className="mt-3 grid grid-cols-3 gap-2.5">
      {[['Followers', '18.4K', '+212 today'], ['Avg. viewers', '912', '+8%'], ['Chat / min', '148', 'peak 203']].map(([k, v, d]) => (
        <div key={k} className="rounded-[8px] border border-[#E3E6EB] bg-white px-3 py-2.5">
          <p className="text-[11px] text-[#8A93A3]">{k}</p>
          <p className="text-[20px] font-semibold leading-tight">{v}</p>
          <p className="text-[11px] text-[#1F8A5B]">{d}</p>
        </div>
      ))}
    </div>
    <div className="mt-3 space-y-1.5 text-[12px] text-[#4A5262]">
      {['nova_k followed', 'Kestrel subscribed (Tier 1)', 'pilotfish raided with 64 viewers'].map((t) => (
        <p key={t} className="flex items-center gap-2"><span className="h-1.5 w-1.5 rounded-full bg-[#2B44D9]" />{t}</p>
      ))}
    </div>
  </div>
);

const PAGES: Record<string, [string, string, () => ReactNode]> = {
  pr: ['git.acme.dev/shop/pull/482/files', 'Add one-tap checkout #482', Pull],
  refs: ['pinboard.example/brand-refresh', 'Tessellation, references', Refs],
  sources: ['search.example/?q=ocean+heat+content', 'ocean heat content trend', Sources],
  dash: ['studio.example/dashboard', 'Creator dashboard', Dash],
};

function BrowserApp({ variant = 'pr' }: { variant?: string }) {
  const [url, tab, Page] = PAGES[variant] ?? PAGES.pr;
  return (
    <div className="flex h-full w-full flex-col bg-white text-[#1B1F29]">
      <div className="flex h-[36px] shrink-0 select-none items-end justify-between bg-[#DDE1E8] pl-2 text-[12px]">
        <div className="flex h-[30px] items-center gap-2 rounded-t-[8px] bg-[#F6F7F9] pl-3 pr-4">
          <span className="h-3 w-3 rounded-[4px] bg-[#12786A]" />
          <span className="max-w-[160px] truncate">{tab}</span>
          <Icon n="close" s={10} />
        </div>
        <Controls className="text-[#4A5262]" />
      </div>
      <div className="flex h-[40px] shrink-0 items-center gap-3 bg-[#F6F7F9] px-3 text-[#5B6474]">
        <Icon n="back" s={15} />
        <Icon n="fwd" s={15} />
        <Icon n="reload" s={15} />
        <div className="flex h-[28px] min-w-0 flex-1 items-center gap-2 rounded-full bg-[#E9ECF1] px-3 text-[12.5px]">
          <Icon n="lock" s={12} />
          <span className="truncate">{url}</span>
        </div>
        <Avatar name="S" color="#2B44D9" size={22} />
      </div>
      <div className="min-h-0 flex-1 overflow-hidden border-t border-[#E3E6EB]"><Page /></div>
    </div>
  );
}

/* ── chat ──────────────────────────────────────────────────────────────── */
type Msg = [who: string, time: string, text: string];
const PEOPLE: Record<string, string> = { Ana: '#2B44D9', Ravi: '#C93450', Mia: '#12786A', Elena: '#C93450', You: '#2B44D9' };

const TEAM: Msg[] = [
  ['Mia', '09:31', 'Morning. Checkout copy is in review, feedback welcome.'],
  ['Ravi', '09:48', 'Looks good to me. One nit on the empty cart state.'],
  ['Ana', '10:05', 'Fixed the nit and pushed. CI is running.'],
  ['Mia', '10:12', 'Thanks! Approving once it goes green.'],
  ['Ana', '10:42', 'Staging is green. Merging #482 after lunch.'],
  ['Ravi', '10:44', 'Pay button copy is updated. Can someone check it on mobile?'],
  ['Mia', '10:51', 'On it. Release notes draft is in the doc.'],
  ['Ana', '10:58', 'Thanks both. Tagging v2.4.0 at 3.'],
];

const CLIENT: Msg[] = [
  ['Elena', 'Mon', 'Hi! Attaching the references we talked about on the call.'],
  ['You', 'Mon', 'Got them, thank you. The tessellation direction is great, I will start there.'],
  ['Elena', 'Mon', 'Perfect. No rush, we only need it before the pitch.'],
  ['Elena', 'Tue', 'Love the tile mark. Could the wordmark sit a little tighter?'],
  ['You', 'Tue', 'Yes. Sending a tighter version with the lockups this afternoon.'],
  ['Elena', 'Wed', 'Perfect. Approved for the pitch deck.'],
];

/* live chat: [name, colour, text, badge]; a name of '!' marks a system notice */
const BADGE: Record<string, string> = { m: '#12786A', s: '#7C5CFF', v: '#F4B13A' };
const N: Record<string, string> = { Kestrel: '#F4B13A', tern: '#5FD3B6', dunes_: '#C792EA', nova_k: '#7C93FF', quill: '#E86F5B', mossy: '#F78FA7', pilotfish: '#5FD3B6' };
const B: Record<string, string> = { Kestrel: 'm', nova_k: 's', quill: 's', pilotfish: 'v' };
const LIVE: [string, string][] = [
  ['Kestrel', 'reminder: be kind in here, new people join every night'],
  ['tern', 'first time catching a live run of this boss'],
  ['dunes_', 'good luck chat is rooting for u'],
  ['nova_k', 'that opener was so smooth'],
  ['quill', 'brb snacks'],
  ['mossy', 'is this the speedrun route or just vibes'],
  ['pilotfish', 'vibes. route is way riskier'],
  ['tern', 'lmao the camera'],
  ['Kestrel', 'clip that one @quill'],
  ['dunes_', 'hiii'],
  ['Kestrel', 'welcome in. clips are on, no spoilers pls'],
  ['quill', 'ok that last fight was insane'],
  ['mossy', 'wait how did u get past phase 2 lmao'],
  ['tern', 'he parried it. he PARRIED it'],
  ['pilotfish', 'phase 2 is just patience tbh'],
  ['nova_k', 'KEKW'],
  ['mossy', '@pilotfish i keep dying to the sweep'],
  ['pilotfish', 'roll toward it not away'],
  ['!', 'pilotfish is raiding with 64 viewers'],
  ['quill*', 'raid gang!!'],
  ['dunes_', 'o7'],
  ['tern', 'been here since the 5am stream and the soundtrack still slaps'],
  ['nova_k', 'who made the overlay, it looks clean'],
  ['Kestrel', '@nova_k its his own, link is in the panel'],
  ['mossy', 'ok going in, wish me luck'],
];

function Messages({ msgs, compact }: { msgs: Msg[]; compact?: boolean }) {
  return (
    <div className={compact ? 'space-y-2 px-4 py-2.5' : 'space-y-3.5 px-4 py-3.5'}>
      {msgs.map(([who, time, text], i) =>
        compact ? (
          <p key={i} className="text-[13px] leading-snug text-[#2A303C]">
            <b style={{ color: PEOPLE[who] }}>{who}</b> <span className="text-[11px] text-[#8A93A3]">{time}</span>
            <br />
            {text}
          </p>
        ) : (
          <div key={i} className="flex gap-2.5">
            <Avatar name={who} color={PEOPLE[who]} size={30} />
            <div className="min-w-0">
              <p className="text-[12.5px]"><b>{who}</b> <span className="text-[11px] text-[#8A93A3]">{time}</span></p>
              <p className="text-[13px] leading-snug text-[#2A303C]">{text}</p>
            </div>
          </div>
        ),
      )}
    </div>
  );
}

const Composer = ({ text }: { text: string }) => (
  <div className="mx-4 mb-3.5 flex h-[38px] shrink-0 items-center justify-between rounded-[8px] border border-[#D5DAE2] px-3 text-[12.5px] text-[#8A93A3]">
    {text}
    <Icon n="send" s={14} />
  </div>
);

function ChatApp({ variant = 'team' }: { variant?: string }) {
  if (variant === 'live') {
    return (
      <div className="flex h-full w-full flex-col bg-[#18181B] text-[#E6E6EA]">
        <TitleBar title="Stream chat" dark glyph="#C93450" />
        <div className="flex h-[34px] shrink-0 items-center justify-between border-b border-white/10 px-4 text-[12px]">
          <b>Stream chat</b>
          <span className="flex items-center gap-1.5 text-[#8E97AA]"><Icon n="eye" s={13} /> 1,247</span>
        </div>
        <div className="chat-fade flex min-h-0 flex-1 flex-col justify-end gap-[3px] overflow-hidden py-2 text-[13px] leading-[1.4]">
          {LIVE.map(([name, text], i) => {
            if (name === '!') {
              return (
                <p key={i} className="mx-2 my-1 rounded-[5px] border-l-[3px] border-[#7C5CFF] bg-[#7C5CFF]/15 px-2.5 py-1.5 text-[12.5px] text-[#D8D2FF]">
                  {text}
                </p>
              );
            }
            const hi = name.endsWith('*');
            const n = hi ? name.slice(0, -1) : name;
            return (
              <p key={i} className={`px-3 py-[3px] ${hi ? 'mx-1 rounded-[5px] bg-white/[0.07]' : ''}`}>
                {B[n] && <span className="mr-1.5 inline-block h-[13px] w-[13px] translate-y-[2px] rounded-[3px]" style={{ background: BADGE[B[n]] }} />}
                <b style={{ color: N[n] }}>{n}</b>
                <span className="text-[#8E97AA]">: </span>
                {text}
              </p>
            );
          })}
        </div>
        <div className="mx-3 mb-3 flex h-[36px] shrink-0 items-center rounded-[8px] bg-[#26262C] px-3 text-[12.5px] text-[#8E97AA]">Send a message</div>
      </div>
    );
  }
  if (variant === 'client') {
    return (
      <div className="flex h-full w-full flex-col bg-white text-[#1B1F29]">
        <TitleBar title="Northwind - Elena Voss" glyph="#C93450" />
        <div className="flex h-[38px] shrink-0 items-center gap-2 border-b border-[#E3E6EB] px-4 text-[13px]">
          <Avatar name="E" color="#C93450" size={22} /> <b>Elena Voss</b> <span className="text-[#8A93A3]">Northwind</span>
        </div>
        <div className="chat-fade flex min-h-0 flex-1 flex-col justify-end overflow-hidden"><Messages msgs={CLIENT} compact /></div>
        <Composer text="Message Elena" />
      </div>
    );
  }
  return (
    <div className="flex h-full w-full flex-col bg-white text-[#1B1F29]">
      <TitleBar title="Acme - releases" glyph="#C93450" />
      <div className="flex min-h-0 flex-1">
        <div className="w-[150px] shrink-0 bg-[#1B2130] px-3 py-3 text-[12.5px] text-[#9AA3B8]">
          <p className="mb-2.5 text-[13px] font-semibold text-white">Acme</p>
          {['general', 'releases', 'design', 'random'].map((c) => (
            <p key={c} className={`flex items-center gap-1.5 rounded-[6px] px-2 py-[5px] ${c === 'releases' ? 'bg-[#2B44D9] text-white' : ''}`}>
              <Icon n="hash" s={12} /> {c}
            </p>
          ))}
          <p className="mb-1.5 mt-4 px-2 text-[11px] text-[#6B7487]">Direct messages</p>
          {['Ana', 'Ravi', 'Mia'].map((n) => (
            <p key={n} className="flex items-center gap-2 px-2 py-[5px]"><span className="h-2 w-2 rounded-full bg-[#5FD3B6]" />{n}</p>
          ))}
        </div>
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex h-[38px] shrink-0 items-center gap-1.5 border-b border-[#E3E6EB] px-4 text-[13px] font-semibold">
            <Icon n="hash" s={13} /> releases
          </div>
          <div className="chat-fade flex min-h-0 flex-1 flex-col justify-end overflow-hidden"><Messages msgs={TEAM} /></div>
          <Composer text="Message #releases" />
        </div>
      </div>
    </div>
  );
}

/* ── design tool ───────────────────────────────────────────────────────── */
const LAYERS: [number, string, boolean?][] = [[0, 'Page 1'], [0, 'Logo lockups'], [1, 'Lockup / dark', true], [1, 'Lockup / light'], [1, 'Mark'], [0, 'Colour'], [1, 'Tiles']];

function DesignApp() {
  return (
    <div className="flex h-full w-full flex-col bg-[#1E2027] text-[#C9CEDA]">
      <TitleBar title="Logo system - Design" dark glyph="#F4B13A" />
      <div className="flex h-[40px] shrink-0 items-center justify-between border-b border-white/10 bg-[#16181D] px-3 text-[#8E97AA]">
        <div className="flex items-center gap-1">
          {['move', 'frame', 'shape', 'pen', 'text', 'hand'].map((n, i) => (
            <span key={n} className={`grid h-[28px] w-[28px] place-items-center rounded-[6px] ${i ? '' : 'bg-[#2B44D9] text-white'}`}>
              <Icon n={n} s={15} />
            </span>
          ))}
        </div>
        <div className="flex items-center gap-4 text-[12px]">
          <span>100%</span>
          <span className="rounded-[6px] bg-[#2B44D9] px-3 py-1 font-semibold text-white">Share</span>
        </div>
      </div>
      <div className="flex min-h-0 flex-1">
        <div className="w-[176px] shrink-0 border-r border-white/10 bg-[#16181D] py-2.5 text-[12.5px]">
          <p className="px-3.5 pb-2 text-[11px] font-semibold text-[#6B7487]">Layers</p>
          {LAYERS.map(([d, n, sel], i) => (
            <div key={n} className={`flex h-[26px] items-center gap-1.5 ${sel ? 'bg-[#2B44D9]/30 text-white' : 'text-[#A3ABBC]'}`} style={{ paddingLeft: 14 + d * 16 }}>
              <Icon n={d === 0 && i > 0 ? 'chevD' : 'shape'} s={11} />
              {n}
            </div>
          ))}
        </div>
        <div className="relative min-w-0 flex-1 overflow-hidden bg-[#2A2D36]" style={{ backgroundImage: 'radial-gradient(#3A3E4A 1px, transparent 1px)', backgroundSize: '18px 18px' }}>
          <div className="absolute left-[7%] top-[9%] h-[40%] w-[40%] rounded-[6px] bg-white p-[8%] shadow-lg">
            <Wordmark className="h-full w-full" fill="#161B25" decorative />
          </div>
          <div className="absolute left-[53%] top-[9%] h-[40%] w-[40%] rounded-[6px] bg-[#161B25] p-[8%] shadow-lg">
            <Wordmark className="h-full w-full" fill="#F7F8F4" decorative />
          </div>
          <div className="absolute left-[7%] top-[56%] flex h-[34%] w-[40%] gap-[3%] rounded-[6px] bg-[#EDEFEA] p-[4%] shadow-lg">
            {['#2B44D9', '#12786A', '#F4B13A', '#C93450'].map((c) => (
              <span key={c} className="flex-1 rounded-[5px]" style={{ background: c }} />
            ))}
          </div>
          <div className="absolute left-[53%] top-[56%] grid h-[34%] w-[40%] place-items-center rounded-[6px] bg-[#2B44D9] shadow-lg">
            <Wordmark className="w-[62%]" fill="#FFFFFF" tiles={['#FFFFFF', '#FFFFFF']} decorative />
          </div>
          <div className="pointer-events-none absolute left-[7%] top-[9%] h-[40%] w-[40%] border border-[#7C93FF]">
            {['-3px', 'calc(100% - 3px)'].flatMap((t) =>
              ['-3px', 'calc(100% - 3px)'].map((l) => <span key={l + t} className="absolute h-[7px] w-[7px] border border-[#7C93FF] bg-white" style={{ left: l, top: t }} />),
            )}
            <span className="absolute left-1/2 top-[calc(100%+8px)] -translate-x-1/2 whitespace-nowrap rounded-[4px] bg-[#7C93FF] px-1.5 py-[1px] text-[10.5px] font-semibold text-white">320 × 200</span>
          </div>
        </div>
        <div className="w-[190px] shrink-0 space-y-3.5 border-l border-white/10 bg-[#16181D] px-3.5 py-3 text-[12px]">
          <p className="text-[11px] font-semibold text-[#6B7487]">Design</p>
          <div className="grid grid-cols-2 gap-2 text-[#A3ABBC]">
            {['X 120', 'Y 96', 'W 320', 'H 200'].map((v) => (
              <span key={v} className="rounded-[5px] bg-[#23262E] px-2 py-1">{v}</span>
            ))}
          </div>
          <p className="pt-1 text-[11px] font-semibold text-[#6B7487]">Fill</p>
          {['FFFFFF', '2B44D9', 'F4B13A'].map((h) => (
            <div key={h} className="flex items-center gap-2 text-[#A3ABBC]">
              <span className="h-[18px] w-[18px] rounded-[4px] ring-1 ring-white/20" style={{ background: `#${h}` }} />
              <span className="flex-1">{h}</span>
              <span>100%</span>
            </div>
          ))}
          <p className="pt-1 text-[11px] font-semibold text-[#6B7487]">Text</p>
          <p className="text-[#A3ABBC]">Bricolage Grotesque<br />Bold · 96 · −3.5%</p>
        </div>
      </div>
    </div>
  );
}

/* ── notes, reader, charts ─────────────────────────────────────────────── */
function NotesApp({ variant = 'brief' }: { variant?: string }) {
  if (variant === 'draft') {
    const P = ({ children }: { children: ReactNode }) => <p className="mt-2 text-[13.5px] leading-[1.7] text-[#2A303C]">{children}</p>;
    return (
      <div className="flex h-full w-full flex-col bg-[#FBFBF8] text-[#1B1F29]">
        <TitleBar title="Thesis - 3. Results" glyph="#12786A" />
        <div className="min-h-0 flex-1 overflow-hidden px-9 py-7">
          <p className="text-[11.5px] text-[#8A93A3]">Chapter 3</p>
          <p className="font-display text-[28px] font-bold leading-tight tracking-[-0.02em]">Results</p>
          <p className="mt-5 font-display text-[17px] font-semibold">3.1 Warming of the upper ocean</p>
          <P>
            Between 1980 and 2020 the global mean sea surface temperature anomaly rose by 0.87 °C (Figure 3.2). The increase was not uniform: the
            subtropical Atlantic and the Indian Ocean warmed fastest, while the Southern Ocean showed the weakest trend over the same period.
          </P>
          <p className="mt-3 text-[13.5px] leading-[1.7] text-[#2A303C]">
            Heat content in the top 700 m followed a similar pattern<sup className="text-[#C93450]">12</sup>. We therefore treat the two series as
            consistent and report regional estimates in Table 3.1, with uncertainty ranges derived from the float coverage described in Section 2.4.
          </p>
          <p className="mt-5 font-display text-[17px] font-semibold">3.2 Regional differences</p>
          <P>
            Regional trends diverge most strongly after 2005, when the float network reached full global coverage and the sampling bias in the
            earlier record largely disappears.
          </P>
        </div>
        <div className="flex h-[26px] shrink-0 items-center justify-between border-t border-[#E3E6EB] px-4 text-[11.5px] text-[#8A93A3]">
          <span>2,418 words</span>
          <span>Saved</span>
        </div>
      </div>
    );
  }
  return (
    <div className="flex h-full w-full flex-col bg-white text-[#1B1F29]">
      <TitleBar title="Brand refresh brief - Notes" glyph="#12786A" />
      <div className="min-h-0 flex-1 overflow-hidden px-6 py-4">
        <p className="font-display text-[22px] font-bold leading-tight tracking-[-0.02em]">Brand refresh brief</p>
        <p className="mt-2 flex gap-4 text-[12px] text-[#5B6474]">
          <span className="rounded-full bg-[#FFF1D6] px-2.5 py-0.5 font-semibold text-[#8A5A00]">In progress</span>
          <span>Due Oct 17</span>
          <span>Owner: you</span>
        </p>
        <p className="mt-3.5 text-[13px] font-semibold">Goals</p>
        <ul className="mt-1 space-y-1 text-[13px] text-[#2A303C]">
          <li>• A mark that works at 16 px and on a billboard</li>
          <li>• Warm, mineral palette, no gradients</li>
        </ul>
        <p className="mt-3.5 text-[13px] font-semibold">To do</p>
        <div className="mt-1.5 space-y-1.5 text-[13px]">
          {(['Tighten wordmark spacing', 'Export lockups for the deck', 'Colour-blind check'] as const).map((t, i) => (
            <p key={t} className="flex items-center gap-2">
              <span className={`grid h-[15px] w-[15px] place-items-center rounded-[4px] border ${i < 2 ? 'border-[#12786A] bg-[#12786A] text-white' : 'border-[#9AA3B2]'}`}>
                {i < 2 && <Icon n="check" s={11} w={2.2} />}
              </span>
              <span className={i < 2 ? 'text-[#8A93A3] line-through' : ''}>{t}</span>
            </p>
          ))}
        </div>
      </div>
    </div>
  );
}

const ABSTRACT =
  'We analyse temperature profiles from 3,900 autonomous floats between 2005 and 2022 and estimate heat content change in the upper 2000 m of the ocean. Regional trends are computed on a one-degree grid and corrected for the sampling bias of the pre-float record.';

function PaperApp() {
  return (
    <div className="flex h-full w-full flex-col bg-[#3A3D45] text-[#E6E9F0]">
      <TitleBar title="ocean-heat-2024.pdf - Reader" dark glyph="#C93450" />
      <div className="flex h-[34px] shrink-0 items-center justify-between bg-[#2B2E35] px-4 text-[12px] text-[#AEB5C4]">
        <span>7 / 24</span>
        <span className="flex items-center gap-3"><Icon n="search" s={14} /> 125%</span>
      </div>
      <div className="flex min-h-0 flex-1">
        <div className="w-[64px] shrink-0 space-y-2.5 bg-[#2B2E35] px-2.5 py-3">
          {[0, 1, 2, 3].map((i) => (
            <div key={i} className={`aspect-[3/4] rounded-[3px] bg-white ${i === 2 ? 'ring-2 ring-[#7C93FF]' : 'opacity-80'}`} />
          ))}
        </div>
        <div className="min-w-0 flex-1 overflow-hidden px-6 pt-5">
          <div className="mx-auto h-[130%] max-w-[420px] rounded-[2px] bg-white px-7 py-6 text-[#1B1F29] shadow-xl">
            <p className="text-center font-display text-[18px] font-bold leading-tight">Upper-ocean heat content and its regional trends</p>
            <p className="mt-1.5 text-center text-[10.5px] text-[#5B6474]">R. Okafor, L. Haugen, M. Tanaka</p>
            <p className="mt-4 text-[9px] font-bold uppercase tracking-wide">Abstract</p>
            <p className="mt-1 text-[10px] leading-[1.55]">
              {ABSTRACT.slice(0, 120)}
              <mark className="bg-[#F4D37A] text-inherit">{ABSTRACT.slice(120, 250)}</mark>
              {ABSTRACT.slice(250)}
            </p>
            <div className="mt-3.5 grid grid-cols-2 gap-4 text-[9.5px] leading-[1.5] text-[#2A303C]">
              <p>
                The warming signal is largest in the subtropical gyres, where surface heat is mixed downward by winter convection and carried below the
                mixed layer. Between 30°N and 30°S, the upper 700 m gained heat in every basin we examined.
              </p>
              <p>
                Uncertainty is dominated by sparse sampling in the Southern Ocean before 2007. After the float network reached full coverage, the
                sampling error falls below the size of the regional signal in all but the highest latitudes.
              </p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function LineChart({ data, xLabels, yTicks, color, h, dark }: { data: number[]; xLabels: string[]; yTicks: number[]; color: string; h: number; dark?: boolean }) {
  const id = useSvgId();
  const [w, padL, padR, padT, padB] = [420, 38, 14, 12, 26];
  const [min, max] = [Math.min(...yTicks), Math.max(...yTicks)];
  const X = (i: number, n = data.length) => padL + (i / (n - 1)) * (w - padL - padR);
  const Y = (v: number) => padT + (1 - (v - min) / (max - min)) * (h - padT - padB);
  const line = data.map((v, i) => `${i ? 'L' : 'M'}${X(i).toFixed(1)} ${Y(v).toFixed(1)}`).join(' ');
  const [grid, txt] = dark ? ['#2A2F3B', '#7B8499'] : ['#E4E7EC', '#8A93A3'];
  const last = data.length - 1;
  return (
    <svg viewBox={`0 0 ${w} ${h}`} className="block h-full w-full" aria-hidden="true" fontSize="10" fill={txt}>
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor={color} stopOpacity="0.28" />
          <stop offset="1" stopColor={color} stopOpacity="0" />
        </linearGradient>
      </defs>
      {yTicks.map((t) => (
        <g key={t}>
          <line x1={padL} x2={w - padR} y1={Y(t)} y2={Y(t)} stroke={grid} />
          <text x={padL - 8} y={Y(t) + 3.5} textAnchor="end">{t}</text>
        </g>
      ))}
      {xLabels.map((l, i) => (
        <text key={l} x={X(i, xLabels.length)} y={h - 8} textAnchor="middle">{l}</text>
      ))}
      <path d={`${line} L${X(last).toFixed(1)} ${h - padB} L${X(0).toFixed(1)} ${h - padB} Z`} fill={`url(#${id})`} />
      <path d={line} fill="none" stroke={color} strokeWidth="2" strokeLinejoin="round" strokeLinecap="round" />
      <circle cx={X(last)} cy={Y(data[last])} r="4.5" fill={dark ? '#14161B' : '#FFFFFF'} stroke={color} strokeWidth="2" />
    </svg>
  );
}

const SST = Array.from({ length: 41 }, (_, i) => 0.02 * i + 0.06 * Math.sin(i * 0.9) + 0.03 * Math.sin(i * 2.3) - 0.05);
const VIEWERS = Array.from({ length: 36 }, (_, i) => 420 + i * 22 + 90 * Math.sin(i * 0.8) + 60 * Math.sin(i * 2.1) + (i > 24 ? 160 : 0));

function ChartApp({ variant = 'sst' }: { variant?: string }) {
  const dark = variant === 'viewers';
  return (
    <div className={`flex h-full w-full flex-col ${dark ? 'bg-[#14161B] text-[#E6E9F0]' : 'bg-white text-[#1B1F29]'}`}>
      <TitleBar title={dark ? 'Stats' : 'Sea surface temperature - Data'} dark={dark} glyph={dark ? '#12786A' : '#2B44D9'} />
      <div className="flex min-h-0 flex-1 flex-col px-5 py-4">
        <p className={`text-[12px] ${dark ? 'text-[#8E97AA]' : 'text-[#8A93A3]'}`}>{dark ? 'Concurrent viewers' : 'Sea surface temperature anomaly, global mean'}</p>
        {dark ? (
          <p className="flex items-baseline gap-2 text-[28px] font-semibold leading-tight">
            1,247 <span className="flex items-center gap-1.5 text-[12px] font-semibold text-[#5FD3B6]"><span className="h-2 w-2 rounded-full bg-[#5FD3B6]" /> Live</span>
          </p>
        ) : (
          <p className="flex items-baseline gap-2 text-[28px] font-semibold leading-tight">
            +0.87 °C <span className="text-[12px] font-semibold text-[#C93450]">since 1980</span>
          </p>
        )}
        <div className="mt-2 min-h-0 flex-1">
          {dark ? (
            <LineChart dark color="#5FD3B6" data={VIEWERS} xLabels={['8 pm', '8:30', '9 pm', '9:30', '10 pm']} yTicks={[0, 500, 1000, 1500]} h={170} />
          ) : (
            <LineChart color="#2B44D9" data={SST} xLabels={['1980', '1990', '2000', '2010', '2020']} yTicks={[-0.2, 0, 0.4, 0.8]} h={190} />
          )}
        </div>
      </div>
    </div>
  );
}

/* ── broadcast software ────────────────────────────────────────────────── */
function Meter({ label, level, icon }: { label: string; level: number; icon: string }) {
  return (
    <div className="flex items-center gap-2.5 text-[11.5px] text-[#AEB5C4]">
      <span className="flex w-[70px] items-center gap-1.5"><Icon n={icon} s={13} /> {label}</span>
      <div className="flex flex-1 gap-[2px]">
        {Array.from({ length: 22 }, (_, i) => (
          <span key={i} className="h-[9px] flex-1 rounded-[1.5px]" style={{ background: i / 22 >= level ? '#2A2F3B' : i > 18.04 ? '#C93450' : i > 13.64 ? '#F4B13A' : '#5FD3B6' }} />
        ))}
      </div>
    </div>
  );
}

function StreamApp() {
  const sky = useSvgId();
  return (
    <div className="flex h-full w-full flex-col bg-[#14161B] text-[#E6E9F0]">
      <TitleBar title="Broadcast - Scene: Main" dark glyph="#2B44D9" />
      <div className="flex min-h-0 flex-1 gap-3 p-3">
        <div className="flex min-w-0 flex-1 flex-col gap-2.5">
          <div className="relative min-h-0 flex-1 overflow-hidden rounded-[8px] bg-black">
            <svg viewBox="0 0 640 360" className="absolute inset-0 h-full w-full" preserveAspectRatio="xMidYMid slice" aria-hidden="true">
              <defs>
                <linearGradient id={sky} x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0" stopColor="#1B2442" />
                  <stop offset="0.7" stopColor="#4A3A6B" />
                  <stop offset="1" stopColor="#C9566B" />
                </linearGradient>
              </defs>
              <rect width="640" height="360" fill={`url(#${sky})`} />
              <circle cx="430" cy="200" r="46" fill="#F4B13A" opacity="0.95" />
              <path d="M0 270 L110 170 L190 240 L280 150 L400 262 L470 210 L640 290 V360 H0Z" fill="#241C3F" />
              <path d="M0 310 L90 250 L180 300 L300 240 L420 305 L540 255 L640 300 V360 H0Z" fill="#150F2B" />
              <rect x="372" y="22" width="248" height="38" rx="8" fill="#000" opacity="0.55" />
              <text x="390" y="47" fontSize="16" fill="#fff" fontWeight="600">New follower: nova_k</text>
            </svg>
            <span className="absolute left-3 top-3 flex items-center gap-1.5 rounded-[5px] bg-[#C93450] px-2 py-[3px] text-[11px] font-bold">
              <span className="h-1.5 w-1.5 rounded-full bg-white" /> LIVE
            </span>
            <div className="absolute bottom-3 right-3 h-[26%] w-[22%] overflow-hidden rounded-[8px] border-2 border-[#2B44D9] bg-[#222A3D]">
              <svg viewBox="0 0 100 80" className="h-full w-full" aria-hidden="true">
                <circle cx="50" cy="34" r="15" fill="#E0B9A0" />
                <path d="M16 80c3-22 18-28 34-28s31 6 34 28z" fill="#2B44D9" />
              </svg>
            </div>
          </div>
          <div className="flex shrink-0 items-center justify-between text-[12px]">
            <span className="rounded-[6px] bg-[#C93450] px-3.5 py-[7px] font-semibold text-white">Stop streaming</span>
            <span className="text-[#8E97AA]">02:14:07 · 1080p 60 fps · 6,000 kbps</span>
          </div>
        </div>
        <div className="flex w-[210px] shrink-0 flex-col gap-3 text-[12px]">
          <div>
            <p className="mb-1.5 text-[11px] font-semibold text-[#6B7487]">Scenes</p>
            {['Main', 'Starting soon', 'Be right back'].map((s, i) => (
              <p key={s} className={`rounded-[5px] px-2.5 py-[5px] ${i ? 'text-[#AEB5C4]' : 'bg-[#2B44D9] text-white'}`}>{s}</p>
            ))}
          </div>
          <div>
            <p className="mb-1.5 text-[11px] font-semibold text-[#6B7487]">Sources</p>
            {['Game capture', 'Webcam', 'Alerts overlay'].map((s) => (
              <p key={s} className="flex items-center gap-2 px-1 py-[3px] text-[#AEB5C4]"><Icon n="eye" s={12} /> {s}</p>
            ))}
          </div>
          <div className="mt-auto space-y-2.5">
            <Meter label="Mic" level={0.58} icon="mic" />
            <Meter label="Desktop" level={0.74} icon="spk" />
          </div>
        </div>
      </div>
    </div>
  );
}

/* ── windows ───────────────────────────────────────────────────────────── */
export const AppBody = memo(function AppBody({ win }: { win: Pick<Win, 'kind' | 'variant'> }) {
  const v = win.variant;
  switch (win.kind) {
    case 'editor': return <EditorApp />;
    case 'terminal': return <TerminalApp />;
    case 'browser': return <BrowserApp variant={v} />;
    case 'chat': return <ChatApp variant={v} />;
    case 'design': return <DesignApp />;
    case 'notes': return <NotesApp variant={v} />;
    case 'paper': return <PaperApp />;
    case 'chart': return <ChartApp variant={v} />;
    case 'stream': return <StreamApp />;
  }
});

/** A window. With Mosaïk it wears its zone frame and tag; plain, it is just a window; flat, it is maximised. */
export function ZoneFrame({ tag, zone, plain, flat, children }: { tag: string; zone: Win['zone']; plain?: boolean; flat?: boolean; children: ReactNode }) {
  const z = ZONE[zone];
  return (
    <div className="relative h-full w-full">
      <span
        className="absolute -top-[25px] left-0 flex h-[21px] items-center rounded-full px-[11px] text-[11.5px] font-semibold leading-none transition-opacity duration-300"
        style={{ background: z.bg, color: z.fg, opacity: plain ? 0 : 1 }}
      >
        {tag}
      </span>
      <div
        className="h-full w-full overflow-hidden transition-[box-shadow] duration-300"
        style={{
          borderRadius: flat ? 0 : 10,
          boxShadow: flat ? 'none' : plain ? '0 0 0 1px rgba(255,255,255,0.14), 0 18px 40px -14px rgba(0,0,0,0.65)' : `0 0 0 2px ${z.bg}, 0 22px 44px -12px rgba(0,0,0,0.6)`,
        }}
      >
        {children}
      </div>
    </div>
  );
}

export const AppWindow = ({ win, plain, flat }: { win: Win; plain?: boolean; flat?: boolean }) => (
  <ZoneFrame tag={win.tag} zone={win.zone} plain={plain} flat={flat}>
    <AppBody win={win} />
  </ZoneFrame>
);
