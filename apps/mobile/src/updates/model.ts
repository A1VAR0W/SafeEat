import { z } from 'zod';

export const updateOrigin = 'https://chat-ink.tail552c89.ts.net:10000';
export const updateManifestUrl = `${updateOrigin}/safeeat-downloads/releases/latest.json`;
const versionPattern = /^(0|[1-9]\d{0,5})\.(0|[1-9]\d{0,5})\.(0|[1-9]\d{0,5})$/;
export function normalizeVersion(value: string) {
  const version = value.replace(/^v/, '');
  if (!versionPattern.test(version)) throw new Error('Invalid stable version');
  return version;
}
export function compareVersions(a: string, b: string) {
  const aa = normalizeVersion(a).split('.').map(Number);
  const bb = normalizeVersion(b).split('.').map(Number);
  for (let i = 0; i < 3; i++) if (aa[i] !== bb[i]) return aa[i]! > bb[i]! ? 1 : -1;
  return 0;
}
const manifestSchema = z.object({
  schemaVersion: z.literal(1),
  repository: z.literal('A1VAR0W/SafeEat'),
  release: z.object({
    id: z.number().int().positive(),
    tag_name: z.string(),
    draft: z.literal(false),
    prerelease: z.literal(false),
    body: z.string().max(20000).nullable(),
    html_url: z.string().url(),
  }),
  minimumVersion: z.string().optional(),
  android: z.object({
    name: z.string(),
    url: z.string().url(),
    size: z.number().int().min(100001).max(262144000),
    sha256: z.string().regex(/^[a-f0-9]{64}$/i),
    versionCode: z.number().int().positive().max(2100000000),
  }),
});
export type AndroidUpdate = {
  version: string;
  notes: string;
  mandatory: boolean;
  asset: z.infer<typeof manifestSchema>['android'];
};
export function availableUpdate(
  input: unknown,
  installed: { version: string; versionCode: number },
): AndroidUpdate | null {
  const manifest = manifestSchema.parse(input);
  const version = normalizeVersion(manifest.release.tag_name);
  const name = `SafeEat-${version}.apk`;
  if (
    manifest.android.name !== name ||
    manifest.android.url !== `${updateOrigin}/safeeat-downloads/releases/${name}` ||
    manifest.release.html_url !== `https://github.com/A1VAR0W/SafeEat/releases/tag/v${version}`
  )
    throw new Error('Untrusted update asset');
  if (
    compareVersions(version, installed.version) <= 0 ||
    manifest.android.versionCode <= installed.versionCode
  )
    return null;
  if (manifest.minimumVersion && compareVersions(manifest.minimumVersion, version) > 0)
    throw new Error('Invalid minimum version');
  return {
    version,
    notes: (manifest.release.body ?? '')
      .replace(/<!--[\s\S]*?-->/g, '')
      .replace(/^#{1,6}\s*/gm, '')
      .slice(0, 5000)
      .trim(),
    mandatory:
      !!manifest.minimumVersion && compareVersions(installed.version, manifest.minimumVersion) < 0,
    asset: manifest.android,
  };
}
