import { Platform } from 'react-native';
import Constants from 'expo-constants';
import { availableUpdate, updateManifestUrl, type AndroidUpdate } from './model';
import { nativeUpdater } from './native';

let checkedAt = 0;
let running: Promise<AndroidUpdate | null> | null = null;
const listeners = new Set<(update: AndroidUpdate) => void>();
export function subscribeToUpdates(listener: (update: AndroidUpdate) => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
export async function checkForUpdate(
  manual = false,
): Promise<'available' | 'current' | 'unavailable'> {
  if (
    Platform.OS !== 'android' ||
    !nativeUpdater ||
    Constants.expoConfig?.extra?.environment !== 'production'
  )
    return 'unavailable';
  if (!manual && Date.now() - checkedAt < 3600000) return 'current';
  if (!running) {
    checkedAt = Date.now();
    running = (async () => {
      const controller = new AbortController();
      const timeout = setTimeout(() => controller.abort(), 8000);
      try {
        const response = await fetch(updateManifestUrl, {
          signal: controller.signal,
          cache: 'no-store',
        });
        if (!response.ok) throw new Error(`Update check HTTP ${response.status}`);
        const raw = await response.text();
        if (raw.length > 40000) throw new Error('Update manifest too large');
        return availableUpdate(JSON.parse(raw), nativeUpdater.installedVersion());
      } finally {
        clearTimeout(timeout);
      }
    })();
  }
  try {
    const update = await running;
    if (update) listeners.forEach((listener) => listener(update));
    return update ? 'available' : 'current';
  } catch (error) {
    console.warn('[SafeEat updates]', error instanceof Error ? error.message : 'Check failed');
    return 'unavailable';
  } finally {
    running = null;
  }
}
