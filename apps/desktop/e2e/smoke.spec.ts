import AxeBuilder from '@axe-core/playwright';
import { type Page, expect, test } from '@playwright/test';

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
const arrange = (page: Page) => page.getByRole('button', { name: 'Arrange my screen', exact: true });
const HINT = 'Drag a window onto another to swap them, or a line between zones to resize them.';

/** Opens a workspace's page from its card's menu on Home. */
async function editWorkspace(page: Page, name: string) {
  await page.getByRole('button', { name: `More for “${name}”` }).click();
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  await expect(page.getByRole('heading', { name, level: 1 })).toBeVisible();
}

/** Opens the zone editor of the stand-in's first workspace, from its card's menu. */
async function editZones(page: Page) {
  await page.getByRole('button', { name: 'More for “Ship the release”' }).click();
  await page.getByRole('button', { name: 'Edit zones' }).click();
  await expect(page.getByRole('heading', { name: 'Ship the release', level: 1 })).toBeVisible();
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
    await expect(shown(page, 'Slack will be minimized.')).toBeVisible();
    await expect(
      page.getByRole('group', {
        name: 'Visual Studio Code in zone 2, Google Chrome in zone 1, Windows Terminal in zone 3, Slack minimized.',
      }),
    ).toBeVisible();
    await arrange(page).click();
    await expect(shown(page, 'Arranged 3 windows in Thirds. 1 window minimized.')).toBeVisible();
    await page.getByRole('button', { name: 'Undo' }).click();
    await expect(shown(page, 'Windows are back where they were.')).toBeVisible();
    await accessible(page);
  });

  test('Arrange lays the screen out for its task, and can be undone', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
    await arrange(page).click();
    await expect(shown(page, 'Arranged 4 windows.')).toBeVisible();
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
    await accessible(page);
    await arrange(page).click();
    await expect(shown(page, 'Arranged 4 windows.')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Undo changes' })).toBeHidden();
    // Arranged as they were put: the picture shows them there now.
    const [c, d] = [await code.boundingBox(), await browser.boundingBox()];
    expect(c && Math.abs(c.x - b.x) < 3).toBe(true);
    expect(d && Math.abs(d.x - a.x) < 3).toBe(true);
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
    await expect(page.getByRole('button', { name: 'Undo changes' })).toBeVisible();
    await arrange(page).click();
    await expect(shown(page, 'Arranged 4 windows.')).toBeVisible();
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
    await expect(shown(page, 'Windows Terminal and Slack will be minimized.')).toBeVisible();
  });

  test('a workspace picked shows how it opens, and opens with Open', async ({ page }) => {
    // The owner: "we shouldnt open the workspace when clicking on it it should show it in the
    // anim as preview and show an open button".
    await page.goto('/');
    const list = page.getByRole('list', { name: 'Workspaces' });
    await expect(list.getByRole('button', { name: 'Ship the release (on screen)', exact: true })).toBeVisible();
    await list.getByRole('button', { name: 'Brand refresh', exact: true }).click();
    await expect(
      page.getByRole('group', {
        name: 'Figma starts in zone 1, Notepad starts in zone 2, Visual Studio Code minimized, Google Chrome minimized, Windows Terminal minimized, Slack minimized.',
      }),
    ).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Preview of “Brand refresh”' })).toBeVisible();
    await expect(shown(page, 'Nothing moves until you press Open.')).toBeVisible();
    await accessible(page);
    // Clicked again, it stays; Esc, or Back, shows the screen again.
    await list.getByRole('button', { name: 'Brand refresh', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Open “Brand refresh”' })).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.getByRole('heading', { name: 'Your screen' })).toBeVisible();
    await expect(page.getByRole('group', { name: AUTO })).toBeVisible();
    await list.getByRole('button', { name: 'Brand refresh', exact: true }).click();
    await page.getByRole('button', { name: 'Back to my screen' }).click();
    await expect(arrange(page)).toBeVisible();
    await expect(list.getByRole('button', { name: 'Brand refresh', exact: true })).toBeFocused();
    await list.getByRole('button', { name: 'Brand refresh', exact: true }).click();
    await page.getByRole('button', { name: 'Open “Brand refresh”' }).click();
    await expect(shown(page, 'Opened “Brand refresh”.')).toBeVisible();
    await expect(list.getByRole('button', { name: 'Brand refresh (on screen)', exact: true })).toBeVisible();
    await expect(arrange(page)).toBeVisible();
  });

  test('a double-click on a workspace opens it at once', async ({ page }) => {
    await page.goto('/');
    const list = page.getByRole('list', { name: 'Workspaces' });
    await list.getByRole('button', { name: 'Brand refresh', exact: true }).dblclick();
    await expect(shown(page, 'Opened “Brand refresh”.')).toBeVisible();
    await expect(list.getByRole('button', { name: 'Brand refresh (on screen)', exact: true })).toBeVisible();
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
    await name.fill('Ship the release');
    await page.keyboard.press('Enter');
    await expect(
      shown(
        page,
        '“Ship the release” already exists. Pick another name, or open its ⋯ menu and choose Replace with what’s on screen.',
      ),
    ).toBeVisible();
    await name.fill('Brand refresh');
    await page.keyboard.press('Enter');
    await expect(shown(page, '“Brand refresh” already exists. Pick another name.')).toBeVisible();
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
    await page.getByRole('button', { name: 'More for “Brand refresh”' }).click();
    for (const name of ['Edit', 'Edit zones']) {
      await expect(page.getByRole('button', { name, exact: true })).toBeInViewport({ ratio: 1 });
    }
    await accessible(page);
  });
});

