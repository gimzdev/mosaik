import type { Metadata } from 'next';
import { DownloadButton } from '@/components/ui';
import { site } from '@/lib/site';

export const metadata: Metadata = {
  title: 'Download for Windows',
  description: 'Download Mosaïk for Windows 11 and Windows 10: free, installed and kept up to date by the Microsoft Store.',
};

const NOTES: [string, React.ReactNode][] = [
  ['Requirements', 'Windows 11, or Windows 10 version 22H2, on a 64-bit (x64) PC.'],
  ['Privacy', 'Mosaïk never connects to the internet. Your layouts stay on your PC.'],
  ['Uninstall', 'Settings, Apps, Installed apps, then Uninstall next to Mosaïk.'],
  [
    'Source',
    <>
      On{' '}
      <a href={site.repo} rel="noopener noreferrer" className="font-semibold text-lapis underline underline-offset-4">
        GitHub
      </a>
      , in the public domain (CC0).
    </>,
  ],
];

export default function DownloadPage() {
  return (
    <div className="page-x pb-24 pt-32 sm:pt-40">
      <h1 className="h1">Download Mosaïk for Windows.</h1>
      <p className="mt-5 max-w-xl text-lg text-slate">
        Free. The download is Microsoft’s small Store installer: it installs Mosaïk from the Microsoft Store, which keeps it up to date.
      </p>
      <div className="mt-8 flex flex-wrap gap-3">
        <DownloadButton className="btn btn-lapis" />
        <a href={site.storePage} rel="noopener noreferrer" className="btn btn-line">
          See it in the Microsoft Store
        </a>
      </div>
      <dl className="mt-16 grid max-w-3xl gap-x-10 gap-y-5 border-t border-grout pt-10 sm:grid-cols-[9rem_1fr]">
        {NOTES.map(([t, d]) => (
          <div key={t} className="contents">
            <dt className="font-semibold">{t}</dt>
            <dd className="-mt-4 text-[16px] leading-relaxed text-slate sm:mt-0">{d}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
