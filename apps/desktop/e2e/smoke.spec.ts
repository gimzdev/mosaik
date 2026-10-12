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

test('workspaces are listed and the page is accessible', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Workspaces', level: 1 })).toBeVisible();
  await expect(page.getByRole('listitem')).toHaveCount(2);
  await accessible(page);
  await windowDoesNotScroll(page);
});

test('the keyboard reaches every page, starting from the page title', async ({ page }) => {
  await page.goto('/');
  const title = page.getByRole('heading', { name: 'Workspaces', level: 1 });
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
  await expect(page.getByRole('button', { name: 'Workspaces' })).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(last).toBeFocused();
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

test('a zone can be picked and edited, and the page stays accessible', async ({ page }) => {
  await page.goto('/');
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
  await expect(page.getByRole('heading', { name: 'Workspaces', level: 1 })).toBeVisible();
});

test.describe('at the smallest window size', () => {
  test.use({ viewport: { width: 760, height: 560 } });

  test('every page is usable and accessible', async ({ page }) => {
    await page.goto('/?second=1');
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
});

test.describe('in high contrast', () => {
  test.use({ forcedColors: 'active' });

  test('the pages stay accessible', async ({ page }) => {
    await page.goto('/');
    await accessible(page);
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
