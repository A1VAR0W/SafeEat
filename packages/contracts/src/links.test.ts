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
it('isolates HTTPS links by environment base path', () => {
  const root = 'https://example.org:10000';
  const preproduction = `${root}/preproduction`;
  expect(parseShareLink(`${preproduction}/p/${token}`, preproduction)).toEqual({
    type: 'product',
    token,
  });
  expect(parseShareLink(`${root}/p/${token}`, preproduction)).toBeNull();
  expect(parseShareLink(`${preproduction}/p/${token}`, root)).toBeNull();
  expect(parseShareLink(`${root}/preproduction-other/p/${token}`, preproduction)).toBeNull();
  expect(parseShareLink(`safeeat://r/${token}`, preproduction)).toEqual({
    type: 'restaurant',
    token,
  });
});
