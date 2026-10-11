import Link from 'next/link';
import { DeskThumb, Stage, SwitcherArt, TabsArt, ZonesArt } from '@/components/desk';
import { Check, PlatformList } from '@/components/ui';
import { WORKSPACES } from '@/lib/data';
import { site } from '@/lib/site';

const FEATURES = [
  {
    id: 'zones',
    title: 'Zones give every window a home',
    body: 'Carve the screen into named areas, like Code, Terminal and Review. Drag a window near one and Mosaïk pulls it in. Open a project and everything lands where you left it.',
    points: ['Zones belong to the project, not the monitor', 'Every window wears its zone’s colour, so you can tell at a glance', 'Works across multiple displays'],
    Art: ZonesArt,
  },
  {
    id: 'tabs',
    title: 'Tabs for any app, not only the browser',
    body: 'Stack your editor, terminal and design tool into one frame and flip between them like browser tabs. One window to find instead of twelve.',
    points: ['Group any windows into one tab strip', 'Tab order is remembered per workspace', 'Pull a tab out and it becomes a window again'],
    Art: TabsArt,
  },
  {
    id: 'layouts',
    title: 'Switch projects, not just windows',
    body: 'Save a workspace once and bring it back whenever you need it. Positions, sizes and tabs return together, so changing task takes a keystroke instead of ten minutes of rearranging.',
    points: ['One workspace per project, role or mood', 'Saved on your computer, never in a cloud', 'Switch from the dock or the keyboard'],
    Art: SwitcherArt,
  },
];

const Intro = ({ title, children, className = '' }: { title: string; children: React.ReactNode; className?: string }) => (
  <div className={`grid gap-5 lg:grid-cols-12 lg:items-end lg:gap-14 ${className}`}>
    <h2 className="h2 lg:col-span-7">{title}</h2>
    <p className="max-w-md text-[17px] leading-[1.6] text-slate lg:col-span-5">{children}</p>
  </div>
);

export default function Home() {
  return (
    <>
      <section className="pb-20 pt-28 sm:pt-32 md:pb-28">
        <div className="page-x">
          <div className="grid gap-7 lg:grid-cols-12 lg:items-end lg:gap-14">
            <h1 className="font-display text-[clamp(2.4rem,4.7vw,4.1rem)] font-bold leading-[1.03] tracking-[-0.035em] lg:col-span-7">
              Every window,
              <br />
              in its place.
            </h1>
            <div className="lg:col-span-5 lg:pb-2">
              <p className="max-w-[30rem] text-[18px] leading-[1.45] text-slate sm:text-[19px]">
                Every project gets its own workspace: your apps and windows arranged and ready in one click.
              </p>
              <p className="mt-2 text-[15px] text-slate">Free and open source.</p>
            </div>
          </div>
          <div className="mt-10 md:mt-12">
            <Stage />
          </div>
        </div>
      </section>

      <section id="how" className="scroll-mt-16 border-t border-grout py-20 md:py-28">
        <div className="page-x">
          <h2 className="h2 max-w-3xl">Three ideas that tidy a whole desktop.</h2>
          <div className="mt-14 space-y-20 md:mt-20 md:space-y-28">
            {FEATURES.map(({ id, title, body, points, Art }, i) => (
              <article key={id} className="grid items-center gap-8 lg:grid-cols-12 lg:gap-14">
                <div className={`lg:col-span-5 ${i % 2 ? 'lg:order-2' : ''}`}>
                  <h3 className="font-display text-[clamp(1.5rem,2.4vw,2rem)] font-bold leading-[1.1] tracking-[-0.025em] text-balance">{title}</h3>
                  <p className="mt-4 text-[17px] leading-[1.6] text-slate">{body}</p>
                  <ul className="mt-6 space-y-3 text-[15px]">
                    {points.map((p) => (
                      <Check key={p}>{p}</Check>
                    ))}
                  </ul>
                </div>
                <div className={`lg:col-span-7 ${i % 2 ? 'lg:order-1' : ''}`}>
                  <Art />
                </div>
              </article>
            ))}
          </div>
        </div>
      </section>

      <section id="people" className="scroll-mt-16 border-t border-grout py-20 md:py-28">
        <div className="page-x">
          <Intro title="One desktop, many kinds of work.">A workspace is whatever you need open together. These four are the ones we kept reaching for.</Intro>
          <div className="mt-12 grid gap-x-8 gap-y-12 sm:grid-cols-2">
            {WORKSPACES.map((w, i) => (
              <div key={w.id}>
                <div className="overflow-hidden rounded-[16px] ring-1 ring-black/10">
                  <DeskThumb index={i} />
                </div>
                <div className="mt-4 flex items-baseline justify-between gap-4">
                  <h3 className="font-display text-[1.35rem] font-bold tracking-[-0.02em]">{w.name}</h3>
                  <p className="shrink-0 text-sm font-semibold text-lapis">{w.role}</p>
                </div>
                <p className="mt-1.5 max-w-md text-[15.5px] leading-relaxed text-slate">{w.about}</p>
              </div>
            ))}
          </div>
        </div>
      </section>

      <section id="platforms" className="scroll-mt-16 border-t border-grout py-20 md:py-28">
        <div className="page-x">
          <Intro title="Desktop first, because that’s where windows live." className="mb-10">
            Only a native app can move other apps’ windows, and each operating system allows a different amount. Here is the plan.
          </Intro>
          <PlatformList />
        </div>
      </section>

      <section className="on-ink bg-ink py-24 text-paper md:py-28">
        <div className="page-x grid gap-8 lg:grid-cols-12 lg:items-end lg:gap-14">
          <h2 className="font-display text-[clamp(2.2rem,4.4vw,3.6rem)] font-bold leading-[1.04] tracking-[-0.035em] lg:col-span-7">Put your work back together.</h2>
          <div className="lg:col-span-5">
            <p className="max-w-[28rem] text-[17px] leading-[1.6] text-white/70">Mosaïk is free and open source.</p>
            <div className="mt-7 flex flex-wrap gap-3">
              <Link href="/download" className="btn btn-saffron">
                Get early access
              </Link>
              <a href={site.repo} rel="noopener noreferrer" className="btn btn-line-light">
                View on GitHub
              </a>
            </div>
          </div>
        </div>
      </section>
    </>
  );
}
