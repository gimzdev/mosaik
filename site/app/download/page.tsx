import type { Metadata } from 'next';
import { Check, PlatformList } from '@/components/ui';
import { site } from '@/lib/site';

export const metadata: Metadata = {
  title: 'Early access',
  description: 'Mosaïk is in development. Get notified when the first build is ready.',
};

const MAIL = `mailto:${site.email}?${new URLSearchParams({ subject: 'Mosaïk early access', body: 'Hi, I’d like to hear when Mosaïk is ready.\n\nPlatform: \nWhat I do: ' }).toString().replace(/\+/g, '%20')}`;

const PLANNED = ['Window zones and saved layouts', 'Tabs for any app', 'One-click workspace switching', 'Multi-monitor support', 'Layouts stored on your computer'];

export default function DownloadPage() {
  return (
    <div className="page-x pb-24 pt-32 sm:pt-40">
      <div className="max-w-[calc(100%*8/12)] max-lg:max-w-none">
        <p className="mb-5 inline-flex items-center gap-2.5 rounded-full border border-grout bg-paper px-4 py-1.5 text-sm font-semibold">
          <span className="h-2.5 w-2.5 rounded-full bg-saffron" aria-hidden="true" />
          In development
        </p>
        <h1 className="h1">Mosaïk isn’t released yet.</h1>
        <p className="mt-6 max-w-xl text-lg text-slate">
          The Windows build comes first. Email us with your platform and what you do, and we’ll write when there is something to try.
        </p>
        <div className="mt-8 flex flex-wrap gap-3">
          <a href={MAIL} className="btn btn-lapis">
            Email for early access
          </a>
          <a href={site.repo} rel="noopener noreferrer" className="btn btn-line">
            View on GitHub
          </a>
        </div>
      </div>

      <section className="mt-24 grid gap-10 border-t border-grout pt-14 lg:grid-cols-12">
        <h2 className="h2 lg:col-span-5">Planned for the first release</h2>
        <ul className="space-y-3.5 text-[17px] lg:col-span-7">
          {PLANNED.map((t) => (
            <Check key={t} s={17} top={5}>
              {t}
            </Check>
          ))}
        </ul>
      </section>

      <section className="mt-20 border-t border-grout pt-14">
        <h2 className="h2 mb-10">Platforms</h2>
        <PlatformList />
      </section>
    </div>
  );
}
