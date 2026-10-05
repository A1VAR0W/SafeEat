import Constants from 'expo-constants';
import { parseShareLink } from '@safeeat/contracts';

// https://docs.expo.dev/router/advanced/native-intent/
export function redirectSystemPath({ path }: { path: string; initial: boolean }) {
  if (path === 'home' || path === '/home' || /^safeeat:\/\/home\/?(?:\?.*)?$/.test(path))
    return '/';
  if (!/^https?:\/\//i.test(path)) return path;
  const link = parseShareLink(path, String(Constants.expoConfig?.extra?.publicUrl ?? ''));
  return link ? `/${link.type === 'product' ? 'p' : 'r'}/${link.token}` : '/invalid-link';
}
