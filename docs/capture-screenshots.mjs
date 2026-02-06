import { chromium } from '@playwright/test';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { deflateSync } from 'node:zlib';

function pad2(n) {
  return String(n).padStart(2, '0');
}

function stampUTC(d = new Date()) {
  return `${d.getUTCFullYear()}${pad2(d.getUTCMonth() + 1)}${pad2(d.getUTCDate())}`;
}

function crc32(buf) {
  let crc = 0xffffffff;
  for (let i = 0; i < buf.length; i++) {
    crc ^= buf[i];
    for (let j = 0; j < 8; j++) {
      const mask = -(crc & 1);
      crc = (crc >>> 1) ^ (0xedb88320 & mask);
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const t = Buffer.from(type, 'ascii');
  const crc = Buffer.alloc(4);
  const crcVal = crc32(Buffer.concat([t, data]));
  crc.writeUInt32BE(crcVal, 0);
  return Buffer.concat([len, t, data, crc]);
}

function makeSolidPng({ width = 48, height = 48, rgba = [255, 0, 0, 255] } = {}) {
  const [r, g, b, a] = rgba;
  const signature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;

  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y++) {
    const rowStart = y * (stride + 1);
    raw[rowStart] = 0;
    for (let x = 0; x < width; x++) {
      const px = rowStart + 1 + x * 4;
      raw[px + 0] = r;
      raw[px + 1] = g;
      raw[px + 2] = b;
      raw[px + 3] = a;
    }
  }

  const idat = deflateSync(raw);
  const iend = Buffer.alloc(0);
  return Buffer.concat([signature, chunk('IHDR', ihdr), chunk('IDAT', idat), chunk('IEND', iend)]);
}

function mustGetEnv(name, fallback) {
  const v = process.env[name] || fallback;
  if (!v) throw new Error(`Missing required env var ${name}`);
  return v;
}

async function main() {
  const port = process.env.PORT || '8097';
  const baseURL = process.env.BASE_URL || `http://127.0.0.1:${port}`;

  const outDir = path.resolve('docs/assets/images');
  const stamp = stampUTC();

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 720 } });
  const page = await context.newPage();

  const die = async (msg, err) => {
    const suffix = err ? `\n${err.stack || String(err)}` : '';
    console.error(`capture-screenshots: ${msg}${suffix}`);
    try {
      await browser.close();
    } catch {
      // ignore
    }
    process.exit(1);
  };

  try {
    // Home
    await page.goto(`${baseURL}/`, { waitUntil: 'networkidle', timeout: 20_000 });
    await page.waitForSelector('#uploadBox', { timeout: 10_000 });
    await page.waitForTimeout(250);
    await page.screenshot({ path: path.join(outDir, `${stamp}-home.png`), fullPage: true });

    // Upload
    const tmp = await mkdtemp(path.join(tmpdir(), 'img2-')); 
    const pngPath = path.join(tmp, 'upload.png');
    await writeFile(pngPath, makeSolidPng({ width: 64, height: 64, rgba: [0, 128, 255, 255] }));

    await page.setInputFiles('#fileInput', pngPath);
    await page.waitForSelector('#preview', { state: 'visible', timeout: 10_000 });
    await page.click('#uploadBtn');

    await page.waitForSelector('#result', { state: 'visible', timeout: 20_000 });
    await page.waitForFunction(() => {
      const el = document.getElementById('imageUrl');
      return el && el.value && el.value.includes('/i/');
    }, { timeout: 10_000 });

    await page.waitForTimeout(250);
    await page.screenshot({ path: path.join(outDir, `${stamp}-after-upload.png`), fullPage: true });

    const imageUrl = await page.locator('#imageUrl').inputValue();
    const u = new URL(imageUrl);
    const id = u.pathname.split('/').pop();
    if (!id) throw new Error('could not extract id from image URL');

    // Image detail (/i/:id)
    await page.goto(`${baseURL}/i/${id}`, { waitUntil: 'networkidle', timeout: 20_000 });
    await page.waitForTimeout(200);
    await page.screenshot({ path: path.join(outDir, `${stamp}-image-${id}.png`), fullPage: true });

    // Thumbnail (/thumb/:id)
    await page.goto(`${baseURL}/thumb/${id}`, { waitUntil: 'networkidle', timeout: 20_000 });
    await page.waitForTimeout(200);
    await page.screenshot({ path: path.join(outDir, `${stamp}-thumb-${id}.png`), fullPage: true });

    await browser.close();
  } catch (err) {
    await die('failed', err);
  }
}

main().catch((err) => {
  console.error(err.stack || String(err));
  process.exit(1);
});
