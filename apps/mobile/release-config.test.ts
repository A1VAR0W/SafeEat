import { afterEach, expect, it, vi } from 'vitest';
afterEach(() => {
  vi.unstubAllEnvs();
  vi.resetModules();
});
it.each([
  'http://example.org',
  'https://localhost',
  'https://127.0.0.1',
  'https://192.168.1.243',
  'https://server.local',
  'https://[::1]',
  'https://example.org/path',
  'https://user:secret@example.org',
])('rejects unsafe release origin %s', async (url) => {
  vi.resetModules();
  vi.stubEnv('APP_ENV', 'preproduction');
  vi.stubEnv('API_PUBLIC_URL', url);
  vi.stubEnv('PUBLIC_APP_URL', 'https://example.org');
  await expect(import('./app.config')).rejects.toThrow('public HTTPS origins');
});
it('accepts a verified HTTPS domain with an explicit port', async () => {
  vi.stubEnv('APP_ENV', 'preproduction');
  vi.stubEnv('API_PUBLIC_URL', 'https://example.org:10000');
  vi.stubEnv('PUBLIC_APP_URL', 'https://example.org:10000');
  expect((await import('./app.config')).default.extra?.apiUrl).toBe('https://example.org:10000');
});
it('accepts the isolated preproduction base path', async () => {
  vi.stubEnv('APP_ENV', 'preproduction');
  vi.stubEnv('API_PUBLIC_URL', 'https://example.org:10000/preproduction');
  vi.stubEnv('PUBLIC_APP_URL', 'https://example.org:10000/preproduction');
  expect((await import('./app.config')).default.extra?.environment).toBe('preproduction');
});
