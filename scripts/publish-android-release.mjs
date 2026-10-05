import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createReadStream, readFileSync, statSync } from 'node:fs';
import { basename } from 'node:path';

const tag = process.argv[2];
const apk = process.argv[3];
if (!/^v\d+\.\d+\.\d+$/.test(tag ?? '') || !apk)
  throw new Error(
    'Usage: node scripts/publish-android-release.mjs v1.0.0 artifacts/SafeEat-1.0.0.apk',
  );
const repository = 'A1VAR0W/SafeEat';
const version = tag.slice(1);
const release = JSON.parse(
  execFileSync('gh', ['api', `repos/${repository}/releases/tags/${tag}`], { encoding: 'utf8' }),
);
if (release.draft || release.prerelease || release.tag_name !== tag)
  throw new Error('Only published stable releases can be mirrored');
const expectedName = `SafeEat-${version}.apk`;
const asset = release.assets.find((a) => a.name === expectedName && a.state === 'uploaded');
if (!asset || basename(apk) !== expectedName || statSync(apk).size !== asset.size)
  throw new Error('Release APK is missing or has a different size');
const hash = createHash('sha256');
for await (const chunk of createReadStream(apk)) hash.update(chunk);
const sha256 = hash.digest('hex');
if (!asset.digest || asset.digest !== `sha256:${sha256}`)
  throw new Error('Local APK does not match GitHub asset digest');
const { androidVersionCode } = JSON.parse(readFileSync('release.json', 'utf8'));
if (JSON.parse(readFileSync('package.json', 'utf8')).version !== version)
  throw new Error('Tag and project version differ');
const minimumVersion = release.body?.match(
  /<!--\s*safeeat-min-version:\s*(\d+\.\d+\.\d+)\s*-->/,
)?.[1];
const origin = 'https://chat-ink.tail552c89.ts.net:10000';
const manifest = {
  schemaVersion: 1,
  repository,
  release: {
    id: release.id,
    tag_name: release.tag_name,
    draft: false,
    prerelease: false,
    body: release.body,
    html_url: release.html_url,
  },
  ...(minimumVersion ? { minimumVersion } : {}),
  android: {
    name: expectedName,
    url: `${origin}/safeeat-downloads/releases/${expectedName}`,
    size: asset.size,
    sha256,
    versionCode: androidVersionCode,
  },
};
const token = process.env.SAFEEAT_RELEASE_PUBLISH_TOKEN;
if (!token) throw new Error('SAFEEAT_RELEASE_PUBLISH_TOKEN is required');
const headers = { Authorization: `Bearer ${token}` };
const upload = await fetch(`${origin}/safeeat-release-publish/${expectedName}`, {
  method: 'PUT',
  headers: {
    ...headers,
    'Content-Type': 'application/vnd.android.package-archive',
    'Content-Length': String(asset.size),
  },
  body: createReadStream(apk),
  duplex: 'half',
  signal: AbortSignal.timeout(300000),
});
if (!upload.ok) throw new Error(`APK mirror failed: HTTP ${upload.status}`);
const publish = await fetch(`${origin}/safeeat-release-publish/latest.json`, {
  method: 'POST',
  headers: { ...headers, 'Content-Type': 'application/json' },
  body: JSON.stringify(manifest),
  signal: AbortSignal.timeout(30000),
});
if (!publish.ok) throw new Error(`Manifest publication failed: HTTP ${publish.status}`);
console.log(`Published ${version} (${androidVersionCode}) from GitHub Release ${release.id}`);
