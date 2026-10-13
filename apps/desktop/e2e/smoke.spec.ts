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

/** On the home page, the picture stays at the top while the page scrolls, and every control the
 * keyboard reaches below it, going down and back up, is in view below it, never under it. */
async function pictureStaysClear(page: Page, height: number) {
  await page.goto('/');
  await expect(page.getByText('Your main screen now: 4 windows')).toBeVisible();
  const picture = page.getByRole('img');
  const clear = async () => {
    const p = await picture.boundingBox();
    const f = await page.evaluate(() => {
      const r = document.activeElement?.getBoundingClientRect();
      return r ? { y: r.y, bottom: r.bottom, name: document.activeElement?.textContent ?? '' } : null;
    });
    if (!p || !f) throw new Error('not on screen');
    expect(p.y, 'the picture stays at the top').toBeGreaterThanOrEqual(0);
    expect(f.y, `${f.name} below the picture`).toBeGreaterThanOrEqual(p.y + p.height);
    // (A fraction of a pixel over is the browser's rounding.)
    expect(f.bottom, `${f.name} in the window`).toBeLessThanOrEqual(height + 1);
  };
  // Through the layouts and the workspaces, then back up.
  await page.getByRole('button', { name: 'Arrange windows' }).focus();
  for (let i = 0; i < 13; i++) {
    await page.keyboard.press('Tab');
    await clear();
  }
  for (let i = 0; i < 13; i++) {
    await page.keyboard.press('Shift+Tab');
    if (i < 12) await clear();
  }
  await expect(page.getByRole('button', { name: 'Arrange windows' })).toBeFocused();
  // The last layout, reached by the keyboard, shows in the picture.
  await page.getByRole('button', { name: 'Whole screen' }).focus();
  await expect(page.getByText('With Whole screen: 3 windows minimized')).toBeVisible();
}

test('the home page shows the screen and the layouts, and is accessible', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Tidy your screen', level: 1 })).toBeVisible();
  await expect(page.getByText('Your main screen now: 4 windows')).toBeVisible();
  await expect(page.getByRole('region', { name: 'Layouts' }).getByRole('button')).toHaveCount(10);
  await accessible(page);
  await windowDoesNotScroll(page);
});

test('the home page shows each workspace’s apps, and opens them to change them', async ({ page }) => {
  await page.goto('/');
  const row = page.getByRole('listitem').filter({ hasText: 'Ship the release' });
  await expect(row.getByRole('list', { name: 'Its apps' }).getByRole('listitem')).toHaveCount(4);
  await row.getByRole('button', { name: 'Apps of “Ship the release”' }).click();
  await expect(page.getByRole('list', { name: 'Apps of “Ship the release”' }).getByRole('listitem')).toHaveCount(4);
  await accessible(page);
});

test('the picture stays in view while the layouts scroll beneath it', async ({ page }) => {
  await pictureStaysClear(page, 760);
});

test('workspaces are listed and the page is accessible', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Workspaces', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Workspaces', level: 1 })).toBeVisible();
  await expect(page.getByRole('listitem')).toHaveCount(2);
  await accessible(page);
  await windowDoesNotScroll(page);
});

test('a workspace’s apps are listed, and can be moved, removed and added', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Workspaces', exact: true }).click();
  await page.getByRole('button', { name: 'Apps of “Ship the release”' }).click();
  const apps = page.getByRole('list', { name: 'Apps of “Ship the release”' });
  await expect(apps.getByRole('listitem')).toHaveCount(4);
  await page.getByRole('combobox', { name: 'Zone of Slack' }).selectOption({ label: 'Browser' });
  await expect(page.getByText('Slack goes in Browser.')).toBeVisible();
  await page.getByRole('button', { name: 'Remove Slack' }).click();
  await expect(apps.getByRole('listitem')).toHaveCount(3);
  await page.getByRole('button', { name: 'Add a window to “Ship the release”' }).click();
  await page.getByRole('button', { name: /^Slack/ }).click();
  await expect(page.getByText('Slack added.')).toBeVisible();
  await expect(apps.getByRole('listitem')).toHaveCount(4);
  await accessible(page);
  await windowDoesNotScroll(page);
});

test('a layout shows where the windows would go, then puts them there, and Undo puts them back', async ({ page }) => {
  await page.goto('/');
  const picture = page.getByRole('img', { name: /^Your main screen now/ });
  await expect(picture).toBeVisible();
  await page.getByRole('button', { name: 'Thirds' }).hover();
  await expect(page.getByText('With Thirds: 1 window minimized')).toBeVisible();
  await expect(page.getByRole('img', { name: /Visual Studio Code in Code/ })).toBeVisible();
  await page.getByRole('button', { name: 'Thirds' }).click();
  await expect(page.getByText('Arranged 3 windows in the “Thirds” layout; 1 window minimized.')).toBeVisible();
  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(page.getByText('Windows are back where they were.')).toBeVisible();
  await accessible(page);
});

test('the keyboard reaches every page, starting from the page title', async ({ page }) => {
  await page.goto('/');
  const title = page.getByRole('heading', { name: 'Tidy your screen', level: 1 });
  await expect(title).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(page.getByRole('button', { name: 'About' })).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(page.getByRole('button', { name: 'Settings' })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('heading', { name: 'Settings', level: 1 })).toBeFocused();
  await accessible(page);
});

