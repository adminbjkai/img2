import { test, expect } from '@playwright/test';

test('GET /health returns 200', async ({ request, baseURL }) => {
  const res = await request.get(`${baseURL}/health`);
  expect(res.status(), 'expected /health to return 200').toBe(200);
  const json = await res.json();
  expect(json).toMatchObject({ status: 'ok', service: 'img2' });
});
