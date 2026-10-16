// Changes made on a workspace's picture (Home's preview of it, and its own page), sent to Mosaïk
// at once and saved in the workspace (ADR 0031): an app put in another zone, or trading zones with
// the app it was dropped on (places, in its own zone: ADR 0032); its zones resized by the lines
// between them. One change at a time,
// in the order made: a resize reads the zones' revision, which the one before it changes.

import { MIN_ZONE } from './editor/model';
import { request } from './engine';
import { type ArrangePreview, type Command, type RatioRect, type Reply, type ZoneEdit, errorMessage } from './protocol';
import { type Dropped, areasOf } from './Screen';

/** What a change came to: Mosaïk's reply, and the words that say it. */
export interface Change {
  reply: Reply;
  text: string;
  done: boolean;
}

let queue: Promise<unknown> = Promise.resolve();

function inTurn(job: () => Promise<Change>): Promise<Change> {
  const run = queue.then(job, job);
  queue = run.catch(() => undefined);
  return run;
}

const refused = (message: string): Reply => ({ reply: 'error', code: 'bad_request', message, hint: null });

/** Why these zones cannot be saved, in words, or null: one too small, or two over each other on
 * one monitor (the editor's lines never make either; a picture out of date could). */
function unfit(zones: readonly ZoneEdit[]): string | null {
  const small = zones.find(
    (z) => z.area.right - z.area.left < MIN_ZONE - 1e-9 || z.area.bottom - z.area.top < MIN_ZONE - 1e-9,
  );
  if (small) return `Zone ${small.name} would be too small, so nothing was resized.`;
  for (const [i, a] of zones.entries()) {
    for (const b of zones.slice(i + 1)) {
      if (a.monitor !== b.monitor) continue;
      const w = Math.min(a.area.right, b.area.right) - Math.max(a.area.left, b.area.left);
      const h = Math.min(a.area.bottom, b.area.bottom) - Math.max(a.area.top, b.area.top);
      if (w > 1e-6 && h > 1e-6) return `Zones ${a.name} and ${b.name} would overlap, so nothing was resized.`;
    }
  }
  return null;
}

/** " of “Ship”" or " in “Ship”", where the page is not the workspace's own. */
const naming = (word: 'of' | 'in', name: string | null) => (name ? ` ${word} “${name}”` : '');

/** Puts an app of `workspace` where it was dropped on its picture: in the zone, or in the zone of
 * the app it was dropped on, which takes its place. */
export function moveApp(workspace: string, name: string | null, preview: ArrangePreview, d: Dropped): Promise<Change> {
  return inTurn(async () => {
    const zone = preview.zones[d.zone];
    const app = d.piece.app;
    if (zone?.id === undefined || app === null) {
      const text = 'This app could not be moved: Mosaïk did not say where it is.';
      return { reply: refused(text), text, done: false };
    }
    const swap = d.onto?.app ?? null;
    const command: Command =
      swap === null
        ? { cmd: 'move_app', workspace, app, zone: zone.id }
        : { cmd: 'move_app', workspace, app, zone: zone.id, swap_with: swap };
    const reply = await request(command);
    if (reply.reply !== 'apps') {
      return { reply, text: errorMessage(reply) ?? 'The workspace could not be changed.', done: false };
    }
    const text =
      d.onto && swap !== null
        ? d.from === d.zone
          ? `${d.piece.label} and ${d.onto.label} traded places in zone ${zone.name}${naming('of', name)}.`
          : `${d.piece.label} and ${d.onto.label} traded zones${naming('in', name)}.`
        : `${d.piece.label} goes in zone ${zone.name}${naming('of', name)}.`;
    return { reply, text, done: true };
  });
}

/** Saves `workspace`'s zones on the main screen as resized on its picture (`areas`, in the
 * picture's order). Only the edges moved change: the others keep their exact place, rather than
 * one read back from the picture's pixels. */
export function resizeZones(
  workspace: string,
  name: string | null,
  preview: ArrangePreview,
  areas: readonly RatioRect[],
): Promise<Change> {
  return inTurn(async () => {
    const read = await request({ cmd: 'zones', workspace });
    if (read.reply !== 'zones') {
      return { reply: read, text: errorMessage(read) ?? 'Its zones could not be read.', done: false };
    }
    const was = areasOf(preview);
    const pick = (to: number, from: number, kept: number) => (Math.abs(to - from) > 1e-9 ? to : kept);
    const zones = read.zones.map((z) => {
      const i = preview.zones.findIndex((p) => p.id !== undefined && p.id === z.id);
      const to = areas[i];
      const from = was[i];
      if (!to || !from) return z;
      return {
        ...z,
        area: {
          left: pick(to.left, from.left, z.area.left),
          top: pick(to.top, from.top, z.area.top),
          right: pick(to.right, from.right, z.area.right),
          bottom: pick(to.bottom, from.bottom, z.area.bottom),
        },
      };
    });
    const problem = unfit(zones);
    if (problem !== null) return { reply: refused(problem), text: problem, done: false };
    const reply = await request({ cmd: 'set_zones', workspace, zones, revision: read.revision });
    if (reply.reply !== 'zones') {
      return { reply, text: errorMessage(reply) ?? 'The zones could not be resized.', done: false };
    }
    return { reply, text: name ? `Zones of “${name}” resized.` : 'Zones resized.', done: true };
  });
}
