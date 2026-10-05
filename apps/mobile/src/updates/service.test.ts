import { afterEach, beforeEach, expect, it, vi } from 'vitest';
const platform = vi.hoisted(() => ({ OS: 'android' }));
vi.mock('react-native', () => ({ Platform: platform }));
vi.mock('expo-constants', () => ({
  default: { expoConfig: { extra: { environment: 'production' } } },
}));
vi.mock('./native', () => ({
  nativeUpdater: { installedVersion: () => ({ version: '1.0.0', versionCode: 76 }) },
}));
beforeEach(() => {
  vi.resetModules();
  platform.OS = 'android';
  vi.spyOn(console, 'warn').mockImplementation(() => {});
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
it.each(['ios', 'web'])('does no update networking on %s', async (os) => {
  platform.OS = os;
  const fetcher = vi.fn();
  vi.stubGlobal('fetch', fetcher);
  expect(await (await import('./service')).checkForUpdate()).toBe('unavailable');
  expect(fetcher).not.toHaveBeenCalled();
});
it('network failure stays silent and throttles subsequent automatic checks', async () => {
  const fetcher = vi.fn().mockRejectedValue(new Error('Offline'));
  vi.stubGlobal('fetch', fetcher);
  const { checkForUpdate } = await import('./service');
  expect(await checkForUpdate()).toBe('unavailable');
  await checkForUpdate();
  expect(fetcher).toHaveBeenCalledTimes(1);
  expect(console.warn).toHaveBeenCalled();
});
it('handles absent releases, rate limits and malformed manifests without throwing', async () => {
  const fetcher = vi
    .fn()
    .mockResolvedValueOnce({ ok: false, status: 404 })
    .mockResolvedValueOnce({ ok: false, status: 429 })
    .mockResolvedValueOnce({ ok: true, text: async () => '{"assets":[]}' });
  vi.stubGlobal('fetch', fetcher);
  const { checkForUpdate } = await import('./service');
  for (let i = 0; i < 3; i++) expect(await checkForUpdate(true)).toBe('unavailable');
});
it('shares concurrent requests instead of repeatedly fetching the release', async () => {
  let finish!: (value: { ok: boolean; status: number }) => void;
  const fetcher = vi.fn(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  vi.stubGlobal('fetch', fetcher);
  const { checkForUpdate } = await import('./service');
  const first = checkForUpdate(true),
    second = checkForUpdate(true);
  finish({ ok: false, status: 503 });
  await Promise.all([first, second]);
  expect(fetcher).toHaveBeenCalledTimes(1);
});
