import Link from 'next/link';
import { TILES } from '@/lib/data';
import { site } from '@/lib/site';

/* Shared vector shapes live once in the page and are drawn with <use>, so the markup stays small. */
const ICONS: Record<string, string> = {
  min: 'M3 8h10',
  max: 'M3.5 3.5h9v9h-9z',
  close: 'M3.5 3.5l9 9M12.5 3.5l-9 9',
  file: 'M4 1.5h5l3 3v10H4z M9 1.5v3h3',
  search: 'M7 11.5a4.5 4.5 0 1 0 0-9 4.5 4.5 0 0 0 0 9z M10.5 10.5L14 14',
  plus: 'M8 3v10M3 8h10',
  hash: 'M6.5 2.5L5.5 13.5M11 2.5L10 13.5M3 6h10.5M2.5 10h10.5',
  back: 'M10 3L5 8l5 5',
  fwd: 'M6 3l5 5-5 5',
  reload: 'M13 8a5 5 0 1 1-1.6-3.7M13 2.5v3h-3',
  lock: 'M4.5 7h7v6.5h-7z M6 7V5.2a2 2 0 0 1 4 0V7',
  check: 'M3 8.5l3 3 7-7',
  chevD: 'M4 6l4 4 4-4',
  chevR: 'M6 4l4 4-4 4',
  send: 'M2 8l12-5-4 11-2.5-4.5z',
  move: 'M3 2l9 5-4 1.5L6.5 13z',
  frame: 'M5 2v12M11 2v12M2 5h12M2 11h12',
  shape: 'M2.5 2.5h11v11h-11z',
  pen: 'M3 13l1-3.5L11 2.5l2.5 2.5-7 7z',
  text: 'M3 3.5h10M8 3.5v9',
  hand: 'M5 8V4a1 1 0 0 1 2 0v3M7 7V3a1 1 0 0 1 2 0v4M9 7V4a1 1 0 0 1 2 0v5c0 2.5-1.5 4-3.5 4S4 11.5 3.5 9.5L3 8',
  branch: 'M5 2.5v8M5 13a1 1 0 1 0 0-.01M11 5a1 1 0 1 0 0-.01M11 6c0 3.5-6 2-6 4.5',
  eye: 'M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8z M8 9.8a1.8 1.8 0 1 0 0-3.6 1.8 1.8 0 0 0 0 3.6z',
  mic: 'M8 2.5a2 2 0 0 0-2 2V8a2 2 0 0 0 4 0V4.5a2 2 0 0 0-2-2z M4 7.5a4 4 0 0 0 8 0M8 11.5v2',
  spk: 'M2.5 6v4h2.5L9 13V3L5 6z M11.5 5.5a3.5 3.5 0 0 1 0 5',
};

