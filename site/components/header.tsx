'use client';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { DownloadButton, Wordmark } from '@/components/ui';

const NAV = [
  ['/#how', 'How it works'],
  ['/#platforms', 'Platforms'],
];

export function Header() {
  const home = usePathname() === '/';
  return (
    <header className="fixed inset-x-0 top-0 z-50 border-b border-grout/80 bg-plaster/85 backdrop-blur-md">
      <div className="page-x flex h-16 items-center justify-between">
        <Link
          href="/"
          aria-label="Mosaïk, home"
          className="text-ink"
          onClick={(e) => {
            if (home) {
              e.preventDefault();
              history.replaceState(null, '', '/');
              scrollTo({ top: 0 });
            }
          }}
        >
          <Wordmark className="block h-7 w-auto" decorative />
        </Link>
        <nav aria-label="Main" className="hidden items-center gap-8 md:flex">
          {NAV.map(([href, label]) => (
            <Link key={href} href={href} className="text-[15px] font-medium text-slate transition-colors hover:text-ink">
              {label}
            </Link>
          ))}
        </nav>
        <DownloadButton className="btn btn-lapis min-h-[40px] px-5 text-sm" label="Download" />
      </div>
    </header>
  );
}
