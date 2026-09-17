import { afterEach, beforeEach, expect, it, vi } from 'vitest';
afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});
const mocks = vi.hoisted(() => ({
  stored: null as string | null,
  clear: vi.fn(async () => {}),
  fetch: vi.fn(),
}));
vi.mock('expo-secure-store', () => ({
  WHEN_UNLOCKED_THIS_DEVICE_ONLY: 1,
  getItemAsync: async () => mocks.stored,
  setItemAsync: async (_key: string, value: string) => {
    mocks.stored = value;
  },
  deleteItemAsync: async () => {
    mocks.stored = null;
  },
}));
vi.mock('expo-constants', () => ({
  default: { expoConfig: { extra: { apiUrl: 'https://example.org' } } },
}));
vi.mock('./query', () => ({ clearPrivateCache: mocks.clear }));
vi.mock('@react-native-community/netinfo', () => ({
  default: { fetch: async () => ({ isConnected: true, isInternetReachable: true }) },
}));
const session = (n: number) => ({
  user: {
    id: `00000000-0000-4000-8000-${String(n).padStart(12, '0')}`,
    email: `user${n}@example.org`,
  },
  accessToken: `access-${n}`,
  refreshToken: String(n).repeat(96),
});
beforeEach(() => {
  vi.resetModules();
  mocks.stored = null;
  mocks.clear.mockClear();
  mocks.fetch.mockReset();
  vi.stubGlobal('fetch', mocks.fetch);
});
it('omits JSON content type for bodyless favorites and account deletion', async () => {
  const s = await import('./session');
  await s.acceptSession(session(1));
  mocks.fetch.mockImplementation(async () => new Response(null, { status: 204 }));
  await s.api('/favorites/products/off-1', { method: 'POST' }, true);
  await s.api('/auth/me', { method: 'DELETE' }, true);
  for (const [, init] of mocks.fetch.mock.calls)
    expect(init.headers['Content-Type']).toBeUndefined();
});
it('sends requests on React Native where AbortSignal.timeout is unavailable', async () => {
  vi.stubGlobal('AbortSignal', {});
  const s = await import('./session');
  mocks.fetch.mockResolvedValue(new Response(JSON.stringify({ environment: 'production' })));
  expect(await s.api('/meta')).toEqual({ environment: 'production' });
  expect(mocks.fetch).toHaveBeenCalledTimes(1);
});
it('reports request timeout separately from an HTTP or authentication error', async () => {
  const s = await import('./session');
  vi.useFakeTimers();
  mocks.fetch.mockImplementation(
    (_url, init) =>
      new Promise((_resolve, reject) => {
        init.signal.addEventListener('abort', () => reject(new Error('aborted')));
      }),
  );
  const pending = expect(s.api('/meta')).rejects.toMatchObject({ code: 'REQUEST_TIMEOUT' });
  await vi.advanceTimersByTimeAsync(15000);
  await pending;
});
it('coalesces concurrent refresh and recovers after missing credentials', async () => {
  const s = await import('./session');
  await expect(s.refresh()).rejects.toMatchObject({ status: 401 });
  await s.acceptSession(session(1));
  mocks.fetch.mockResolvedValue(new Response(JSON.stringify(session(1))));
  await Promise.all([s.refresh(), s.refresh(), s.refresh()]);
  expect(mocks.fetch).toHaveBeenCalledTimes(1);
});
it('cancels obsolete searches through an ordinary AbortController on React Native', async () => {
  vi.stubGlobal('AbortSignal', {});
  const s = await import('./session');
  const controller = new AbortController();
  let requestSignal: AbortSignal | undefined;
  mocks.fetch.mockImplementation(
    (_url, init) =>
      new Promise((_resolve, reject) => {
        requestSignal = init.signal;
        init.signal.addEventListener('abort', () =>
          reject(Object.assign(new Error('cancelled'), { name: 'AbortError' })),
        );
      }),
  );
  const pending = expect(
    s.api('/products/search?q=old', { signal: controller.signal }),
  ).rejects.toMatchObject({ name: 'AbortError' });
  controller.abort();
  await pending;
  expect(requestSignal?.aborted).toBe(true);
});
it('does not resurrect a session when refresh completes after clearing it', async () => {
  const s = await import('./session');
  await s.acceptSession(session(1));
  let finish!: (response: Response) => void;
  mocks.fetch.mockImplementation(
    () =>
      new Promise<Response>((resolve) => {
        finish = resolve;
      }),
  );
  const pending = s.refresh();
  await vi.waitFor(() => expect(mocks.fetch).toHaveBeenCalled());
  await s.clearSession();
  finish(new Response(JSON.stringify(session(1))));
  await expect(pending).rejects.toMatchObject({ code: 'SESSION_CHANGED' });
  expect(s.getSession().user).toBeNull();
  expect(mocks.stored).toBeNull();
  expect(mocks.clear).toHaveBeenCalled();
});
it('does not replace a newly logged-in user with an old refresh', async () => {
  const s = await import('./session');
  await s.acceptSession(session(1));
  let finish!: (response: Response) => void;
  mocks.fetch.mockImplementation(
    () =>
      new Promise<Response>((resolve) => {
        finish = resolve;
      }),
  );
  const pending = s.refresh();
  await vi.waitFor(() => expect(mocks.fetch).toHaveBeenCalled());
  await s.acceptSession(session(2));
  finish(new Response(JSON.stringify(session(1))));
  await expect(pending).rejects.toMatchObject({ code: 'SESSION_CHANGED' });
  expect(s.getSession().user?.id).toBe(session(2).user.id);
  expect(JSON.parse(mocks.stored!).user.id).toBe(session(2).user.id);
});
it('clears private cached data after server revocation and missing SecureStore state', async () => {
  const s = await import('./session');
  await s.restoreSession();
  expect(mocks.clear).toHaveBeenCalledTimes(1);
  await s.acceptSession(session(1));
  mocks.fetch.mockResolvedValue(new Response('{}', { status: 401 }));
  await expect(s.refresh()).rejects.toMatchObject({ status: 401 });
  expect(mocks.stored).toBeNull();
  expect(s.getSession().user).toBeNull();
});
