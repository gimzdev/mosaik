import type { Metadata } from 'next';
import { site } from '@/lib/site';

export const metadata: Metadata = {
  title: 'Privacy and terms',
  description: 'Mosaïk sends nothing anywhere. How the app, its updates and this website handle your data, and the terms it is released under.',
};

export default function ConditionsPage() {
  const mail = <a href={`mailto:${site.email}`}>{site.email}</a>;
  const ext = (href: string, label: string) => (
    <a href={href} rel="noopener noreferrer">
      {label}
    </a>
  );
  return (
    <div className="page-x pb-24 pt-32 sm:pt-40">
      <h1 className="h1">Privacy and terms</h1>
      <p className="mt-5 text-slate">Last updated October 12, 2026.</p>
      <div className="prose-page mt-8 max-w-2xl">
        <h2>The app</h2>
        <p>
          Mosaïk makes no network requests. It has no telemetry, no account and no cloud: your workspaces, layouts and settings stay on your
          computer, in Mosaïk’s own folder.
        </p>
        <p>
          It doesn’t check for updates itself. The Microsoft Store installs and updates it, as it does every Store app, and the Download
          buttons on this site fetch Microsoft’s installer from get.microsoft.com. Microsoft’s{' '}
          {ext('https://privacy.microsoft.com/privacystatement', 'privacy statement')} covers both.
        </p>
        <p>
          The Support on Patreon links, here and in Mosaïk’s About page and tray menu, open {ext(site.patreon, 'Patreon')} in your browser. The
          app itself sends nothing to Patreon.
        </p>
        <p>If a release ever needs to send something, this page will say exactly what and why before that release ships.</p>
        <h2>This website</h2>
        <p>It sets no cookies and runs no analytics or advertising trackers.</p>
        <p>If you email {mail}, we receive your address and your message, and use them only to reply. We don’t sell or share them.</p>
        <h2>Licence</h2>
        <p>
          Mosaïk is released under CC0 1.0, a public-domain dedication. You can use, copy, change and distribute it for any purpose, with no
          attribution required. The open-source components it is built with keep their own licences, listed in the app’s About page.
        </p>
        <h2>Warranty</h2>
        <p>Mosaïk is provided as is, without warranty of any kind. Use it at your own risk.</p>
        <h2>Using it well</h2>
        <ul>
          <li>Don’t use Mosaïk for anything illegal.</li>
          <li>Be kind in issues, discussions and pull requests.</li>
        </ul>
        <h2>Questions</h2>
        <p>
          Write to {mail} or open an issue on{' '}
          <a href={site.repo} rel="noopener noreferrer">
            GitHub
          </a>
          .
        </p>
      </div>
    </div>
  );
}
