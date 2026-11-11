// Builds the launcher icons from one square source icon (artwork on a flat background colour).
// Usage: node scripts/prepare-app-icon.mjs <icon-1024.png>
//  - apps/mobile/assets/icon.png: opaque full-bleed square (iOS masks it; legacy Android icon).
//  - apps/mobile/assets/safeeat-adaptive-foreground.png: artwork on transparency, sized for the
//    Android adaptive-icon safe zone so circle/squircle masks never clip the corner brackets.
// The background colour is read from the source and must match android.adaptiveIcon.backgroundColor.
import { resolve } from 'node:path';
import sharp from 'sharp';

const source = process.argv[2];
if (!source) throw new Error('Usage: node scripts/prepare-app-icon.mjs <icon-1024.png>');
const out = resolve('apps/mobile/assets');
const SIZE = 1024;
// Artwork side as a share of the 108dp adaptive canvas: its rounded corners stay inside the
// 72dp visible circle (radius 0.333) that the strictest launcher mask leaves.
const FOREGROUND_SHARE = 0.48;

const { data, info } = await sharp(source)
  .resize(SIZE, SIZE, { fit: 'fill' })
  .ensureAlpha()
  .raw()
  .toBuffer({ resolveWithObject: true });
const at = (x, y) => (y * info.width + x) * 4;
// Background = the opaque pixel a quarter of the way down the left edge band (outside the artwork).
const b = at(Math.round(SIZE * 0.1), Math.round(SIZE / 2));
const bg = [data[b], data[b + 1], data[b + 2]];
const hex = `#${bg.map((v) => v.toString(16).padStart(2, '0')).join('')}`.toUpperCase();

// 1. Opaque full-bleed icon: fill the source's transparent rounded corners with the background.
await sharp(data, { raw: info })
  .flatten({ background: { r: bg[0], g: bg[1], b: bg[2] } })
  .png()
  .toFile(`${out}/icon.png`);

// 2. Foreground: "colour to alpha" against the background keeps anti-aliased edges clean.
const fg = Buffer.alloc(data.length);
let left = SIZE, top = SIZE, right = 0, bottom = 0;
for (let y = 0; y < SIZE; y += 1) {
  for (let x = 0; x < SIZE; x += 1) {
    const i = at(x, y);
    if (data[i + 3] < 250) continue; // outside the source's rounded square
    let alpha = 0;
    for (let c = 0; c < 3; c += 1) {
      const v = data[i + c], base = bg[c];
      const a = v > base ? (v - base) / (255 - base) : v < base ? (base - v) / base : 0;
      alpha = Math.max(alpha, a);
    }
    if (alpha < 1 / 255) continue;
    for (let c = 0; c < 3; c += 1) {
      fg[i + c] = Math.max(0, Math.min(255, Math.round(bg[c] + (data[i + c] - bg[c]) / alpha)));
    }
    fg[i + 3] = Math.round(alpha * 255);
    if (alpha > 0.1) {
      left = Math.min(left, x); top = Math.min(top, y);
      right = Math.max(right, x); bottom = Math.max(bottom, y);
    }
  }
}
const side = Math.max(right - left, bottom - top) + 1;
const cx = Math.round((left + right) / 2), cy = Math.round((top + bottom) / 2);
const crop = { left: cx - Math.floor(side / 2), top: cy - Math.floor(side / 2), width: side, height: side };
const target = Math.round(SIZE * FOREGROUND_SHARE);
const pad = Math.floor((SIZE - target) / 2);
const artwork = await sharp(fg, { raw: info }).extract(crop).resize(target, target).png().toBuffer();
await sharp(artwork)
  .extend({ top: pad, left: pad, bottom: SIZE - target - pad, right: SIZE - target - pad, background: { r: 0, g: 0, b: 0, alpha: 0 } })
  .png()
  .toFile(`${out}/safeeat-adaptive-foreground.png`);

console.log(`icon.png + safeeat-adaptive-foreground.png written; adaptive background ${hex}`);
