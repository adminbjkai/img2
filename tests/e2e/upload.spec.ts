import { test, expect } from '@playwright/test';
import { makeSolidPng } from './_helpers';

test('upload via UI returns a link that loads', async ({ page, request, baseURL }) => {
  await page.goto('/', { waitUntil: 'networkidle' });
  await expect(page.locator('#uploadBox')).toBeVisible();

  const png = makeSolidPng({ width: 48, height: 48, rgba: [0, 128, 255, 255] });
  await page.setInputFiles('#fileInput', {
    name: 'smoke.png',
    mimeType: 'image/png',
    buffer: png
  });

  // Preview becomes visible when a file is selected.
  await expect(page.locator('#preview')).toBeVisible();

  await page.click('#uploadBtn');

  // Result area is shown on success.
  await expect(page.locator('#result')).toBeVisible();
  const url = await page.locator('#imageUrl').inputValue();
  expect(url, 'expected UI to populate imageUrl').toContain('/i/');

  // Verify the image and thumb endpoints are reachable.
  const u = new URL(url);
  const id = u.pathname.split('/').pop();
  expect(id, 'expected an id in returned /i/:id URL').toBeTruthy();

  const imgRes = await request.get(`${baseURL}/i/${id}`);
  expect(imgRes.status(), 'expected /i/:id to return 200').toBe(200);
  expect(imgRes.headers()['content-type'] || '').toMatch(/^image\//);

  const thumbRes = await request.get(`${baseURL}/thumb/${id}`);
  expect(thumbRes.status(), 'expected /thumb/:id to return 200').toBe(200);
  expect(thumbRes.headers()['content-type'] || '').toMatch(/^image\/png/);
});
