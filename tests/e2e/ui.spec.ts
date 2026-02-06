import { test, expect } from '@playwright/test';

test('home loads and shows upload UI', async ({ page }) => {
  await page.goto('/', { waitUntil: 'networkidle' });
  await expect(page.locator('h1')).toHaveText('img2');
  await expect(page.locator('#uploadBox')).toBeVisible();
  await expect(page.locator('#fileInput')).toHaveCount(1);
  await expect(page.locator('#uploadBtn')).toHaveCount(1);
});
