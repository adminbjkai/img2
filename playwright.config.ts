import { defineConfig } from '@playwright/test';

const baseURL = process.env.BASE_URL || 'http://127.0.0.1:8097';

export default defineConfig({
  testDir: 'tests/e2e',
  fullyParallel: true,
  timeout: 30_000,
  expect: { timeout: 10_000 },
  reporter: [['list']],

  // Keep CI/dev artifacts out of the repo.
  outputDir: 'docs/assets/test-artifacts',

  use: {
    baseURL,
    headless: true,
    viewport: { width: 1280, height: 720 },
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure'
  }
});
