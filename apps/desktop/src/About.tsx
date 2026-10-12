// About Mosaïk: version, what it promises about privacy, support, and the ways out of trouble.

import { useEffect, useState } from 'react';

import { type Link, openLink, openLogs, request } from './engine';
import { errorMessage } from './protocol';
import { Button, Section, Status } from './ui';

export function About({ onWelcome }: { onWelcome: () => void }) {
  const [version, setVersion] = useState<string | null>(null);
  const [status, setStatus] = useState<{ text: string; error: boolean } | null>(null);

  useEffect(() => {
    void request({ cmd: 'hello', client: 'about' }).then((reply) => {
      if (reply.reply === 'hello') setVersion(reply.app_version);
    });
  }, []);

  const link = (l: Link) => () => {
    void openLink(l);
  };

  return (
    <article aria-labelledby="about-title">
      <h1 id="about-title" className="font-display text-3xl font-bold tracking-tight">
        Mosaïk
      </h1>
      <p className="mt-2 text-[var(--muted)]">
        {version ? `Version ${version}. ` : ''}Free forever, and in the public domain (CC0): no rights reserved.
      </p>

      <Section title="Your windows, your computer">
        <p className="py-3 text-[var(--muted)]">
          Mosaïk makes no network requests: no accounts, no tracking, no cloud. Your workspaces and settings are saved on
          this PC only; updates come through the Microsoft Store.
        </p>
      </Section>

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
        <div className="flex flex-col gap-1 py-3">
          <p className="text-[var(--muted)]">
            Every window Mosaïk hides (background tabs) can be brought back at once, even if Mosaïk stopped.
          </p>
          <div className="mt-2 flex flex-wrap gap-2">
            <Button
              onClick={() => {
                void request({ cmd: 'rescue' }).then((reply) => {
                  setStatus(
                    reply.reply === 'done'
                      ? { text: reply.message, error: false }
                      : { text: errorMessage(reply) ?? 'Rescue did not finish.', error: true },
                  );
                });
              }}
            >
              Rescue windows
            </Button>
            <Button
              tone="quiet"
              onClick={() => {
                void openLogs();
              }}
            >
              Open the logs folder
            </Button>
            <Button tone="quiet" onClick={onWelcome}>
              Show the welcome again
            </Button>
          </div>
          <Status text={status?.text ?? null} error={status?.error ?? false} />
        </div>
      </Section>
    </article>
  );
}
