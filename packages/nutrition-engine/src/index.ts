import { NutritionInputSchema, type NutritionInput, type NutritionScore } from '@safeeat/contracts';
export function scoreNutrition(value: NutritionInput): NutritionScore {
  const parsed = NutritionInputSchema.safeParse(value);
  const empty = {
    score: null,
    breakdown: [],
    explanation:
      'Datos insuficientes o inválidos por 100 g/ml para calcular la calidad nutricional.',
    version: '1.0.0',
  };
  if (!parsed.success) return empty;
  const n = parsed.data;
  if (
    n.energyKcal === undefined ||
    n.sugars === undefined ||
    n.saturatedFat === undefined ||
    n.salt === undefined ||
    n.fibre === undefined ||
    n.protein === undefined
  )
    return empty;
  if (n.sugars + n.saturatedFat + n.fibre + n.protein + n.salt > 105) return empty;
  const breakdown = [
    { label: 'Base', points: 70 },
    { label: 'Energía', points: -Math.min(15, Math.floor(n.energyKcal / 100) * 3) },
    { label: 'Azúcar', points: -Math.min(25, Math.floor(n.sugars / 5) * 5) },
    { label: 'Grasas saturadas', points: -Math.min(20, Math.floor(n.saturatedFat / 2) * 4) },
    { label: 'Sal', points: -Math.min(20, Math.floor(n.salt / 0.3) * 4) },
    { label: 'Fibra', points: Math.min(15, Math.floor(n.fibre / 2) * 5) },
    { label: 'Proteína', points: Math.min(10, Math.floor(n.protein / 5) * 5) },
  ];
  const raw = breakdown.reduce((sum, part) => sum + part.points, 0);
  return {
    score: Math.round(Math.max(0, Math.min(100, raw)) / 5) * 5,
    breakdown,
    explanation:
      'Indicador orientativo SafeEat, redondeado a 5 puntos. No es Nutri-Score ni una evaluación médica. Es independiente del gluten.',
    version: '1.0.0',
  };
}
