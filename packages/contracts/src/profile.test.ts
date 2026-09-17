import { expect, it } from 'vitest';
import {
  registrationSchema,
  UserSchema,
  dateOfBirthSchema,
  emailChangeSchema,
  deleteAccountSchema,
} from './index';
const valid = {
  firstName: ' Ana ',
  dateOfBirth: '2000-02-29',
  email: 'ana@example.org',
  password: 'a-secure-password-123',
};
it('requires a valid date and name for registration, without requiring surname or marketing consent', () => {
  expect(registrationSchema.parse(valid)).toMatchObject({
    firstName: 'Ana',
    productUpdatesConsent: false,
  });
  expect(registrationSchema.safeParse({ ...valid, firstName: '' }).success).toBe(false);
  for (const date of [
    '2001-02-29',
    '2020-04-31',
    '2030-01-01',
    new Date().toISOString().slice(0, 10),
    '23',
  ])
    expect(dateOfBirthSchema.safeParse(date).success).toBe(false);
  expect(
    registrationSchema.safeParse({ email: valid.email, password: valid.password }).success,
  ).toBe(false);
});
it('reads legacy user contracts without inventing personal data or consent', () => {
  expect(
    UserSchema.parse({ id: '00000000-0000-4000-8000-000000000001', email: valid.email }),
  ).toMatchObject({ firstName: null, dateOfBirth: null, productUpdatesConsent: false });
});
it('rejects changing immutable profile data through email updates and requires deliberate account deletion', () => {
  expect(
    emailChangeSchema.safeParse({
      email: valid.email,
      currentPassword: valid.password,
      firstName: 'Changed',
    }).success,
  ).toBe(false);
  expect(deleteAccountSchema.safeParse({ currentPassword: valid.password }).success).toBe(false);
  expect(
    deleteAccountSchema.safeParse({ currentPassword: valid.password, confirmation: 'ELIMINAR' })
      .success,
  ).toBe(true);
});