test.describe('a workspace’s page', () => {
  test('shows how it opens, lists its apps by zone, and moves, removes and adds them', async ({ page }) => {
    await page.goto('/');
    await editWorkspace(page, 'Ship the release');
    await expect(
      page.getByRole('group', {
        name: 'Visual Studio Code in zone 1, Google Chrome in zone 2, Windows Terminal in zone 3 with Slack as tabs.',
      }),
    ).toBeVisible();
    const apps = page.getByRole('group', { name: 'Apps of “Ship the release”' });
    await expect(apps.getByRole('listitem')).toHaveCount(4);
    await expect(apps.getByRole('group', { name: 'Zone 3', exact: true }).getByRole('listitem')).toHaveCount(2);
    await page.getByRole('button', { name: 'Move Slack to another zone' }).click();
    await page.getByRole('button', { name: 'Zone 2', exact: true }).click();
    await expect(shown(page, 'Slack goes in zone 2.')).toBeVisible();
    await expect(apps.getByRole('group', { name: 'Zone 2', exact: true }).getByText('Slack')).toBeVisible();
    await page.getByRole('button', { name: 'Remove Slack' }).click();
    await expect(apps.getByRole('listitem')).toHaveCount(3);
    await page.getByRole('button', { name: 'Add a window to “Ship the release”' }).click();
    const slack = page.getByRole('button', { name: 'Slack Slack window' });
    await expect(slack).toBeInViewport({ ratio: 1 });
    await slack.click();
    await expect(shown(page, 'Slack added.')).toBeVisible();
    await expect(apps.getByRole('listitem')).toHaveCount(4);
    await accessible(page);
    await windowDoesNotScroll(page);
    await page.getByRole('button', { name: 'Back to Home' }).click();
    await expect(page.getByRole('heading', { name: 'Your screen' })).toBeVisible();
  });

  test('its picture shows each zone once, empty ones too, and an app dragged there goes in it', async ({ page }) => {
    // The owner, of a workspace with two windows in one zone drawn over each other: "look at the
    // anim wtf is happening"; and "when a zone is empty we dont have either color or number".
    await page.goto('/');
    await editWorkspace(page, 'Release checks');
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
    const apps = page.getByRole('group', { name: 'Apps of “Release checks”' });
    await expect(apps.getByRole('group', { name: 'Zone 3', exact: true }).getByText('Claude')).toBeVisible();
    await accessible(page);
  });

  test('renames it, and deletes it after asking', async ({ page }) => {
    await page.goto('/');
    await editWorkspace(page, 'Brand refresh');
    await page.getByRole('button', { name: 'Rename “Brand refresh”' }).click();
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
  // No word of welcome to put away (the owner: "this shit has to go"); the workspaces made of the
  // windows open are said, until put away.
  await expect(page.getByText(/Mosaïk tidies your windows/)).toHaveCount(0);
  await expect(shown(page, 'Made from the windows you had open. Delete any you don’t need.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Coding', exact: true })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Your screen' }).getByRole('button', { name: 'Got it' })).toHaveCount(
    0,
  );
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
    await editWorkspace(page, 'Ship the release');
    await accessible(page);
    await windowDoesNotScroll(page);
    await page.getByRole('button', { name: 'Back to Home' }).click();
    await page.locator('main').evaluate((main) => {
      main.scrollTop = main.scrollHeight;
    });
    await page.getByRole('button', { name: 'More for “Brand refresh”' }).click();
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
