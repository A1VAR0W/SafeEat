import { readFileSync, writeFileSync } from 'node:fs';
const next = process.argv[2];
if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(next ?? ''))
  throw new Error('Usage: node scripts/set-version.mjs 1.0.1');
const pkg = JSON.parse(readFileSync('package.json', 'utf8'));
const old = pkg.version.split('.').map(Number),
  parts = next.split('.').map(Number);
const first = parts.findIndex((part, i) => part !== old[i]);
if (first < 0 || parts[first] < old[first]) throw new Error('Version must increase');
const release = JSON.parse(readFileSync('release.json', 'utf8'));
release.androidVersionCode++;
for (const file of ['package.json', 'package-lock.json']) {
  const value = JSON.parse(readFileSync(file, 'utf8'));
  value.version = next;
  if (value.packages?.['']) value.packages[''].version = next;
  writeFileSync(file, JSON.stringify(value, null, 2) + '\n');
}
writeFileSync('release.json', JSON.stringify(release, null, 2) + '\n');
console.log(`SafeEat ${next}; Android versionCode ${release.androidVersionCode}`);
