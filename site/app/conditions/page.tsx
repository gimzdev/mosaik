import type { Metadata } from 'next';
import { site } from '@/lib/site';

export const metadata: Metadata = {
  title: 'Privacy and terms',
  description: 'How Mosaïk handles your data, and the terms it is released under.',
};

export default function ConditionsPage() {
  const mail = <a href={`mailto:${site.email}`}>{site.email}</a>;
  return (
    <div className="page-x pb-24 pt-32 sm:pt-40">
      <h1 className="h1">Privacy and terms</h1>
      <p className="mt-5 text-slate">Last updated October 1, 2026.</p>
      <div className="prose-page mt-8 max-w-2xl">
        <h2>Privacy</h2>
        <p>This website sets no cookies and runs no analytics or advertising trackers.</p>
        <p>
          If you email {mail}, we receive your address and your message. We use them to reply and, if you ask for early access, to tell you
          when a build is ready. We don’t sell or share them.
        </p>
        <p>
          Mosaïk is designed to keep your workspaces on your own computer. Before any release adds a feature that sends data anywhere, this page
          will say exactly what is sent and why.
        </p>
        <h2>License</h2>
        <p>
          Mosaïk is released under CC0 1.0, a public-domain dedication. You can use, copy, change and distribute it for any purpose, with no
          attribution required.
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