/** Bricolage Grotesque 800 outlines of the wordmark; the dots of the ï are drawn as two tiles. */
const WORDMARK =
  'M69 0V-660H310L482-185H487L655-660H885V0H733L742-496H731L540 0H408L221-496H210L219 0ZM1225 14Q1145 14 1084-17Q1022-48 987-110Q952-172 952-265Q952-358 987-420Q1022-481 1084-512Q1146-542 1226-542Q1306-542 1368-511Q1430-480 1465-418Q1500-357 1500-264Q1500-169 1464-107Q1428-45 1366-16Q1303 14 1225 14ZM1230-103Q1266-103 1290-120Q1314-136 1326-170Q1338-204 1338-254Q1338-307 1325-344Q1312-380 1286-400Q1261-419 1221-419Q1187-419 1162-402Q1138-386 1126-352Q1114-318 1114-267Q1114-185 1144-144Q1175-103 1230-103ZM1775 14Q1722 14 1679 4Q1636-5 1604-23Q1572-41 1552-66Q1531-92 1523-123L1643-176Q1650-159 1668-142Q1685-124 1714-113Q1744-102 1786-102Q1826-102 1848-114Q1871-125 1871-147Q1871-163 1858-172Q1846-182 1821-190Q1796-197 1759-204Q1721-212 1682-222Q1643-233 1609-252Q1575-270 1554-300Q1534-331 1534-378Q1534-427 1560-464Q1586-500 1638-521Q1689-542 1763-542Q1829-542 1879-525Q1929-508 1962-476Q1996-445 2009-401L1880-355Q1875-377 1860-393Q1844-409 1820-418Q1797-426 1765-426Q1726-426 1704-414Q1683-402 1683-382Q1683-366 1698-356Q1712-345 1739-338Q1766-331 1803-323Q1842-315 1880-304Q1919-294 1950-276Q1982-259 2000-230Q2019-202 2019-157Q2019-104 1991-66Q1963-27 1908-6Q1854 14 1775 14ZM2197 14Q2153 14 2119-4Q2085-21 2066-54Q2047-88 2047-136Q2047-181 2064-210Q2082-240 2116-258Q2149-275 2196-286Q2243-298 2303-307Q2332-312 2352-316Q2371-321 2381-331Q2391-341 2391-360Q2391-385 2373-403Q2355-421 2315-421Q2287-421 2264-411Q2242-401 2226-382Q2210-363 2202-336L2060-378Q2073-420 2096-451Q2120-482 2153-502Q2186-523 2228-532Q2271-542 2319-542Q2399-542 2450-516Q2500-491 2524-438Q2549-384 2549-300V-219Q2549-183 2550-146Q2552-110 2554-74Q2557-37 2561 0H2419Q2415-23 2411-56Q2407-88 2405-122H2386Q2372-84 2346-52Q2319-21 2282-4Q2244 14 2197 14ZM2270-100Q2288-100 2306-106Q2325-113 2342-124Q2359-136 2373-153Q2387-170 2394-190L2392-269L2414-264Q2395-252 2372-245Q2349-238 2326-234Q2302-230 2280-226Q2257-221 2240-214Q2222-207 2212-195Q2202-183 2202-162Q2202-134 2221-117Q2240-100 2270-100ZM2633 0V-528H2794V0ZM2884 0V-721H3042V-323Q3071-344 3096-368Q3122-392 3144-418Q3165-444 3182-472Q3200-499 3213-528H3397Q3384-489 3361-450Q3338-412 3305-380Q3272-347 3226-324Q3181-301 3124-293V-275Q3196-289 3242-276Q3289-262 3317-232Q3345-202 3360-162Q3375-122 3385-83L3406 0H3231L3221-50Q3211-99 3198-134Q3184-169 3159-188Q3134-208 3087-208H3042V0Z';

export function Sprite() {
  return (
    <svg width="0" height="0" className="absolute" aria-hidden="true">
      <defs>
        <path id="wm" d={WORDMARK} />
        {Object.entries(ICONS).map(([k, d]) => (
          <path key={k} id={`i-${k}`} d={d} />
        ))}
      </defs>
    </svg>
  );
}

export function Icon({ n, s = 14, w = 1.4 }: { n: string; s?: number; w?: number }) {
  return (
    <svg viewBox="0 0 16 16" width={s} height={s} strokeWidth={w} className="ic" aria-hidden="true">
      <use href={`#i-${n}`} />
    </svg>
  );
}

/** The Mosaïk wordmark, from the shared outlines. */
export function Wordmark({ className, fill = 'currentColor', tiles = TILES, decorative }: { className: string; fill?: string; tiles?: string[]; decorative?: boolean }) {
  return (
    <svg viewBox="59 -806 3357 830" className={className} {...(decorative ? { 'aria-hidden': true } : { role: 'img', 'aria-label': 'Mosaïk' })}>
      <use href="#wm" fill={fill} />
      <rect x="2538" y="-796" width="158" height="158" rx="26" fill={tiles[0]} />
      <rect x="2731" y="-796" width="158" height="158" rx="26" fill={tiles[1]} />
    </svg>
  );
}

