import { it, expect } from 'vitest';
import { parseShareLink } from './index';
const token = 'a'.repeat(48);
it('parses app and universal links', () => {
  expect(parseShareLink(`safeeat://p/${token}`, 'https://example.org')).toEqual({
    type: 'product',
    token,
  });
  expect(parseShareLink(`https://example.org/r/${token}`, 'https://example.org')).toEqual({
    type: 'restaurant',
    token,
  });
});
it('rejects foreign origins and invalid paths', () => {
  expect(parseShareLink(`https://evil.org/p/${token}`, 'https://example.org')).toBeNull();
  expect(parseShareLink('safeeat://p/123', 'https://example.org')).toBeNull();
});
