import { describe, expect, it } from 'vitest';
import { availableUpdate, compareVersions, updateOrigin } from './model';

function manifest(version = '1.0.1') {
  const name = `SafeEat-${version}.apk`;
  return {
    schemaVersion: 1,
    repository: 'A1VAR0W/SafeEat',
    release: {
      id: 1,
      tag_name: `v${version}`,
      draft: false,
      prerelease: false,
      body: '# Mejoras\n- Nuevo mapa',
      html_url: `https://github.com/A1VAR0W/SafeEat/releases/tag/v${version}`,
    },
    android: {
      name,
      url: `${updateOrigin}/safeeat-downloads/releases/${name}`,
      size: 123456789,
      sha256: 'a'.repeat(64),
      versionCode: 77,
    },
  };
}
const installed = { version: '1.0.0', versionCode: 76 };
describe('Android release validation', () => {
  it.each([
    ['1.0.0', '1.0.0', 0],
    ['1.0.1', '1.0.0', 1],
    ['v1.0.10', '1.0.9', 1],
    ['2.0.0', '1.99.99', 1],
    ['1.9.9', '1.10.0', -1],
  ])('compares %s and %s semantically', (a, b, result) =>
    expect(compareVersions(a, b)).toBe(result),
  );
  it('does nothing for equal or older releases and refuses Android downgrades', () => {
    expect(availableUpdate(manifest('1.0.0'), installed)).toBeNull();
    expect(availableUpdate(manifest('0.9.9'), installed)).toBeNull();
    expect(
      availableUpdate(
        { ...manifest(), android: { ...manifest().android, versionCode: 76 } },
        installed,
      ),
    ).toBeNull();
  });
  it('offers a newer stable APK with notes', () => {
    expect(availableUpdate(manifest(), installed)).toMatchObject({
      version: '1.0.1',
      mandatory: false,
      notes: 'Mejoras\n- Nuevo mapa',
    });
    expect(availableUpdate(manifest('1.0.10'), { ...installed, version: '1.0.9' })).not.toBeNull();
  });
  it('supports a minimum version without leaking release metadata into notes', () => {
    const m = manifest();
    m.release.body += '\n<!-- safeeat-min-version:1.0.1 -->';
    const update = availableUpdate({ ...m, minimumVersion: '1.0.1' }, installed);
    expect(update?.mandatory).toBe(true);
    expect(update?.notes).not.toContain('<!--');
  });
  it.each(['draft', 'prerelease'])('rejects %s releases', (key) => {
    const m = manifest();
    expect(() =>
      availableUpdate({ ...m, release: { ...m.release, [key]: true } }, installed),
    ).toThrow();
  });
  it.each([
    'https://evil.test/SafeEat-1.0.1.apk',
    `${updateOrigin}/safeeat-downloads/releases/../x.apk`,
    `${updateOrigin}/safeeat-downloads/releases/SafeEat-1.0.1.apk?redirect=evil`,
  ])('rejects untrusted URLs: %s', (url) => {
    const m = manifest();
    expect(() => availableUpdate({ ...m, android: { ...m.android, url } }, installed)).toThrow();
  });
  it('rejects missing APKs, invalid checksums and unsupported versions', () => {
    expect(() => availableUpdate({ ...manifest(), android: undefined }, installed)).toThrow();
    expect(() =>
      availableUpdate(
        { ...manifest(), android: { ...manifest().android, sha256: 'bad' } },
        installed,
      ),
    ).toThrow();
    expect(() => compareVersions('1.0.1-beta', '1.0.0')).toThrow();
  });
});
