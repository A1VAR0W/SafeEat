import { beforeEach, expect, it, vi } from 'vitest';
const storage = vi.hoisted(() => ({
  getItem: vi.fn(),
  setItem: vi.fn(async (_key: string, _value: string) => {}),
}));
vi.mock('@react-native-async-storage/async-storage', () => ({ default: storage }));
beforeEach(() => {
  vi.resetModules();
  storage.getItem.mockReset();
  storage.setItem.mockClear();
});
it('persists mutually exclusive theme and accessible text presets, restoring them on next launch', async () => {
  let preferences = await import('./preferences');
  expect(preferences.getPreferences()).toEqual({ theme: 'system', textSize: 'standard' });
  await preferences.setPreferences({ theme: 'dark', textSize: 'large' });
  await preferences.setPreferences({ theme: 'light' });
  const saved = storage.setItem.mock.calls.at(-1)![1];
  storage.getItem.mockResolvedValue(saved);
  vi.resetModules();
  preferences = await import('./preferences');
  await preferences.loadPreferences();
  expect(preferences.getPreferences()).toEqual({ theme: 'light', textSize: 'large' });
  await preferences.setPreferences({ theme: 'system' });
  expect(preferences.getPreferences().theme).toBe('system');
});
it('does not overwrite a new choice with slow hydration or accept corrupt settings', async () => {
  let resolve!: (value: string) => void;
  storage.getItem.mockImplementation(
    () =>
      new Promise<string>((r) => {
        resolve = r;
      }),
  );
  const preferences = await import('./preferences');
  const loading = preferences.loadPreferences();
  await preferences.setPreferences({ theme: 'dark' });
  resolve('{"theme":"light","textSize":"small"}');
  await loading;
  expect(preferences.getPreferences().theme).toBe('dark');
  storage.getItem.mockResolvedValue('{broken');
  await preferences.loadPreferences();
  expect(preferences.getPreferences().theme).toBe('dark');
});
