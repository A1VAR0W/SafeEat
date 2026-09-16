import { useSyncExternalStore } from 'react';
import * as SecureStore from 'expo-secure-store';
import Constants from 'expo-constants';
import { SessionSchema, type Session, type User } from '@safeeat/contracts';
import { es } from './i18n';
export const apiUrl = String(Constants.expoConfig?.extra?.apiUrl ?? '').replace(/\/$/, '');
type State = { user: User | null; accessToken: string | null; ready: boolean };
let state: State = { user: null, accessToken: null, ready: false };
const listeners = new Set<() => void>();
const emit = (next: State) => {
  state = next;
  listeners.forEach((fn) => fn());
};
const storageKey = 'safeeat.session';
let refreshing: Promise<string> | null = null;
export class RequestError extends Error {
  constructor(
    public code: string,
    message: string,
    public status = 0,
  ) {
    super(message);
  }
}
export const getSession = () => state;
export function useSession() {
  return useSyncExternalStore(
    (fn) => {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    () => state,
    () => state,
  );
}
async function raw(path: string, init: RequestInit = {}) {
  let response: Response;
  try {
    response = await fetch(`${apiUrl}/api/v1${path}`, {
      ...init,
      signal: AbortSignal.timeout(15000),
      headers: { 'Content-Type': 'application/json', ...init.headers },
    });
  } catch {
    throw new RequestError('NETWORK_UNAVAILABLE', es.network);
  }
  if (!response.ok) {
    const body = (await response.json().catch(() => null)) as {
      error?: { code?: string; message?: string };
    } | null;
    throw new RequestError(
      body?.error?.code ?? 'REQUEST_FAILED',
      body?.error?.message ?? 'No se ha podido completar la operación.',
      response.status,
    );
  }
  return response.status === 204 ? undefined : response.json();
}
export async function acceptSession(input: unknown) {
  const value = SessionSchema.parse(input);
  await SecureStore.setItemAsync(
    storageKey,
    JSON.stringify({ refreshToken: value.refreshToken, user: value.user }),
    { keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY },
  );
  emit({ user: value.user, accessToken: value.accessToken, ready: true });
}
export async function refresh() {
  if (refreshing) return refreshing;
  refreshing = (async () => {
    const stored = await SecureStore.getItemAsync(storageKey);
    if (!stored) throw new RequestError('UNAUTHORIZED', 'Inicia sesión para continuar.', 401);
    const saved = JSON.parse(stored) as Pick<Session, 'refreshToken' | 'user'>;
    try {
      const data = SessionSchema.parse(
        await raw('/auth/refresh', {
          method: 'POST',
          body: JSON.stringify({ refreshToken: saved.refreshToken }),
        }),
      );
      await acceptSession(data);
      return data.accessToken;
    } catch (error) {
      if (error instanceof RequestError && error.status === 401) await clearSession();
      throw error;
    } finally {
      refreshing = null;
    }
  })();
  return refreshing;
}
export async function restoreSession() {
  try {
    const stored = await SecureStore.getItemAsync(storageKey);
    if (stored) {
      const saved = JSON.parse(stored) as { user: User };
      emit({ user: saved.user, accessToken: null, ready: true });
      await refresh();
    }
  } catch (error) {
    if (!(error instanceof RequestError)) await clearSession();
  } finally {
    emit({ ...state, ready: true });
  }
}
export async function clearSession() {
  await SecureStore.deleteItemAsync(storageKey);
  emit({ user: null, accessToken: null, ready: true });
}
export async function logout() {
  const stored = await SecureStore.getItemAsync(storageKey);
  // Require server revocation before reporting logout as complete.
  if (stored) {
    const saved = JSON.parse(stored) as { refreshToken: string };
    await raw('/auth/logout', {
      method: 'POST',
      body: JSON.stringify({ refreshToken: saved.refreshToken }),
    });
  }
  await clearSession();
}
export async function api<T>(
  path: string,
  init: RequestInit = {},
  authenticated = false,
): Promise<T> {
  let token = state.accessToken;
  if (authenticated && !token) token = await refresh();
  try {
    return (await raw(path, {
      ...init,
      headers: { ...init.headers, ...(token ? { Authorization: `Bearer ${token}` } : {}) },
    })) as T;
  } catch (error) {
    if (error instanceof RequestError && error.status === 401 && state.user) {
      token = await refresh();
      return (await raw(path, {
        ...init,
        headers: { ...init.headers, Authorization: `Bearer ${token}` },
      })) as T;
    }
    throw error;
  }
}
