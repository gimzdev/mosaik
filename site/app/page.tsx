import { Stage, SwitcherArt, TabsArt, ZonesArt } from '@/components/desk';
import { Check, DownloadButton, PlatformList } from '@/components/ui';
import { site } from '@/lib/site';

const FEATURES = [
  {
    id: 'zones',
    title: 'Zones give every window a home',
    body: 'One click reads what you are working on: the window you work in gets the most room, a terminal goes under your editor, and the windows that help sit beside it, each zone as big as its window needs. Or pick one of ten layouts, or carve the screen into your own named areas. Before you click, Mosaïk shows where each window would go.',
    points: ['Zones belong to the project, not the monitor', 'Zones fill the screen, so no space is lost', 'Works across multiple displays'],
    Art: ZonesArt,
  },
  {
    id: 'tabs',
    title: 'Tabs for any app, not only the browser',
    body: 'Stack your editor, terminal and design tool into one frame and flip between them like browser tabs. Tabs dragged out of Chrome or Edge join in too. One window to find instead of twelve.',
    points: ['Group any windows into one tab strip', 'Tab order is remembered per workspace', 'Pull a tab out and it becomes a window again'],
    Art: TabsArt,
  },
  {
    id: 'layouts',
    title: 'Switch projects, not just windows',
    body: 'Save a workspace once and bring it back whenever you need it. Positions, sizes and tabs return together, so changing task takes a keystroke instead of ten minutes of rearranging.',
    points: ['Sets of the apps you use together, opened in one click', 'Saved on your computer, never in a cloud', 'Switch from the tray, the screen’s edge, or press Win+Alt+Space'],
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
          <div className="grid gap-5 lg:grid-cols-12 lg:items-end lg:gap-14">
            <h1 className="font-display text-[clamp(2.4rem,4.7vw,4.1rem)] font-bold leading-[1.03] tracking-[-0.035em] lg:col-span-7">
              Every window,
              <br />
              in its place.
            </h1>
            <p className="max-w-md text-[17px] leading-[1.6] text-slate lg:col-span-5">
              Mosaïk suggests sets of apps that go together and opens them in tidy views. You can make your own, too.
            </p>
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
            <p className="max-w-[28rem] text-[17px] leading-[1.6] text-white/70">
              Mosaïk is free and open source, and stays that way. If it earns its place on your desktop, you can support its work on Patreon.
            </p>
            <div className="mt-7 flex flex-wrap gap-3">
              <DownloadButton className="btn btn-saffron" />
              <a href={site.patreon} rel="noopener noreferrer" className="btn btn-line-light">
                Support on Patreon
              </a>
            </div>
          </div>
        </div>
      </section>
    </>
  );
}
