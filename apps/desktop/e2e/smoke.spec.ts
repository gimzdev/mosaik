import AxeBuilder from '@axe-core/playwright';
import { type Locator, type Page, expect, test } from '@playwright/test';

/** No accessibility problems that axe rates serious or critical (WCAG 2.1 A and AA). */
async function accessible(page: Page) {
  const results = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze();
  const bad = results.violations.filter((v) => v.impact === 'serious' || v.impact === 'critical');
  expect(bad.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(' ')).join(', ')}`)).toEqual([]);
}

/** The window never scrolls as a whole; only the page inside it does. */
async function windowDoesNotScroll(page: Page) {
  const overflow = await page.evaluate(() => document.documentElement.scrollHeight - window.innerHeight);
  expect(overflow).toBeLessThanOrEqual(0);
}

/** Words shown on the page (the announcer says the same words to screen readers, outside it). */
const shown = (page: Page, text: string) => page.locator('main').getByText(text, { exact: true });

/** The stand-in's screen as the automatic layout would leave it. */
const AUTO = 'Visual Studio Code in zone 1, Google Chrome in zone 2, Windows Terminal in zone 4, Slack in zone 3.';
const arrange = (page: Page) => page.getByRole('button', { name: 'Move windows', exact: true });
const HINT =
  'Drag a window into a zone to add it there, onto another window to swap them, or drag a line to resize zones.';

/** Where a bar of a picture is once it has stopped moving: zones glide to a new layout. */
async function still(page: Page, l: Locator) {
  let last = await l.boundingBox();
  for (let tries = 0; tries < 40; tries += 1) {
    await page.waitForTimeout(60);
    const now = await l.boundingBox();
    if (now && now.x === last?.x && now.y === last.y && now.width === last.width) return now;
    last = now;
  }
  throw new Error('it never stopped moving');
}

/** Drags one bar or tab of a picture onto another, as a person does: a press, a few pixels to
 * start the drag, then the rest of the way, to the point of `to` at fractions `fx` and `fy` of its
 * width and height (its middle unless said). `from` is brought into view first, as a person
 * scrolls to what they take. */
async function dragOnto(page: Page, from: Locator, to: Locator, fx = 0.5, fy = 0.5) {
  await from.scrollIntoViewIfNeeded();
  const [a, b] = [await still(page, from), await still(page, to)];
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(a.x + a.width / 2 + 10, a.y + a.height / 2 + 6, { steps: 4 });
  await page.mouse.move(b.x + b.width * fx, b.y + b.height * fy, { steps: 12 });
  await page.mouse.up();
}

/** Drags a bar of a picture into the zone another bar is in, below that bar: onto the zone itself. */
async function dragInto(page: Page, from: Locator, there: Locator) {
  const a = await still(page, from);
  const zone = await there.locator('xpath=ancestor::div[contains(@class, "stage-window")]').boundingBox();
  if (!zone) throw new Error('the zone is not drawn');
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(a.x + a.width / 2 + 10, a.y + a.height / 2 + 6, { steps: 4 });
  await page.mouse.move(zone.x + zone.width / 2, zone.y + zone.height * 0.75, { steps: 12 });
  await page.mouse.up();
}

/** The colour of the zone a window's bar is in, as the chip with the zone's number shows it. */
const zoneColour = (bar: Locator) =>
  bar.evaluate((el) => {
    const chip = el.closest('.stage-window')?.querySelector('.zone-chip');
    return chip ? getComputedStyle(chip).backgroundColor : null;
  });

/** Opens a workspace's page from its card's menu on Home. */
async function editWorkspace(page: Page, name: string) {
  await page.getByRole('button', { name: `More for “${name}”` }).click();
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  await expect(page.getByRole('heading', { name, level: 1 })).toBeVisible();
}

/** Opens the zone editor of the stand-in's first workspace, from its card's menu. */
async function editZones(page: Page) {
  await page.getByRole('button', { name: 'More for “Website”' }).click();
  await page.getByRole('button', { name: 'Edit zones' }).click();
  await expect(page.getByRole('heading', { name: 'Website', level: 1 })).toBeVisible();
}

test.describe('Home', () => {
  test('shows the automatic layout for the screen, Arrange and the layouts, and is accessible', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
    await expect(shown(page, 'Automatic layout for coding. Nothing moves yet.')).toBeVisible();
    await expect(shown(page, HINT)).toBeVisible();
    const layouts = page.getByRole('region', { name: 'Your screen' }).getByRole('radio');
    await expect(layouts).toHaveCount(9);
    await expect(page.getByRole('radio', { name: 'Automatic' })).toBeChecked();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    await accessible(page);
    await windowDoesNotScroll(page);
  });

  test('the layouts are all in view, in two rows beside Automatic, and a click anywhere on one chooses it', async ({
    page,
  }) => {
    // The owner: "we dont want dropdown menu", "9 left fitting in 2 lines of options".
    await page.goto('/');
    await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
    const group = page.getByRole('group', { name: 'Layout' });
    await expect(group.getByRole('button')).toHaveCount(0);
    // Automatic as tall as the two rows; the eight others in two rows of four.
    const boxes = await Promise.all(
      (await group.locator('label').all()).map(async (l) => {
        const b = await l.boundingBox();
        if (!b) throw new Error('a layout is not drawn');
        return b;
      }),
    );
    const [auto, ...others] = boxes;
    if (!auto) throw new Error('Automatic is not drawn');
    expect(others).toHaveLength(8);
    expect(new Set(others.map((b) => Math.round(b.y))).size).toBe(2);
    expect(auto.height).toBeGreaterThan(Math.max(...others.map((b) => b.height)) * 1.8);
    await expect(group.getByText('Three rows')).toHaveCount(0);
    await expect(group.getByText('Top + bottom')).toHaveCount(0);
    // A click anywhere on it, its picture or its name, chooses it: its radio button covers it.
    await group
      .locator('label')
      .filter({ hasText: 'Stack + half' })
      .click({ position: { x: 12, y: 12 } });
    await expect(group.getByRole('radio', { name: 'Stack + half' })).toBeChecked();
    await expect(shown(page, 'Stack + half. Nothing moves yet.')).toBeVisible();
    await accessible(page);
  });

  test('a layout shows where the windows would go; Arrange puts them there; Undo puts them back', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
    await page.getByRole('radio', { name: 'Thirds' }).check();
    // Four windows for three zones: the fourth stacks in one rather than go to the taskbar.
    await expect(
      page.getByRole('group', {
        name: 'Visual Studio Code in zone 2, Google Chrome in zone 1, Windows Terminal in zone 3, Slack in zone 2.',
      }),
    ).toBeVisible();
    await expect(page.getByText(/will be minimized/)).toHaveCount(0);
    await arrange(page).click();
    await expect(shown(page, 'Moved 4 windows into Thirds.')).toBeVisible();
    await page.getByRole('button', { name: 'Undo' }).click();
    await expect(shown(page, 'Windows are back where they were.')).toBeVisible();
    await accessible(page);
  });

  test('Arrange lays the screen out for its task, and can be undone', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
    await arrange(page).click();
    await expect(shown(page, 'Moved 4 windows.')).toBeVisible();
    await expect(shown(page, 'Your windows are already like this.')).toBeHidden();
    await page.getByRole('button', { name: 'Undo' }).click();
    await expect(shown(page, 'Windows are back where they were.')).toBeVisible();
  });

  test('a window dragged onto another trades places with it, and Arrange puts them so', async ({ page }) => {
    // The owner: "we should be able to drag drop apps to permute them".
    await page.goto('/');
    await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
    // Picked by where they start: the names say their zones, which change as they move.
    const code = page.getByRole('button', { name: /^Visual Studio Code in zone/ });
    const browser = page.getByRole('button', { name: /^Google Chrome in zone/ });
    const [a, b] = [await code.boundingBox(), await browser.boundingBox()];
    if (!a || !b) throw new Error('the windows are not drawn');
    const colours = [await zoneColour(code), await zoneColour(browser)];
    expect(colours[0]).not.toEqual(colours[1]);
    await page.mouse.move(a.x + a.width / 2, a.y + 10);
    await page.mouse.down();
    await page.mouse.move(a.x + a.width / 2 + 20, a.y + 20, { steps: 4 });
    await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2, { steps: 12 });
    await page.mouse.up();
    // Each lands in the other's place, with its colour; the zones keep their numbers.
    await expect
      .poll(async () => {
        const [c, d] = [await code.boundingBox(), await browser.boundingBox()];
        return c && d ? Math.abs(c.x - b.x) < 3 && Math.abs(d.x - a.x) < 3 && Math.abs(c.y - b.y) < 3 : false;
      })
      .toBe(true);
    await expect(page.getByRole('button', { name: 'Undo changes' })).toBeVisible();
    expect([await zoneColour(code), await zoneColour(browser)]).toEqual(colours);
    await accessible(page);
    await arrange(page).click();
    await expect(shown(page, 'Moved 4 windows.')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Undo changes' })).toBeHidden();
    // Arranged as they were put: the picture shows them there now, each in the colour it wears
    // on screen (ADR 0032), and Move windows again moves nothing.
    const [c, d] = [await code.boundingBox(), await browser.boundingBox()];
    expect(c && Math.abs(c.x - b.x) < 3).toBe(true);
    expect(d && Math.abs(d.x - a.x) < 3).toBe(true);
    await expect.poll(async () => [await zoneColour(code), await zoneColour(browser)]).toEqual(colours);
    await arrange(page).click();
    await expect(
      page.getByRole('group', { name: /^Google Chrome in zone 1, Visual Studio Code in zone 2/ }),
    ).toBeVisible();
    expect([await zoneColour(code), await zoneColour(browser)]).toEqual(colours);
  });

  test('two windows of one zone trade places front to back, and Move windows puts them so', async ({ page }) => {
    // ADR 0032, the owner: "i should be able to permute chrome with claude". In Thirds, Slack
    // stacks behind Visual Studio Code in zone 2; dropped onto it, it comes to the front.
    await page.goto('/');
    await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
    await page.getByRole('radio', { name: 'Thirds' }).check();
    await expect(
      page.getByRole('group', {
        name: 'Visual Studio Code in zone 2, Google Chrome in zone 1, Windows Terminal in zone 3, Slack in zone 2.',
      }),
    ).toBeVisible();
    const code = page.getByRole('button', { name: 'Visual Studio Code in zone 2' });
    const slack = page.getByRole('button', { name: 'Slack in zone 2' });
    const [a, b] = [await still(page, code), await still(page, slack)];
    expect(a.y < b.y).toBe(true);
    await dragOnto(page, slack, code);
    await expect(
      page.getByRole('group', {
        name: 'Slack in zone 2, Google Chrome in zone 1, Windows Terminal in zone 3, Visual Studio Code in zone 2.',
      }),
    ).toBeVisible();
    // Slack's bar is now the first of the zone.
    await expect
      .poll(async () => {
        const [c, d] = [await code.boundingBox(), await slack.boundingBox()];
        return c && d ? d.y < c.y : false;
      })
      .toBe(true);
    await expect(page.getByRole('button', { name: 'Undo changes' })).toBeVisible();
    await arrange(page).click();
    await expect(shown(page, 'Moved 4 windows into Thirds.')).toBeVisible();
    // Done once: the picture shows the screen as it now is, Slack in front.
    await expect(page.getByRole('button', { name: 'Undo changes' })).toBeHidden();
    await expect(
      page.getByRole('group', {
        name: 'Slack in zone 2, Google Chrome in zone 1, Windows Terminal in zone 3, Visual Studio Code in zone 2.',
      }),
    ).toBeVisible();
    await accessible(page);
  });

  test('a window stacked behind another, dropped onto a window of another zone, trades places with exactly it', async ({
    page,
  }) => {
    // The review of 598f728: Slack, stacked behind Visual Studio Code in Thirds, dropped onto
    // Chrome: Slack takes Chrome's zone, and Chrome Slack's place, behind the editor.
    await page.goto('/');
    await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
    await page.getByRole('radio', { name: 'Thirds' }).check();
    const slack = page.getByRole('button', { name: 'Slack in zone 2' });
    await expect(slack).toBeVisible();
    await dragOnto(page, slack, page.getByRole('button', { name: 'Google Chrome in zone 1' }));
    await expect(
      page.getByRole('group', {
        name: 'Visual Studio Code in zone 2, Slack in zone 1, Windows Terminal in zone 3, Google Chrome in zone 2.',
      }),
    ).toBeVisible();
    await arrange(page).click();
    await expect(shown(page, 'Moved 4 windows into Thirds.')).toBeVisible();
    // Done, and kept so: the picture shows the screen as it now is.
    await expect(page.getByRole('button', { name: 'Undo changes' })).toBeHidden();
    await expect(
      page.getByRole('group', {
        name: 'Visual Studio Code in zone 2, Slack in zone 1, Windows Terminal in zone 3, Google Chrome in zone 2.',
      }),
    ).toBeVisible();
  });

  test('a window dragged into another zone joins it, behind its window, and Move windows puts it so', async ({
    page,
  }) => {
    // ADR 0033 (the owner: it "permutes with the app of welcoming zone instead of just adding
    // it"). In Thirds, Windows Terminal dragged into Chrome's zone, below Chrome's bar: it goes
    // behind Chrome, and its own zone is left empty; no other window moves.
    await page.goto('/');
    await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
    await page.getByRole('radio', { name: 'Thirds' }).check();
    const terminal = page.getByRole('button', { name: 'Windows Terminal in zone 3' });
    await expect(terminal).toBeVisible();
    await dragInto(page, terminal, page.getByRole('button', { name: 'Google Chrome in zone 1' }));
    const joined =
      'Visual Studio Code in zone 2, Google Chrome in zone 1, Slack in zone 2, Windows Terminal in zone 1. Zone 3 is empty.';
    await expect(page.getByRole('group', { name: joined })).toBeVisible();
    await arrange(page).click();
    await expect(shown(page, 'Moved 4 windows into Thirds.')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Undo changes' })).toBeHidden();
    await expect(page.getByRole('group', { name: joined })).toBeVisible();
  });

  test('a line between zones dragged resizes them; Arrange puts the windows so, and the picture keeps them', async ({
    page,
  }) => {
    // The owner: "for anim homescreen we should be able to stretch zones".
    await page.goto('/');
    await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
    const line = page.getByRole('separator', { name: 'Line right of zone 1 and zone 4, left of zone 2 and zone 3' });
    await expect(line).toHaveAttribute('aria-valuenow', '58');
    const box = await line.boundingBox();
    const picture = await page.getByRole('group', { name: AUTO }).boundingBox();
    if (!box || !picture) throw new Error('the line is not drawn');
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    // A tenth of the picture to the left.
    await page.mouse.move(box.x + box.width / 2 - picture.width / 10, box.y + box.height / 2, { steps: 8 });
    await page.mouse.up();
    await expect(line).toHaveAttribute('aria-valuenow', '48');
    // Every window stays in its zone (ADR 0033).
    await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Undo changes' })).toBeVisible();
    await arrange(page).click();
    await expect(shown(page, 'Moved 4 windows.')).toBeVisible();
    // The screen is as the picture showed it: nothing more to undo there.
    await expect(page.getByRole('button', { name: 'Undo changes' })).toBeHidden();
    await expect(line).toHaveAttribute('aria-valuenow', '48');
    await accessible(page);
  });

  test('the arrow keys go through the layouts, and the picture follows', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
    await page.getByRole('radio', { name: 'Automatic' }).focus();
    await page.keyboard.press('ArrowRight');
    await expect(page.getByRole('radio', { name: 'Halves' })).toBeChecked();
    await expect(page.getByRole('radio', { name: 'Halves' })).toBeFocused();
    await expect(
      page.getByRole('group', {
        name: 'Visual Studio Code in zone 1, Google Chrome in zone 2, Windows Terminal in zone 1, Slack in zone 2.',
      }),
    ).toBeVisible();
  });

  test('a workspace picked shows how it opens, and opens with Open', async ({ page }) => {
    // The owner: "we shouldnt open the workspace when clicking on it it should show it in the
    // anim as preview and show an open button".
    await page.goto('/');
    const list = page.getByRole('list', { name: 'Workspaces' });
    await expect(list.getByRole('button', { name: 'Website (on screen)', exact: true })).toBeVisible();
    await list.getByRole('button', { name: 'Portfolio', exact: true }).click();
    await expect(
      page.getByRole('group', {
        name: 'Figma starts in zone 1, Notepad starts in zone 2, Visual Studio Code minimized, Google Chrome minimized, Windows Terminal minimized, Slack minimized.',
      }),
    ).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Preview of “Portfolio”' })).toBeVisible();
    await expect(shown(page, 'Nothing moves until you press Open.')).toBeVisible();
    await accessible(page);
    // Clicked again, it stays; Esc, or Back, shows the screen again.
    await list.getByRole('button', { name: 'Portfolio', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Open “Portfolio”' })).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.getByRole('heading', { name: 'Your screen' })).toBeVisible();
    await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
    await list.getByRole('button', { name: 'Portfolio', exact: true }).click();
    await page.getByRole('button', { name: 'Back to my screen' }).click();
    await expect(arrange(page)).toBeVisible();
    await expect(list.getByRole('button', { name: 'Portfolio', exact: true })).toBeFocused();
    await list.getByRole('button', { name: 'Portfolio', exact: true }).click();
    await page.getByRole('button', { name: 'Open “Portfolio”' }).click();
    await expect(shown(page, 'Opened “Portfolio”.')).toBeVisible();
    await expect(list.getByRole('button', { name: 'Portfolio (on screen)', exact: true })).toBeVisible();
    await expect(arrange(page)).toBeVisible();
  });

  test('the workspace with the most apps keeps every app on every layout chosen for it, and Undo puts it back', async ({
    page,
  }) => {
    // The owner: "try coding for example theres 6 apps normally and going to quarter get rid of
    // some"; "lets try workspace with most apps, on all layouts" (ADR 0033).
    await page.goto('/');
    const list = page.getByRole('list', { name: 'Workspaces' });
    await list.getByRole('button', { name: 'Backend', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Preview of “Backend”' })).toBeVisible();
    const picture = page.getByRole('group', { name: /^Visual Studio Code/ });
    await expect(picture).toBeVisible();
    const before = await picture.getAttribute('aria-label');
    await expect(page.getByRole('radio', { name: 'Automatic' })).toBeDisabled();
    const apps = ['Visual Studio Code', 'Windows Terminal', 'File Explorer', 'MSYS2', 'Google Chrome', 'Claude'];
    for (const layout of [
      'Halves',
      'Thirds',
      'Main + side',
      'Side + main',
      'Half + stack',
      'Stack + half',
      'Quarters',
      'Full screen',
    ]) {
      const radio = page.getByRole('radio', { name: layout });
      // Its own layout already (Half + stack): nothing to do.
      const own = await radio.isChecked();
      if (!own) {
        // Checked once Mosaïk has laid it out, as its picture then shows.
        await radio.click();
        await expect(shown(page, `“Backend” is laid out in ${layout}.`)).toBeVisible();
        await expect(page.getByRole('heading', { name: 'Preview of “Backend”' })).toBeVisible();
        await expect(radio).toBeChecked();
      }
      const label = (await picture.getAttribute('aria-label')) ?? '';
      for (const app of apps) {
        expect(label, layout).toContain(app);
        expect(label, layout).not.toContain(`${app} minimized`);
      }
      if (own) continue;
      await page.getByRole('button', { name: 'Undo', exact: true }).click();
      await expect(shown(page, '“Backend” is back as it was.')).toBeVisible();
      await expect(picture).toHaveAttribute('aria-label', before ?? '');
    }
  });

  test('a double-click on a workspace opens it at once', async ({ page }) => {
    await page.goto('/');
    const list = page.getByRole('list', { name: 'Workspaces' });
    await list.getByRole('button', { name: 'Portfolio', exact: true }).dblclick();
    await expect(shown(page, 'Opened “Portfolio”.')).toBeVisible();
    await expect(list.getByRole('button', { name: 'Portfolio (on screen)', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Your screen' })).toBeVisible();
  });

  test('a suggested workspace shows what it starts, and starts it with Start these apps', async ({ page }) => {
    await page.goto('/');
    await page.getByRole('button', { name: /^Coding \(suggested\)/ }).click();
    await expect(page.getByRole('heading', { name: 'Preview of “Coding”' })).toBeVisible();
    await expect(
      shown(
        page,
        'Coding starts Visual Studio Code, Terminal, Google Chrome, Claude and File Explorer, and puts them in their zones.',
      ),
    ).toBeVisible();
    await expect(page.getByRole('list', { name: 'Apps of “Coding”' }).getByRole('listitem')).toHaveCount(5);
    await accessible(page);
    await page.getByRole('button', { name: 'Start these apps' }).click();
    await expect(shown(page, 'Opened “Coding”.')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Coding (on screen)', exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: /^Coding \(suggested\)/ })).toHaveCount(0);
  });

  test('what is on screen is kept as a new workspace, under a free name', async ({ page }) => {
    await page.goto('/');
    await page.getByRole('button', { name: 'Save as workspace' }).click();
    const name = page.getByRole('textbox', { name: 'Name the new workspace' });
    await expect(name).toHaveValue('Workspace 1');
    await expect(name).toBeFocused();
    await name.fill('Website');
    await page.keyboard.press('Enter');
    await expect(
      shown(
        page,
        '“Website” already exists. Pick another name, or open its ⋯ menu and choose Replace with what’s on screen.',
      ),
    ).toBeVisible();
    await name.fill('Portfolio');
    await page.keyboard.press('Enter');
    await expect(shown(page, '“Portfolio” already exists. Pick another name.')).toBeVisible();
    await name.fill('Review');
    await page.keyboard.press('Enter');
    // Said under the picture, as everything done on Home is.
    await expect(
      page.getByRole('region', { name: 'Your screen' }).getByText('Saved “Review”.', { exact: true }),
    ).toBeVisible();
    await expect(page.getByRole('button', { name: 'Review (on screen)', exact: true })).toBeVisible();
    await accessible(page);
  });

  test('the last workspace’s menu opens where its items can be seen', async ({ page }) => {
    await page.goto('/');
    await page.locator('main').evaluate((main) => {
      main.scrollTop = main.scrollHeight;
    });
    await page.getByRole('button', { name: 'More for “Portfolio”' }).click();
    for (const name of ['Edit', 'Edit zones']) {
      await expect(page.getByRole('button', { name, exact: true })).toBeInViewport({ ratio: 1 });
    }
    await accessible(page);
  });
});

test.describe('a workspace’s page', () => {
  test('is one width: its title and Open, its picture and its sections line up, in the middle', async ({ page }) => {
    // The owner, of the page with its picture narrower than the rest and on the left: "the view in
    // image two is disgraceful we should be more appealing symetrical" (ADR 0034).
    for (const size of [
      { width: 1000, height: 760 },
      { width: 1400, height: 1100 },
    ]) {
      await page.setViewportSize(size);
      await page.goto('/');
      await editWorkspace(page, 'Website');
      const picture = await still(page, page.getByRole('group', { name: /^Visual Studio Code in zone 1/ }));
      const edges = async (l: Locator) => {
        const b = await still(page, l);
        return [Math.round(b.x), Math.round(b.x + b.width)];
      };
      const wanted = [Math.round(picture.x), Math.round(picture.x + picture.width)];
      for (const name of ['Apps', 'Shortcut', 'Zones']) {
        expect(await edges(page.getByRole('region', { name })), `${name}, ${String(size.height)} high`).toEqual(wanted);
      }
      const [, open] = await edges(page.getByRole('button', { name: 'Open', exact: true }));
      expect(open, `Open, ${String(size.height)} high`).toBe(wanted[1]);
      const main = await page.locator('main').evaluate((m) => {
        const r = m.getBoundingClientRect();
        return { left: r.left, right: r.left + m.clientWidth };
      });
      const [left = 0, right = 0] = wanted;
      expect(
        Math.abs(left - main.left - (main.right - right)),
        `in the middle, ${String(size.height)} high`,
      ).toBeLessThanOrEqual(2);
    }
  });

  test('shows how it opens, lists its apps by zone, and moves, removes and adds them', async ({ page }) => {
    await page.goto('/');
    await editWorkspace(page, 'Website');
    await expect(
      page.getByRole('group', {
        name: 'Visual Studio Code in zone 1, Google Chrome in zone 2, Windows Terminal in zone 3 with Slack as tabs.',
      }),
    ).toBeVisible();
    const apps = page.getByRole('group', { name: 'Apps of “Website”' });
    await expect(apps.getByRole('listitem')).toHaveCount(4);
    await expect(apps.getByRole('group', { name: 'Zone 3', exact: true }).getByRole('listitem')).toHaveCount(2);
    await page.getByRole('button', { name: 'Move Slack to another zone' }).click();
    await page.getByRole('button', { name: 'Zone 2', exact: true }).click();
    await expect(shown(page, 'Slack goes in zone 2.')).toBeVisible();
    await expect(apps.getByRole('group', { name: 'Zone 2', exact: true }).getByText('Slack')).toBeVisible();
    await page.getByRole('button', { name: 'Remove Slack' }).click();
    await expect(apps.getByRole('listitem')).toHaveCount(3);
    await page.getByRole('button', { name: 'Add an app to “Website”' }).click();
    const slack = page.getByRole('group', { name: 'Apps to add' }).getByRole('button', { name: 'Slack', exact: true });
    await expect(slack).toBeInViewport({ ratio: 1 });
    await accessible(page);
    await slack.click();
    await expect(shown(page, 'Slack added.')).toBeVisible();
    await expect(apps.getByRole('listitem')).toHaveCount(4);
    // An app that is not open, found by its name (ADR 0033): it starts when the workspace opens.
    await page.getByRole('button', { name: 'Add an app to “Website”' }).click();
    await page.getByRole('searchbox', { name: 'Find an app' }).fill('spot');
    await page.getByRole('group', { name: 'Apps to add' }).getByRole('button', { name: 'Spotify' }).click();
    await expect(shown(page, 'Spotify added.')).toBeVisible();
    await expect(apps.getByRole('listitem')).toHaveCount(5);
    await accessible(page);
    await windowDoesNotScroll(page);
    await page.getByRole('button', { name: 'Back to Home' }).click();
    await expect(page.getByRole('heading', { name: 'Your screen' })).toBeVisible();
  });

  test('its apps are dragged in their list into a zone, which they join, or onto another app, trading places', async ({
    page,
  }) => {
    // The owner: "the apps in image 1 must be able to be dragged and droped from zones" (ADR 0033);
    // then, as an app dropped in a zone traded with the app there: "permute when specifically
    // dropped on another app otherwise just add", as on the picture (ADR 0034).
    await page.goto('/');
    await editWorkspace(page, 'Backend');
    const apps = page.getByRole('group', { name: 'Apps of “Backend”' });
    const zone = (n: string) => apps.getByRole('group', { name: `Zone ${n}`, exact: true });
    const row = (label: string) => apps.getByRole('listitem').filter({ hasText: label });
    const name = (label: string) => row(label).getByText(label, { exact: true });
    await expect(zone('3').getByText('Empty. Drag an app here, or use Move.')).toBeVisible();
    await dragOnto(page, row('MSYS2'), zone('3'));
    await expect(shown(page, 'MSYS2 goes in zone 3.')).toBeVisible();
    await expect(zone('3').getByText('MSYS2')).toBeVisible();
    // Into zone 3 over MSYS2's row, beside its name: added there, behind it.
    await dragOnto(page, row('Visual Studio Code'), row('MSYS2'), 0.55);
    await expect(shown(page, 'Visual Studio Code goes in zone 3.')).toBeVisible();
    await expect(zone('3').getByRole('listitem')).toHaveText([/MSYS2/, /Visual Studio Code/]);
    // Onto Google Chrome's name: the two trade zones.
    await dragOnto(page, row('Visual Studio Code'), name('Google Chrome'));
    await expect(shown(page, 'Visual Studio Code and Google Chrome traded zones.')).toBeVisible();
    await expect(zone('2').getByText('Visual Studio Code')).toBeVisible();
    await expect(zone('3').getByText('Google Chrome')).toBeVisible();
    // The picture follows.
    await expect(page.getByRole('group', { name: /Visual Studio Code in zone 2/ })).toBeVisible();
    await accessible(page);
  });

  test('its picture shows each zone once, empty ones too, and an app dragged there goes in it', async ({ page }) => {
    // The owner, of a workspace with two windows in one zone drawn over each other: "look at the
    // anim wtf is happening"; and "when a zone is empty we dont have either color or number".
    await page.goto('/');
    await editWorkspace(page, 'Backend');
    const picture = page.getByRole('group', {
      name: 'Visual Studio Code in zone 1, Windows Terminal in zone 1 with File Explorer as tabs, MSYS2 starts in zone 2, Google Chrome in zone 2 with Claude as tabs, Slack minimized. Zone 3 is empty.',
    });
    await expect(picture).toBeVisible();
    await expect(picture.getByText('Empty', { exact: true })).toBeVisible();
    // Every bar in its zone, one under the other: none drawn over another.
    const bars = await Promise.all(
      ['Visual Studio Code in zone 1', 'Windows Terminal, a tab in zone 1'].map(async (name) => {
        const b = await page.getByRole('button', { name }).boundingBox();
        if (!b) throw new Error(`${name} is not drawn`);
        return b;
      }),
    );
    const [first, second] = bars;
    expect(first && second && second.y >= first.y + first.height).toBe(true);
    // Claude dragged out of its group into the empty zone.
    const claude = page.getByRole('button', { name: 'Claude, a tab in zone 2' });
    const empty = await picture.getByText('Empty', { exact: true }).boundingBox();
    const from = await claude.boundingBox();
    if (!empty || !from) throw new Error('not drawn');
    await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
    await page.mouse.down();
    await page.mouse.move(empty.x + empty.width / 2, empty.y + empty.height / 2, { steps: 12 });
    await page.mouse.up();
    await expect(shown(page, 'Claude goes in zone 3.')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Claude starts in zone 3' })).toBeVisible();
    const apps = page.getByRole('group', { name: 'Apps of “Backend”' });
    await expect(apps.getByRole('group', { name: 'Zone 3', exact: true }).getByText('Claude')).toBeVisible();
    await accessible(page);
  });

  test('two tabs of one zone trade places in their strip, the tab showing staying so', async ({ page }) => {
    // ADR 0032, the owner: "i should be able to permute chrome with claude".
    await page.goto('/');
    await editWorkspace(page, 'Backend');
    const chrome = page.getByRole('button', { name: 'Google Chrome, a tab in zone 2' });
    const claude = page.getByRole('button', { name: 'Claude, a tab in zone 2' });
    const [a, b] = [await chrome.boundingBox(), await claude.boundingBox()];
    expect(a && b && a.x < b.x).toBe(true);
    await dragOnto(page, claude, chrome);
    await expect(shown(page, 'Claude and Google Chrome traded places in zone 2.')).toBeVisible();
    await expect
      .poll(async () => {
        const [c, d] = [await chrome.boundingBox(), await claude.boundingBox()];
        return c && d ? d.x < c.x : false;
      })
      .toBe(true);
    // Chrome still shows, Claude behind it; the list says the order too.
    await expect(page.getByRole('group', { name: /Google Chrome in zone 2 with Claude as tabs/ })).toBeVisible();
    const zone2 = page
      .getByRole('group', { name: 'Apps of “Backend”' })
      .getByRole('group', { name: 'Zone 2', exact: true });
    await expect(zone2.getByText(/^(MSYS2|Claude|Google Chrome)$/)).toHaveText(['MSYS2', 'Claude', 'Google Chrome']);
    await accessible(page);
  });

  test('renames it, and deletes it after asking', async ({ page }) => {
    await page.goto('/');
    await editWorkspace(page, 'Portfolio');
    await page.getByRole('button', { name: 'Rename “Portfolio”' }).click();
    const name = page.getByRole('textbox', { name: 'Name' });
    await expect(name).toBeFocused();
    await name.fill('Brand 2');
    await page.keyboard.press('Enter');
    await expect(page.getByRole('heading', { name: 'Brand 2', level: 1 })).toBeVisible();
    await page.getByRole('button', { name: 'Delete workspace' }).click();
    const dialog = page.getByRole('dialog', { name: 'Delete “Brand 2”?' });
    await expect(dialog).toBeVisible();
    await accessible(page);
    await dialog.getByRole('button', { name: 'Delete workspace' }).click();
    await expect(page.getByRole('heading', { name: 'Your screen' })).toBeVisible();
    await expect(page.getByRole('button', { name: /^Brand 2/ })).toHaveCount(0);
  });
});

test.describe('Settings', () => {
  test('shows what most people change, the rest under More settings, and About last', async ({ page }) => {
    await page.goto('/');
    await page.getByRole('button', { name: 'Settings' }).click();
    await expect(page.getByRole('heading', { name: 'Settings', level: 1 })).toBeFocused();
    for (const name of ['General', 'Side menu', 'Windows', 'About']) {
      await expect(page.getByRole('region', { name })).toBeVisible();
    }
    await expect(page.getByRole('switch', { name: 'Start a workspace’s closed apps' })).toBeHidden();
    await expect(page.getByRole('radiogroup', { name: 'Zone tags' })).toBeHidden();
    await page.getByRole('button', { name: 'More settings' }).click();
    await expect(page.getByRole('switch', { name: 'Start a workspace’s closed apps' })).toBeVisible();
    await expect(page.getByRole('radiogroup', { name: 'Zone tags' })).toBeVisible();
    await accessible(page);
    await windowDoesNotScroll(page);
  });

  test('a light theme can be chosen, and stays accessible', async ({ page }) => {
    await page.goto('/');
    await page.getByRole('button', { name: 'Settings' }).click();
    // As a person does: on the segment's label (the radio itself is drawn by it).
    await page.getByRole('radiogroup', { name: 'Theme' }).getByText('Light').click();
    await expect(page.getByRole('radio', { name: 'Light' })).toBeChecked();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
    await accessible(page);
    await page.getByRole('button', { name: 'Home' }).click();
    await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
    await accessible(page);
  });

  test('says why the side menu is on the other edge for now', async ({ page }) => {
    await page.goto('/?moved=1');
    await page.getByRole('button', { name: 'Settings' }).click();
    await expect(
      page.getByText(
        'On the left edge for now: the pointer doesn’t stop at the right edge of your main screen, because another screen is there.',
      ),
    ).toBeVisible();
  });

  test('settings saved by a newer Mosaïk are shown, and cannot be changed', async ({ page }) => {
    await page.goto('/?newer=1');
    await page.getByRole('button', { name: 'Settings' }).click();
    await expect(
      page.getByText('These settings were saved by a newer Mosaïk. Update Mosaïk to change them here.'),
    ).toBeVisible();
    await expect(page.getByRole('switch', { name: 'Start with Windows' })).toBeDisabled();
    await expect(page.getByRole('alert').filter({ hasText: /./ })).toHaveCount(0);
    await accessible(page);
  });
});

test.describe('the keyboard', () => {
  test('reaches both places, starting from the page title', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByRole('heading', { name: 'Home', level: 1 })).toBeFocused();
    await page.keyboard.press('Shift+Tab');
    await expect(page.getByRole('button', { name: /^Switch workspace/ })).toBeFocused();
    await page.keyboard.press('Shift+Tab');
    await expect(page.getByRole('button', { name: 'Settings' })).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(page.getByRole('heading', { name: 'Settings', level: 1 })).toBeFocused();
    await page.keyboard.press('Shift+Tab');
    await page.keyboard.press('Shift+Tab');
    await page.keyboard.press('Shift+Tab');
    await expect(page.getByRole('button', { name: 'Home' })).toBeFocused();
  });

  test('goes round the window with Tab, and never leaves it', async ({ page }) => {
    await page.goto('/');
    await page.getByRole('button', { name: 'Settings' }).click();
    const last = page.getByRole('button', { name: 'Logs' });
    await last.focus();
    await page.keyboard.press('Tab');
    await expect(page.getByRole('button', { name: 'Home' })).toBeFocused();
    await page.keyboard.press('Shift+Tab');
    await expect(last).toBeFocused();
  });

  test('stays inside a dialog', async ({ page }) => {
    await page.goto('/');
    await editZones(page);
    await page.getByRole('button', { name: 'Start from a layout' }).click();
    await page.getByRole('button', { name: 'Thirds' }).click();
    await page.getByRole('button', { name: 'Settings' }).click();
    const dialog = page.getByRole('dialog', { name: 'Leave without saving?' });
    await expect(dialog).toBeVisible();
    const discard = dialog.getByRole('button', { name: 'Discard changes' });
    await discard.focus();
    await page.keyboard.press('Tab');
    await expect(dialog.getByRole('button', { name: 'Cancel' })).toBeFocused();
    await page.keyboard.press('Shift+Tab');
    await expect(discard).toBeFocused();
  });
});

test('a page opens at its top, wherever the last one was scrolled to', async ({ page }) => {
  await page.goto('/');
  await page.locator('main').evaluate((main) => {
    main.scrollTop = main.scrollHeight;
  });
  await page.getByRole('button', { name: 'Settings' }).click();
  await expect(page.getByRole('heading', { name: 'Settings', level: 1 })).toBeInViewport();
  expect(await page.locator('main').evaluate((main) => main.scrollTop)).toBe(0);
});

test.describe('the zone editor', () => {
  test('resizes zones from the keyboard and saves them', async ({ page }) => {
    await page.goto('/');
    await editZones(page);
    const line = page.getByRole('separator', { name: 'Line right of zone 1, left of zone 2 and zone 3' });
    await line.focus();
    await page.keyboard.press('ArrowLeft');
    await expect(line).toHaveAttribute('aria-valuenow', '58');
    await page.getByRole('button', { name: 'Save zones' }).click();
    await expect(shown(page, 'Zones saved.')).toBeVisible();
    await accessible(page);
    await windowDoesNotScroll(page);
  });

  test('starts from the same layouts as Home', async ({ page }) => {
    await page.goto('/');
    await editZones(page);
    await page.getByRole('button', { name: 'Start from a layout' }).click();
    await page.getByRole('button', { name: 'Stack + half' }).click();
    await expect(page.getByRole('separator')).toHaveCount(2);
    await expect(page.getByRole('separator', { name: /^Line right of .+ and .+, left of / })).toHaveCount(1);
    await page.getByRole('button', { name: 'Start from a layout' }).click();
    await expect(page.getByRole('button', { name: 'Three rows' })).toHaveCount(0);
    await page.getByRole('button', { name: 'Thirds' }).click();
    await expect(page.getByRole('separator', { name: /^Line between / })).toHaveCount(2);
    await accessible(page);
  });

  test('picks a zone to edit, by its number', async ({ page }) => {
    await page.goto('/');
    await editZones(page);
    await page.getByRole('button', { name: /^Zone 1,/ }).click();
    await expect(page.getByRole('region', { name: 'Zone 1' })).toBeVisible();
    await accessible(page);
  });
});

test('the first run has no tour: Home shows what Arrange will do, and Mosaïk starts with Windows', async ({ page }) => {
  await page.goto('/?first=1');
  await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  // No word of welcome, nor of the workspaces made of the windows open (the owner: "this shit has
  // to go", then "lets not flood with useless infos"): they are simply there.
  await expect(page.getByRole('button', { name: 'Coding', exact: true })).toBeVisible();
  await expect(page.getByText(/Mosaïk tidies your windows/)).toHaveCount(0);
  await expect(page.getByText(/Made from the windows/)).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Got it' })).toHaveCount(0);
  await accessible(page);
  await page.getByRole('button', { name: 'Settings' }).click();
  await expect(page.getByRole('switch', { name: 'Start with Windows' })).toBeChecked();
  await page.getByRole('button', { name: 'Home' }).click();
  await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
});

test('Mosaïk’s notes wait behind a button, never over the page', async ({ page }) => {
  await page.goto('/?notes=1');
  const bell = page.getByRole('button', { name: 'Notes from Mosaïk: 2' });
  await expect(bell).toBeVisible();
  await expect(page.getByText('FancyZones is running')).toBeHidden();
  await bell.click();
  const notes = page.getByRole('region', { name: 'Notes' });
  await expect(notes.getByText('FancyZones is running')).toBeVisible();
  await accessible(page);
  await notes.getByRole('button', { name: 'Clear all' }).click();
  await expect(page.getByRole('button', { name: /^Notes from Mosaïk/ })).toHaveCount(0);
});

test('with nothing open and nothing saved, Home says what to do', async ({ page }) => {
  await page.goto('/?empty=1&nowindows=1');
  await expect(shown(page, 'Open some apps first.')).toBeVisible();
  await expect(arrange(page)).toBeDisabled();
  await expect(
    shown(page, 'Arrange your windows, then save them as a workspace to bring them back later.'),
  ).toBeVisible();
  await accessible(page);
});

test.describe('at the smallest window size', () => {
  test.use({ viewport: { width: 760, height: 560 } });

  test('every page is usable and accessible', async ({ page }) => {
    await page.goto('/?second=1');
    await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
    await expect(page.getByRole('radio', { name: 'Halves' })).toBeInViewport();
    // The workspaces beside the picture, not under it.
    const picture = await page.getByRole('group', { name: AUTO }).boundingBox();
    const column = await page.getByRole('region', { name: 'Workspaces' }).boundingBox();
    expect(picture && column && column.x >= picture.x + picture.width && column.y < picture.y + picture.height).toBe(
      true,
    );
    await accessible(page);
    await windowDoesNotScroll(page);
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await page.getByRole('button', { name: 'More settings' }).click();
    await accessible(page);
    await windowDoesNotScroll(page);
    await page.getByRole('button', { name: 'Home', exact: true }).click();
    await editWorkspace(page, 'Website');
    await accessible(page);
    await windowDoesNotScroll(page);
    await page.getByRole('button', { name: 'Back to Home' }).click();
    await page.locator('main').evaluate((main) => {
      main.scrollTop = main.scrollHeight;
    });
    await page.getByRole('button', { name: 'More for “Portfolio”' }).click();
    await expect(page.getByRole('button', { name: 'Edit zones' })).toBeInViewport({ ratio: 1 });
    await page.keyboard.press('Escape');
    await editZones(page);
    await page.getByRole('button', { name: /Portrait/ }).click();
    await expect(page.getByText('No zones on this monitor. Start from a layout.')).toBeVisible();
    await accessible(page);
  });
});

test.describe('in high contrast', () => {
  test.use({ forcedColors: 'active' });

  test('the pages stay accessible', async ({ page }) => {
    await page.goto('/');
    await accessible(page);
    await page.getByRole('radio', { name: 'Thirds' }).check();
    await accessible(page);
    await editZones(page);
    await page.getByRole('button', { name: /^Zone 1,/ }).click();
    await accessible(page);
  });
});

test('when Mosaïk does not answer, the window says so and offers to try again', async ({ page }) => {
  await page.goto('/?broken=1');
  await expect(page.getByRole('heading', { name: 'Mosaïk isn’t answering' })).toBeVisible();
  await expect(page.getByRole('alert')).toHaveText('Mosaïk did not answer in time. Try again in a moment.');
  await expect(page.getByRole('button', { name: 'Try again' })).toBeVisible();
  await accessible(page);
});