test('the Tab key goes round the window and never leaves it', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'About' }).click();
  const last = page.getByRole('button', { name: 'Show the welcome again' });
  await last.focus();
  await page.keyboard.press('Tab');
  await expect(page.getByRole('button', { name: 'Home' })).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(last).toBeFocused();
});

test('a dialog keeps the Tab key inside it', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Workspaces', exact: true }).click();
  await page.getByRole('button', { name: 'Edit zones of “Ship the release”' }).click();
  await page.getByRole('button', { name: 'Start from a preset' }).click();
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

test('a dark theme can be chosen, and stays accessible', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Settings' }).click();
  await page.getByRole('radio', { name: 'Dark' }).check();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await accessible(page);
});

test('zones can be resized from the keyboard and saved', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Workspaces', exact: true }).click();
  await page.getByRole('button', { name: 'Edit zones of “Ship the release”' }).click();
  const line = page.getByRole('separator', { name: 'Line right of Code, left of Browser and Terminal' });
  await line.focus();
  await page.keyboard.press('ArrowLeft');
  await expect(line).toHaveAttribute('aria-valuenow', '58');
  await page.getByRole('button', { name: 'Save zones' }).click();
  await expect(page.getByText('Zones saved.')).toBeVisible();
  await accessible(page);
  await windowDoesNotScroll(page);
});

test('Arrange lays the screen out for its task and can be undone', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Arrange windows' }).hover();
  await expect(page.getByText('With Arrange, for coding: every window in its own zone')).toBeVisible();
  await page.getByRole('button', { name: 'Arrange windows' }).click();
  await expect(page.getByText('Arranged 4 windows for coding.')).toBeVisible();
  await expect(page.getByRole('img', { name: /^Your main screen now/ })).toBeVisible();
  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(page.getByText('Windows are back where they were.')).toBeVisible();
  await accessible(page);
});

test('the vertical presets: two stacked zones beside a half, and rows', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Workspaces', exact: true }).click();
  await page.getByRole('button', { name: 'Edit zones of “Ship the release”' }).click();
  await page.getByRole('button', { name: 'Start from a preset' }).click();
  await page.getByRole('button', { name: 'Two stacked beside half' }).click();
  await expect(page.getByRole('separator')).toHaveCount(2);
  await expect(page.getByRole('separator', { name: /^Line right of .+ and .+, left of / })).toHaveCount(1);
  await page.getByRole('button', { name: 'Start from a preset' }).click();
  await page.getByRole('button', { name: 'Three rows' }).click();
  await expect(page.getByRole('separator')).toHaveCount(2);
  await expect(page.getByRole('separator', { name: /^Line between / })).toHaveCount(2);
  await accessible(page);
});

test('a zone can be picked and edited, and the page stays accessible', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Workspaces', exact: true }).click();
  await page.getByRole('button', { name: 'Edit zones of “Ship the release”' }).click();
  await page.getByRole('button', { name: /^Code,/ }).click();
  await expect(page.getByRole('textbox', { name: 'Name' })).toHaveValue('Code');
  await accessible(page);
});

test('the welcome shows on a first run, and fits the window', async ({ page }) => {
  await page.goto('/?welcome=1');
  await expect(page.getByRole('heading', { name: 'Welcome to Mosaïk' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Get started' })).toBeInViewport();
  await accessible(page);
  await page.getByRole('button', { name: 'Get started' }).click();
  await expect(page.getByRole('heading', { name: 'Tidy your screen', level: 1 })).toBeVisible();
});

test.describe('at the smallest window size', () => {
  test.use({ viewport: { width: 760, height: 560 } });

  test('every page is usable and accessible', async ({ page }) => {
    await page.goto('/?second=1');
    await expect(page.getByRole('heading', { name: 'Tidy your screen', level: 1 })).toBeVisible();
    await accessible(page);
    await windowDoesNotScroll(page);
    for (const name of ['Settings', 'About', 'Workspaces']) {
      await page.getByRole('button', { name, exact: true }).click();
      await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
      await accessible(page);
      await windowDoesNotScroll(page);
    }
    await page.getByRole('button', { name: 'Edit zones of “Ship the release”' }).click();
    await page.getByRole('button', { name: /Portrait/ }).click();
    await expect(page.getByText('No zones on this monitor. Start from a preset.')).toBeVisible();
    await accessible(page);
  });

  test('the picture stays in view, and what the keyboard reaches below it is not hidden under it', async ({ page }) => {
    await pictureStaysClear(page, 560);
  });
});

test.describe('in high contrast', () => {
  test.use({ forcedColors: 'active' });

  test('the pages stay accessible', async ({ page }) => {
    await page.goto('/');
    await accessible(page);
    await page.getByRole('button', { name: 'Thirds' }).hover();
    await accessible(page);
    await page.getByRole('button', { name: 'Workspaces', exact: true }).click();
    await page.getByRole('button', { name: 'Edit zones of “Ship the release”' }).click();
    await page.getByRole('button', { name: /^Code,/ }).click();
    await accessible(page);
  });
});

test('when Mosaïk does not answer, the window says so and offers to try again', async ({ page }) => {
  await page.goto('/?broken=1');
  await expect(page.getByRole('alert')).toHaveText('Mosaïk did not answer in time. Try again in a moment.');
  await expect(page.getByRole('button', { name: 'Try again' })).toBeVisible();
  await accessible(page);
});
