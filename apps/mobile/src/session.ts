import { useSyncExternalStore } from 'react';
import * as SecureStore from 'expo-secure-store';
import Constants from 'expo-constants';
import NetInfo from '@react-native-community/netinfo';
import { SessionSchema, type Session, type User } from '@safeeat/contracts';
import { es } from './i18n';
import { clearPrivateCache } from './query';
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
let generation = 0;
let storageWork: Promise<void> = Promise.resolve();
const serializeStorage = (work: () => Promise<void>) => {
  const next = storageWork.then(work, work);
  storageWork = next.catch(() => {});
  return next;
};
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
  if (!/^https?:\/\//.test(apiUrl))
    throw new RequestError(
      'CONFIGURATION_INVALID',
      'Esta versión de SafeEat no tiene una conexión configurada. Instala la versión actualizada.',
    );
  let response: Response;
  const controller = new AbortController();
  const cancel = () => controller.abort();
  if (init.signal?.aborted) controller.abort();
  else init.signal?.addEventListener('abort', cancel);
  let timedOut = false;
  const timeout = setTimeout(() => {
    timedOut = true;
    controller.abort();
  }, 15000);
  try {
    response = await fetch(`${apiUrl}/api/v1${path}`, {
      ...init,
      signal: controller.signal,
      headers: {
        ...(init.body !== undefined ? { 'Content-Type': 'application/json' } : {}),
        ...init.headers,
      },
    });
  } catch (error) {
    if (init.signal?.aborted) throw error;
    if (timedOut || (error instanceof Error && ['TimeoutError', 'AbortError'].includes(error.name)))
      throw new RequestError(
        'REQUEST_TIMEOUT',
        'SafeEat tarda demasiado en responder. Inténtalo de nuevo.',
      );
    const network = await NetInfo.fetch().catch(() => null);
    if (network?.isConnected === false || network?.isInternetReachable === false)
      throw new RequestError('OFFLINE', es.network);
    throw new RequestError(
      'CONNECTION_FAILED',
      'No se puede establecer una conexión segura con SafeEat. Comprueba tu conexión e inténtalo de nuevo.',
    );
  } finally {
    clearTimeout(timeout);
    init.signal?.removeEventListener('abort', cancel);
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
export async function acceptSession(input: unknown, expectedGeneration?: number) {
  const value = SessionSchema.parse(input);
  const current = expectedGeneration ?? ++generation;
  await serializeStorage(async () => {
    if (current !== generation) return;
    if (state.user?.id !== value.user.id) await clearPrivateCache();
    await SecureStore.setItemAsync(
      storageKey,
      JSON.stringify({ refreshToken: value.refreshToken, user: value.user }),
      { keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY },
    );
    if (current === generation)
      emit({ user: value.user, accessToken: value.accessToken, ready: true });
  });
}
export async function refresh() {
  if (refreshing) return refreshing;
  const current = generation;
  refreshing = (async () => {
    try {
      const stored = await SecureStore.getItemAsync(storageKey);
      if (!stored) throw new RequestError('UNAUTHORIZED', 'Inicia sesión para continuar.', 401);
      const saved = JSON.parse(stored) as Pick<Session, 'refreshToken' | 'user'>;
      const data = SessionSchema.parse(
        await raw('/auth/refresh', {
          method: 'POST',
          body: JSON.stringify({ refreshToken: saved.refreshToken }),
        }),
      );
      if (current !== generation)
        throw new RequestError('SESSION_CHANGED', 'La sesión ha cambiado.', 401);
      await acceptSession(data, current);
      if (current !== generation)
        throw new RequestError('SESSION_CHANGED', 'La sesión ha cambiado.', 401);
      return data.accessToken;
    } catch (error) {
      if (current === generation && error instanceof RequestError && error.status === 401)
        await clearSession();
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
    } else await clearSession();
  } catch (error) {
    if (!(error instanceof RequestError)) await clearSession();
  } finally {
    emit({ ...state, ready: true });
  }
}
export async function clearSession() {
  ++generation;
  emit({ user: null, accessToken: null, ready: true });
  await serializeStorage(async () => {
    await SecureStore.deleteItemAsync(storageKey);
    await clearPrivateCache();
  });
}
export async function logout() {
  // Revoke the current rotated credential, not one read before an in-flight refresh.
  if (refreshing) await refreshing.catch(() => {});
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
  const current = generation;
  try {
    return (await raw(path, {
      ...init,
      headers: { ...init.headers, ...(token ? { Authorization: `Bearer ${token}` } : {}) },
    })) as T;
  } catch (error) {
    if (
      error instanceof RequestError &&
      error.status === 401 &&
      state.user &&
      current === generation
    ) {
      token =
        state.accessToken && state.accessToken !== token ? state.accessToken : await refresh();
      return (await raw(path, {
        ...init,
        headers: { ...init.headers, Authorization: `Bearer ${token}` },
      })) as T;
    }
    throw error;
  }
}
