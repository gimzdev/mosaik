import AxeBuilder from '@axe-core/playwright';
import { type Page, expect, test } from '@playwright/test';

/** No accessibility problems that axe rates serious or critical (WCAG 2.1 A and AA). */
async function accessible(page: Page) {
  const results = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze();
  const bad = results.violations.filter((v) => v.impact === 'serious' || v.impact === 'critical');
  expect(bad.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(' ')).join(', ')}`)).toEqual([]);
}

test('workspaces are listed and the page is accessible', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Workspaces', level: 1 })).toBeVisible();
  await expect(page.getByRole('listitem')).toHaveCount(2);
  await accessible(page);
});

test('the keyboard reaches every page', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Workspaces', level: 1 })).toBeVisible();
  await page.keyboard.press('Tab');
  await page.keyboard.press('Tab');
  await expect(page.getByRole('button', { name: 'Settings' })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('heading', { name: 'Settings', level: 1 })).toBeVisible();
  await accessible(page);
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
  await page.getByRole('button', { name: 'Edit the zones of “Ship the release”' }).click();
  const line = page.getByRole('separator', { name: 'Line between Code and Browser' });
  await line.focus();
  await page.keyboard.press('ArrowLeft');
  await expect(line).toHaveAttribute('aria-valuenow', '58');
  await page.getByRole('button', { name: 'Save zones' }).click();
  await expect(page.getByText('Zones of “Ship the release” saved.')).toBeVisible();
  await accessible(page);
});

test('the welcome shows on a first run', async ({ page }) => {
  await page.goto('/?welcome=1');
  await expect(page.getByRole('heading', { name: 'Welcome to Mosaïk' })).toBeVisible();
  await accessible(page);
  await page.getByRole('button', { name: 'Get started' }).click();
  await expect(page.getByRole('heading', { name: 'Workspaces', level: 1 })).toBeVisible();
});
