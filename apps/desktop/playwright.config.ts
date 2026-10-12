import { defineConfig, devices } from '@playwright/test';

// Smoke tests of the interface in a browser, against the development server and its stand-in
// engine (src/mock.ts). `PLAYWRIGHT_CHROMIUM` points at a Chromium already installed, if any.
const chromium = process.env.PLAYWRIGHT_CHROMIUM;

export default defineConfig({
  testDir: 'e2e',
  timeout: 30_000,
  forbidOnly: Boolean(process.env.CI),
  reporter: process.env.CI ? 'github' : 'list',
  use: {
    ...devices['Desktop Chrome'],
    baseURL: 'http://127.0.0.1:1420',
    viewport: { width: 1000, height: 760 },
    ...(chromium ? { launchOptions: { executablePath: chromium } } : {}),
  },
  webServer: {
    command: 'npm run dev',
    url: 'http://127.0.0.1:1420',
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
