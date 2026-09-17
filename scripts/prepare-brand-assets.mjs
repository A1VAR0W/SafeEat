import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import sharp from 'sharp';

const [markSource, wordmarkSource] = process.argv.slice(2);
if (!markSource || !wordmarkSource) {
  throw new Error('Usage: node scripts/prepare-brand-assets.mjs <mark.png> <wordmark.png>');
}

const mobileAssets = resolve('apps/mobile/assets');
const webAssets = resolve('apps/web/public/brand');
await Promise.all([
  mkdir(mobileAssets, { recursive: true }),
  mkdir(webAssets, { recursive: true }),
]);

async function trimToAlpha(source) {
  const { data, info } = await sharp(source)
    .ensureAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true });
  for (let i = 0; i < data.length; i += info.channels) {
    if (data[i] + data[i + 1] + data[i + 2] < 24) data[i + 3] = 0;
  }
  let left = info.width;
  let top = info.height;
  let right = 0;
  let bottom = 0;
  for (let y = 0; y < info.height; y += 1) {
    for (let x = 0; x < info.width; x += 1) {
      if (data[(y * info.width + x) * info.channels + 3] > 8) {
        left = Math.min(left, x);
        top = Math.min(top, y);
        right = Math.max(right, x);
        bottom = Math.max(bottom, y);
      }
    }
  }
  return sharp(data, { raw: info })
    .extract({ left, top, width: right - left + 1, height: bottom - top + 1 })
    .png()
    .toBuffer();
}

const mark = await trimToAlpha(markSource);
const wordmark = await trimToAlpha(wordmarkSource);

await Promise.all([
  sharp(mark).resize({ width: 768 }).png().toFile(`${mobileAssets}/safeeat-mark.png`),
  sharp(wordmark).resize({ width: 1200 }).png().toFile(`${mobileAssets}/safeeat-wordmark.png`),
  sharp(mark).resize({ width: 768 }).png().toFile(`${webAssets}/safeeat-mark.png`),
  sharp(wordmark).resize({ width: 1200 }).png().toFile(`${webAssets}/safeeat-wordmark.png`),
  sharp(mark)
    .resize({
      width: 690,
      height: 690,
      fit: 'contain',
      background: { r: 0, g: 0, b: 0, alpha: 0 },
    })
    .extend({
      top: 167,
      bottom: 167,
      left: 167,
      right: 167,
      background: { r: 0, g: 0, b: 0, alpha: 0 },
    })
    .png()
    .toFile(`${mobileAssets}/safeeat-adaptive-foreground.png`),
  sharp(mark).resize({ width: 640 }).png().toFile(`${mobileAssets}/safeeat-splash.png`),
]);

const iconBackground = Buffer.from(
  `
    <svg width="1024" height="1024" xmlns="http://www.w3.org/2000/svg">
      <defs><linearGradient id="g" x1="0" y1="1" x2="1" y2="0"><stop stop-color="#005C43"/><stop offset="1" stop-color="#79C941"/></linearGradient></defs>
      <rect width="1024" height="1024" fill="url(#g)"/>
    </svg>`,
);
const iconMark = await sharp(mark).resize({ width: 720 }).png().toBuffer();
await sharp(iconBackground)
  .composite([{ input: iconMark, gravity: 'center' }])
  .png()
  .toFile(`${mobileAssets}/icon.png`);
