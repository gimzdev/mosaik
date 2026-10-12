import type { Metadata } from 'next';
import { Check, DownloadButton, Keys, PlatformList } from '@/components/ui';
import { site } from '@/lib/site';

export const metadata: Metadata = {
  title: 'Download for Windows',
  description: 'Download Mosaïk for Windows 11 and Windows 10: free, installed and kept up to date by the Microsoft Store.',
};

/* The same three steps as the welcome page in the app. */
const STEPS: [string, React.ReactNode][] = [
  ['Arrange', 'Open the apps you use for one project and place them side by side.'],
  [
    'Save',
    <>
      Press <Keys k="Win Alt Space" /> then <Keys k="S" /> and name it. Each place becomes a zone with its own colour.
    </>,
  ],
  [
    'Switch',
    <>
      Press <Keys k="Win Alt Space" /> and pick a workspace: its windows come back and missing apps start.
    </>,
  ],
];

const INSIDE = [
  'Window zones and saved layouts',
  'Tabs for any app',
  'One-click switching from the tray or the keyboard',
  'Multiple monitors, including different scaling',
  'Layouts stored on your computer only',
];

const NOTES: [string, React.ReactNode][] = [
  [
    'Why the Microsoft Store',
    'Windows trusts what the Store installs, so there is no warning about an unknown publisher, and the Store keeps Mosaïk up to date. Mosaïk itself never connects to the internet.',
  ],
  ['Uninstalling', 'Settings, Apps, Installed apps, then Uninstall next to Mosaïk. Windows removes the app and the layouts it saved.'],
  [
    'The source',
    <>
      Every line is on{' '}
      <a href={site.repo} rel="noopener noreferrer" className="font-semibold text-lapis underline underline-offset-4">
        GitHub
      </a>
      , in the public domain (CC0).
    </>,
  ],
];

const Row = ({ title, children }: { title: string; children: React.ReactNode }) => (
  <section className="mt-20 grid gap-10 border-t border-grout pt-14 lg:grid-cols-12">
    <h2 className="h2 lg:col-span-5">{title}</h2>
    <div className="lg:col-span-7">{children}</div>
  </section>
);

export default function DownloadPage() {
  return (
    <div className="page-x pb-24 pt-32 sm:pt-40">
      <div className="max-w-[calc(100%*8/12)] max-lg:max-w-none">
        <p className="mb-5 inline-flex items-center gap-2.5 rounded-full border border-grout bg-paper px-4 py-1.5 text-sm font-semibold">
          <span className="h-2.5 w-2.5 rounded-full bg-verdigris" aria-hidden="true" />
          Free, for Windows 11 and Windows 10
        </p>
        <h1 className="h1">Download Mosaïk for Windows.</h1>
        <p className="mt-6 max-w-xl text-lg text-slate">One click gets Microsoft’s installer. It installs Mosaïk from the Microsoft Store, then starts it.</p>
        <div className="mt-8 flex flex-wrap gap-3">
          <DownloadButton className="btn btn-lapis" />
          <a href={site.storePage} rel="noopener noreferrer" className="btn btn-line">
            See it in the Microsoft Store
          </a>
        </div>
        <p className="mt-4 text-sm text-slate">Windows 11, or Windows 10 version 22H2, on a 64-bit (x64) PC.</p>
      </div>

      <Row title="Your first workspace in three steps">
        <ol className="space-y-7">
          {STEPS.map(([title, body], i) => (
            <li key={title} className="flex gap-4">
              <span className="grid h-9 w-9 shrink-0 place-items-center rounded-full bg-ink font-display font-bold text-paper" aria-hidden="true">
                {i + 1}
              </span>
              <div>
                <h3 className="font-display text-xl font-bold tracking-[-0.02em]">{title}</h3>
                <p className="mt-1 text-[17px] leading-relaxed text-slate">{body}</p>
              </div>
            </li>
          ))}
        </ol>
      </Row>

      <Row title="What’s inside">
        <ul className="space-y-3.5 text-[17px]">
          {INSIDE.map((t) => (
            <Check key={t} s={17} top={5}>
              {t}
            </Check>
          ))}
        </ul>
      </Row>

      <Row title="Good to know">
        <dl className="space-y-6">
          {NOTES.map(([t, d]) => (
            <div key={t}>
              <dt className="font-semibold">{t}</dt>
              <dd className="mt-1 text-[17px] leading-relaxed text-slate">{d}</dd>
            </div>
          ))}
        </dl>
      </Row>

      <section className="mt-20 border-t border-grout pt-14">
        <h2 className="h2 mb-10">Platforms</h2>
        <PlatformList />
      </section>
    </div>
  );
}
