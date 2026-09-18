import { ApiError } from '../plugins/errors';
export async function providerJson(
  url: string,
  init: RequestInit = {},
  timeoutMs = 8000,
): Promise<unknown> {
  try {
    const response = await fetch(url, {
      ...init,
      signal: AbortSignal.timeout(timeoutMs),
      redirect: 'error',
    });
    if (response.status === 404) return null;
    if (!response.ok) throw new Error('provider response');
    return await response.json();
  } catch {
    throw new ApiError(
      503,
      'PROVIDER_UNAVAILABLE',
      'La fuente de información no está disponible. Inténtalo de nuevo más tarde.',
    );
  }
}
/** Single-process provider budget. Deployment intentionally runs a single API replica. */
export class ProviderBudget {
  private calls: number[] = [];
  constructor(
    private limit: number,
    private windowMs = 60000,
  ) {}
  take() {
    const now = Date.now();
    this.calls = this.calls.filter((t) => t > now - this.windowMs);
    if (this.calls.length >= this.limit)
      throw new ApiError(
        503,
        'PROVIDER_BUSY',
        'La fuente está ocupada. Espera un minuto antes de volver a intentarlo.',
      );
    this.calls.push(now);
  }
}
export function safeHttpUrl(value: unknown): string | undefined {
  if (typeof value !== 'string') return;
  try {
    const u = new URL(value);
    return ['https:', 'http:'].includes(u.protocol) ? u.toString() : undefined;
  } catch {
    return;
  }
}
