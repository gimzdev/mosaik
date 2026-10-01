import type { Metadata, Viewport } from 'next';
import localFont from 'next/font/local';
import { Footer, Header, Sprite } from '@/components/ui';
import { site } from '@/lib/site';
import './globals.css';

// Self-hosted variable fonts (latin), preloaded with metric-matched fallbacks so nothing shifts while they load.
const display = localFont({ src: '../node_modules/@fontsource-variable/bricolage-grotesque/files/bricolage-grotesque-latin-wght-normal.woff2', weight: '200 800', variable: '--ff-display' });
const sans = localFont({ src: '../node_modules/@fontsource-variable/instrument-sans/files/instrument-sans-latin-wght-normal.woff2', weight: '400 700', variable: '--ff-sans' });
const mono = localFont({ src: '../node_modules/@fontsource-variable/jetbrains-mono/files/jetbrains-mono-latin-wght-normal.woff2', weight: '100 800', variable: '--ff-mono', preload: false });

const description = 'Mosaïk gives every project its own workspace: your apps and windows arranged and ready in one click. Free and open source.';

export const metadata: Metadata = {
  metadataBase: new URL(site.url),
  title: { default: `${site.name} | Workspace Organizer`, template: `%s | ${site.name}` },
  description,
  openGraph: { type: 'website', siteName: site.name, title: `${site.name} | Workspace Organizer`, description },
  twitter: { card: 'summary', title: `${site.name} | Workspace Organizer`, description },
};

export const viewport: Viewport = { themeColor: '#EDEFEA' };

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={`${display.variable} ${sans.variable} ${mono.variable}`}>
      <body>
        <Sprite />
        <a href="#content" className="sr-only focus:not-sr-only focus:fixed focus:left-4 focus:top-4 focus:z-[60] focus:rounded-full focus:bg-ink focus:px-5 focus:py-3 focus:text-paper">
          Skip to content
        </a>
        <Header />
        <main id="content">{children}</main>
        <Footer />
      </body>
    </html>
  );
}
