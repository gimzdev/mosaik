// About Mosaïk: version, what it promises about privacy, support, and the ways out of trouble.

import { useEffect, useState } from 'react';

import { type Link, openLink, openLogs, request, rescueWindows } from './engine';
import { Button, PageTitle, Section, useSay } from './ui';

export function About({ onWelcome }: { onWelcome: () => void }) {
  const [version, setVersion] = useState<string | null>(null);
  const [rescuing, setRescuing] = useState(false);
  const say = useSay();

  useEffect(() => {
    let live = true;
    void request({ cmd: 'hello', client: 'about' }).then((reply) => {
      if (live && reply.reply === 'hello') setVersion(reply.app_version);
    });
    return () => {
      live = false;
    };
  }, []);

  const link = (l: Link) => () => {
    void openLink(l).then((error) => {
      if (error) say(error, true);
    });
  };

  return (
    <article aria-labelledby="about-title">
      <PageTitle id="about-title">Mosaïk</PageTitle>
      <p className="mt-2 text-[var(--muted)]">
        {version ? `Version ${version}. ` : ''}Free forever, and in the public domain (CC0): no rights reserved.
      </p>

      <div className="mt-6">
        <Section title="Your windows, your computer">
          <div className="flex flex-wrap items-center justify-between gap-3 py-3">
            <p className="max-w-lg text-small text-[var(--muted)]">
              Mosaïk makes no network requests: no account, no tracking, no cloud. Your workspaces and settings stay on
              this PC; updates come through the Microsoft Store.
            </p>
            <Button tone="quiet" onClick={link('privacy')}>
              Privacy
            </Button>
          </div>
        </Section>
      </div>

      <Section
        title="Support Mosaïk"
        intro="Mosaïk is free and stays free. If it helps you, you can support its work on Patreon."
      >
        <div className="flex flex-wrap gap-2 py-3">
          <Button tone="primary" onClick={link('patreon')}>
            Support on Patreon
          </Button>
          <Button onClick={link('site')}>Website</Button>
          <Button onClick={link('source')}>Source code</Button>
        </div>
      </Section>

      <Section title="If something goes wrong">
        <div className="py-3">
          <p className="text-small text-[var(--muted)]">
            Every window Mosaïk hides (tabs not shown) comes back at once, even if Mosaïk stopped answering.
          </p>
          <div className="mt-3 flex flex-wrap gap-2">
            <Button
              pending={rescuing}
              onClick={() => {
                setRescuing(true);
                void rescueWindows().then(({ text, error }) => {
                  setRescuing(false);
                  say(text, error);
                });
              }}
            >
              Rescue windows
            </Button>
            <Button
              tone="quiet"
              onClick={() => {
                void openLogs().then((error) => {
                  if (error) say(error, true);
                });
              }}
            >
              Open the logs folder
            </Button>
            <Button tone="quiet" onClick={onWelcome}>
              Show the welcome again
            </Button>
          </div>
        </div>
      </Section>
    </article>
  );
}
