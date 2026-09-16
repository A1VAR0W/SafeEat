import type { EvidenceItem, GlutenAssessment } from '@safeeat/contracts';

export type GlutenInput = {
  ingredients?: string;
  allergens?: string;
  traces?: string;
  declaration?: string;
  source: string;
  sourceUrl?: string;
  origin?: 'provider' | 'manual' | 'ocr';
  reviewed?: boolean;
};
export function normalize(text: string): string {
  return text
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(/[‐‑–—_-]/g, ' ')
    .replace(/[^a-z0-9\s.,;:!?()]/g, ' ')
    .replace(/[\t ]+/g, ' ')
    .trim();
}
const hazards =
  /\b(trigo|wheat|cebada|barley|centeno|rye|espelta|spelt|kamut|khorasan|triticale|gluten|malta|malt|avena|oats?)\b/g;
const freeClaim =
  /\b(sin gluten|gluten free|libre de gluten|no contiene gluten|does not contain gluten)\b/g;
const uncertainty =
  /\b(no es|not|no garantiza|no garantizado|not guaranteed|no certificado|no certificad[ao]|no apto|no es apto|no se puede|no podemos|cannot|uncertain|desconocid[ao]|desconoce|desconocemos|posible|posiblemente|podria|could|might|consult[ae]|consultar|pregunt[ae]|ask|opcion|option|excepto|except|sin confirmar|no confirmado|unconfirmed)\b|[?]/;

/** Explicit evidence only. A missing hazard is never evidence of absence. */
export function assessGluten(input: GlutenInput): GlutenAssessment {
  const evidence: EvidenceItem[] = [];
  const add = (text: string, kind: EvidenceItem['kind']) =>
    evidence.push({
      source: input.source,
      text,
      kind,
      ...(input.sourceUrl ? { url: input.sourceUrl } : {}),
    });
  let contains = false,
    traces = false,
    oats = false,
    claim = false,
    ambiguous = false;
  const fields = [
    ['ingredients', input.ingredients],
    ['allergens', input.allergens],
    ['traces', input.traces],
    ['declaration', input.declaration],
  ] as const;
  for (const [field, raw] of fields) {
    if (!raw?.trim()) continue;
    const normalized = normalize(raw);
    for (const clause of normalized.split(/[.;\n]+/).filter(Boolean)) {
      let rest = clause;
      const free = [...clause.matchAll(freeClaim)];
      if (free.length) {
        const unclear =
          uncertainty.test(clause) ||
          /\b(no|not|without)\s+(?:es\s+|is\s+|a\s+|un\s+)?(?:sin gluten|gluten free|libre de gluten)\b/.test(
            clause,
          );
        if (unclear) {
          ambiguous = true;
          add(clause.trim(), 'uncertainty');
        } else {
          claim = true;
          add(clause.trim(), 'claim');
        }
        rest = rest.replace(freeClaim, ' ');
      }
      // Negated cereals are not positive gluten evidence, and never establish gluten-free.
      rest = rest.replace(
        /\b(sin|libre de|no contiene|without|free from|does not contain)\s+(trigo|wheat|cebada|barley|centeno|rye|avena|oats?)\b/g,
        () => {
          ambiguous = true;
          return ' ';
        },
      );
      // A precaution applies only after its marker, and ends at an explicit ingredients/allergens section.
      rest = rest.replace(
        /\b(?:puede[n]? contener|puede tener|trazas de|may contain|may contain traces of|traces of|processed in|made in|elaborado en|fabricado en|contaminacion cruzada|cross contamination)\b.*?(?=\b(?:ingredientes|ingredients|alergenos|allergens|contiene:|contains:)\b|$)/g,
        (scope) => {
          if (scope.match(hazards)) {
            traces = true;
            add(scope.trim(), 'trace');
          }
          return ' ';
        },
      );
      const hits = rest.match(hazards) ?? [];
      if (field === 'traces' && hits.length) {
        traces = true;
        add(clause.trim(), 'trace');
        continue;
      }
      if (hits.length) {
        if (uncertainty.test(rest) || /\b(no|not|sin)\b/.test(rest)) {
          ambiguous = true;
          add(clause.trim(), 'uncertainty');
        } else {
          const onlyOats = hits.every((h) => /^(avena|oats?)$/.test(h));
          if (onlyOats) {
            oats = true;
            add(clause.trim(), 'uncertainty');
          } else {
            contains = true;
            add(clause.trim(), field === 'allergens' ? 'allergen' : 'ingredient');
          }
        }
      }
    }
  }
  const reasons: GlutenAssessment['reasons'] = [];
  let status: GlutenAssessment['status'] = 'UNKNOWN';
  let confidence = 0;
  if (contains) {
    status = 'CONTAINS_GLUTEN';
    confidence = 0.95;
    reasons.push({
      code: 'CEREAL_DETECTED',
      message: 'Se han detectado gluten o cereales con gluten en los ingredientes o alérgenos.',
    });
  } else if (traces) {
    status = 'MAY_CONTAIN_TRACES';
    confidence = 0.9;
    reasons.push({
      code: 'PRECAUTIONARY_LABEL',
      message: 'La información disponible advierte de posibles trazas o contaminación cruzada.',
    });
  } else if (claim && !ambiguous && (input.origin !== 'ocr' || input.reviewed)) {
    status = 'GLUTEN_FREE';
    confidence = input.origin === 'provider' ? 0.75 : 0.65;
    reasons.push({
      code: 'EXPLICIT_CLAIM',
      message: 'El producto está declarado explícitamente como sin gluten en la fuente indicada.',
    });
  } else {
    reasons.push({
      code: 'INSUFFICIENT_EVIDENCE',
      message: 'No disponemos de información suficiente para confirmar que sea seguro.',
    });
  }
  if (oats && status !== 'GLUTEN_FREE')
    reasons.push({
      code: 'OATS_UNCERTAIN',
      message: 'La avena requiere una declaración fiable de ausencia de gluten.',
    });
  if (claim && (contains || traces || ambiguous))
    reasons.push({
      code: 'CONTRADICTORY_EVIDENCE',
      message:
        'Hay información contradictoria; prevalece la precaución. Comprueba la etiqueta y consulta al fabricante.',
    });
  if (input.origin === 'ocr') {
    confidence = Math.min(confidence, 0.65);
    reasons.push({
      code: 'OCR_LIMITATION',
      message:
        'El reconocimiento de texto puede omitir palabras. Revisa toda la etiqueta original; no se analiza el aspecto del envase.',
    });
  }
  return { status, confidence, reasons, evidence, engineVersion: '1.0.0' };
}
