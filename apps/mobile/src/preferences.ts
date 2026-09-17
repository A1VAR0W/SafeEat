import { useSyncExternalStore } from 'react';
import AsyncStorage from '@react-native-async-storage/async-storage';
import { z } from 'zod';
const schema = z.object({
  theme: z.enum(['system', 'light', 'dark']),
  textSize: z.enum(['small', 'standard', 'large']),
});
export type Preferences = z.infer<typeof schema>;
let preferences: Preferences = { theme: 'system', textSize: 'standard' };
let revision = 0;
let pending: Promise<void> = Promise.resolve();
const listeners = new Set<() => void>();
const key = 'safeeat.preferences.v2';
export const getPreferences = () => preferences;
export async function loadPreferences() {
  const current = revision;
  const saved = await AsyncStorage.getItem(key).catch(() => null);
  if (!saved || current !== revision) return;
  try {
    preferences = schema.parse(JSON.parse(saved));
    listeners.forEach((fn) => fn());
  } catch {
    /* Corrupt local settings fall back to the defaults. */
  }
}
export function setPreferences(value: Partial<Preferences>) {
  preferences = schema.parse({ ...preferences, ...value });
  ++revision;
  listeners.forEach((fn) => fn());
  const saved = JSON.stringify(preferences);
  const next = pending.then(() => AsyncStorage.setItem(key, saved));
  pending = next.catch(() => {});
  return next;
}
export function usePreferences() {
  return useSyncExternalStore(
    (fn) => {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    getPreferences,
    getPreferences,
  );
}
export const textScales = { small: 0.9, standard: 1, large: 1.15 } as const;
