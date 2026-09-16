import { it, expect } from 'vitest';
import { scoreNutrition } from './index';
const complete = { energyKcal: 250, sugars: 10, saturatedFat: 2, salt: 0.4, fibre: 6, protein: 10 };
it('deterministic, explained and rounded', () => {
  const r = scoreNutrition(complete);
  expect(r.score).toBe(70);
  expect(r.breakdown.length).toBe(7);
  expect(r).toEqual(scoreNutrition(complete));
});
it.each([
  {},
  { ...complete, fibre: undefined },
  { ...complete, salt: -1 },
  { ...complete, energyKcal: NaN },
  { ...complete, sugars: 101 },
  { ...complete, protein: Infinity },
])('rejects incomplete/invalid data', (n) => expect(scoreNutrition(n).score).toBeNull());
it('zero values are valid', () =>
  expect(
    scoreNutrition({ energyKcal: 0, sugars: 0, saturatedFat: 0, salt: 0, fibre: 0, protein: 0 })
      .score,
  ).toBe(70));
it('bounded score', () =>
  expect(
    scoreNutrition({
      energyKcal: 900,
      sugars: 50,
      saturatedFat: 20,
      salt: 20,
      fibre: 0,
      protein: 0,
    }).score,
  ).toBe(0));
