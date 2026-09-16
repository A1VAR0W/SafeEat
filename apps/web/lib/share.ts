import { cache } from 'react';
import { notFound } from 'next/navigation';
import { ShareSchema } from '@safeeat/contracts';
export const getShare = cache(async (token: string) => {
  if (!/^[a-f0-9]{48}$/.test(token)) notFound();
  const api = process.env.API_INTERNAL_URL ?? process.env.API_PUBLIC_URL ?? 'http://localhost:4000';
  const response = await fetch(`${api}/api/v1/public/shares/${token}`, {
    cache: 'no-store',
    signal: AbortSignal.timeout(10000),
  });
  if (response.status === 404) notFound();
  if (!response.ok) throw new Error('No se puede consultar la fuente en este momento.');
  return ShareSchema.parse(await response.json());
});
export function publicUrl() {
  const raw = process.env.PUBLIC_APP_URL ?? 'http://localhost:3000';
  return raw.replace(/\/$/, '');
}
