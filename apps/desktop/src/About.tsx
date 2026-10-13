// About Mosaïk, at the end of Settings: its version, what it promises about privacy, support, and
// the ways out of trouble.

import { useEffect, useState } from 'react';

import { type Link, openLicences, openLink, openLogs, request, rescueWindows } from './engine';
import { Button, Section, useSay } from './ui';

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
    <Section title="About Mosaïk">
      <div className="py-3">
        <p className="max-w-xl text-small text-[var(--muted)]">
          {version ? `Version ${version}. ` : ''}Free forever, in the public domain (CC0). No account, no tracking, no
          network requests: everything stays on this PC.
        </p>
        <div className="mt-3 flex flex-wrap gap-2">
          <Button tone="primary" onClick={link('patreon')}>
            Support on Patreon
          </Button>
          <Button onClick={link('site')}>Website</Button>
          <Button onClick={link('source')}>Source code</Button>
          <Button tone="quiet" onClick={link('privacy')}>
            Privacy
          </Button>
          <Button
            tone="quiet"
            onClick={() => {
              void openLicences().then((error) => {
                if (error) say(error, true);
              });
            }}
          >
            Open-source licences
          </Button>
        </div>
      </div>
      <div className="py-3">
        <p className="text-small text-[var(--muted)]">
          If something goes wrong, Rescue brings back every window Mosaïk hid.
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
  );
}