export const Check = ({ children, s = 16, top = 3 }: { children: React.ReactNode; s?: number; top?: number }) => (
  <li className="flex items-start gap-3">
    <span className="text-lapis" style={{ marginTop: top }}>
      <Icon n="check" s={s} w={2} />
    </span>
    {children}
  </li>
);

const NAV = [
  ['/#how', 'How it works'],
  ['/#people', 'Who it’s for'],
  ['/#platforms', 'Platforms'],
];

export function Header() {
  return (
    <header className="fixed inset-x-0 top-0 z-50 border-b border-grout/80 bg-plaster/85 backdrop-blur-md">
      <div className="page-x flex h-16 items-center justify-between">
        <Link href="/" aria-label="Mosaïk, home" className="text-ink">
          <Wordmark className="block h-7 w-auto" decorative />
        </Link>
        <nav aria-label="Main" className="hidden items-center gap-8 md:flex">
          {NAV.map(([href, label]) => (
            <Link key={href} href={href} className="text-[15px] font-medium text-slate transition-colors hover:text-ink">
              {label}
            </Link>
          ))}
        </nav>
        <Link href="/download" className="btn btn-lapis min-h-[40px] px-5 text-sm">
          Get early access
        </Link>
      </div>
    </header>
  );
}

export function Footer() {
  const links: [string, string][] = [
    ['/download', 'Early access'],
    ['/conditions', 'Privacy and terms'],
    [site.repo, 'GitHub'],
    [`mailto:${site.email}`, site.email],
  ];
  return (
    <footer className="border-t border-grout">
      <div className="page-x flex flex-col gap-8 py-12 md:flex-row md:items-end md:justify-between">
        <div className="space-y-3">
          <Wordmark className="block h-8 w-auto text-ink" />
          <p className="max-w-xs text-sm text-slate">Open source, released under CC0. Take it, change it, ship it.</p>
        </div>
        <nav aria-label="Footer" className="flex flex-wrap gap-x-8 gap-y-3 text-[15px] font-medium">
          {links.map(([href, label]) =>
            href.startsWith('/') ? (
              <Link key={href} href={href} className="text-slate hover:text-ink">
                {label}
              </Link>
            ) : (
              <a key={href} href={href} className="text-slate hover:text-ink" rel="noopener noreferrer">
                {label}
              </a>
            ),
          )}
        </nav>
      </div>
    </footer>
  );
}

const PLATFORMS = [
  ['Windows', 'In development', '#12786A', 'First. Windows lets one app move, resize and group other apps’ windows, so zones, tabs and saved layouts arrive here first.'],
  ['macOS', 'Planned', '#F4B13A', 'Next. Mosaïk asks for the Accessibility permission, then snaps and stacks windows into zones. Apps can’t be embedded in a tab there, so tabs stack and swap instead.'],
  ['Linux', 'Planned, X11 first', '#2B44D9', 'X11 allows the full feature set. Wayland deliberately limits what an app can do to other apps’ windows, so support there depends on the desktop environment.'],
];

export function PlatformList() {
  return (
    <ul className="divide-y divide-grout border-y border-grout">
      {PLATFORMS.map(([name, status, color, text]) => (
        <li key={name} className="grid gap-2 py-6 md:grid-cols-12 md:gap-8">
          <h3 className="font-display text-[1.6rem] font-bold tracking-[-0.025em] md:col-span-3">{name}</h3>
          <p className="flex items-center gap-2.5 text-[15px] font-semibold md:col-span-3">
            <span className="h-2.5 w-2.5 rounded-full" style={{ backgroundColor: color }} aria-hidden="true" />
            {status}
          </p>
          <p className="max-w-xl text-[15.5px] leading-relaxed text-slate md:col-span-6">{text}</p>
        </li>
      ))}
    </ul>
  );
}
