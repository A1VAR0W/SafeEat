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
    .replace(/\s+/g, ' ')
    .trim();
}
// Word lists shared by the patterns below. The Rust port (crates/gluten-engine) uses the
// same strings; keep both in step and regenerate the golden cases after any change.
// Cereals with gluten in Spanish, English, French, Italian, German, Dutch, Portuguese and Catalan.
const CEREALS =
  'trigo|wheat|cebada|barley|centeno|rye|espelta|spelt|escanda|kamut|khorasan|triticale|einkorn|emmer|farro|freekeh|durum|seitan|bulgur|cuscus|couscous|ble|froment|orge|seigle|epeautre|frumento|grano duro|grano tenero|orzo|segale|weizen[a-z0-9]*|gerste[a-z0-9]*|roggen[a-z0-9]*|dinkel[a-z0-9]*|tarwe[a-z0-9]*|gerst|rogge|cevada|centeio|blat|ordi|segol';
const OATS = 'avena|oats?|avoine|hafer[a-z0-9]*|aveia|civada|havermout|havervlokken|havermeel';
const OTHER_HAZARDS = 'gluten|glutine|malta|malt|malte|malz[a-z0-9]*';
const HAZARD_WORDS = `${CEREALS}|${OTHER_HAZARDS}|${OATS}`;
const hazards = new RegExp(String.raw`\b(${HAZARD_WORDS})\b`, 'g');
const onlyOatsWord = new RegExp(`^(${OATS})$`);
const buckwheat =
  /\b(trigo sarraceno|buckwheat|ble noir|sarrasin|grano saraceno|trigo mourisco|blat negre|alforfon)\b/g;
const freeClaim =
  /\b(sin gluten|gluten free|libre de gluten|no contiene gluten|does not contain gluten|sans gluten|glutenfrei|senza glutine|sem gluten|sense gluten|glutenvrij)\b/g;
// A negation only cancels the hazard it governs: it must come right before it, separated
// at most by filler words or other hazards ("no contiene harina de trigo ni cebada").
// "Harina de trigo, no contiene leche" and "cerveza sin alcohol de cebada" stay positive.
const NEGATIONS =
  'no|not|sin|s[1l]n|ni|nor|never|nunca|without|free from|libre de|exent[oa]s? de|ausencia de|sans|ohne|senza|sem|sense|kein[a-z0-9]*';
const FILLERS =
  'de|del|la|el|los|las|lo|of|the|any|added|anadid[oa]s?|harinas?|flour|con|contienen?|contener|contain|contains|containing|es|is|un|una|a|an|trazas|traces|y|e|o|u|and|or|derivados|productos|ingredientes|ingredients|que|contengan|with|tiene|lleva';
const negationBefore = new RegExp(
  String.raw`\b(?:${NEGATIONS})(?:\s+(?:${FILLERS}|${HAZARD_WORDS}))*\s*:?\s*$`,
);
// Hedging about a hazard anywhere in the same list item.
const hedge =
  /\b(no garantiza|no garantizado|not guaranteed|no certificad[ao]|no apto|no es apto|no se puede|no podemos|cannot|uncertain|desconocid[ao]|desconoce|desconocemos|posible|posiblemente|podria|could|might|consult[ae]|consultar|pregunt[ae]|ask|opcion|option|excepto|except|sin confirmar|no confirmado|unconfirmed|no se ha comprobado|no se ha verificado|not tested|not verified)\b/;
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
      let rest = clause.replace(buckwheat, ' ');
      const claimContext = clause.replace(
        /\b(no contiene gluten|does not contain gluten)\b/g,
        'sin gluten',
      );
      // A recipe, historical statement or unverified/absent declaration is not a product claim.
      if (
        /\b(segun receta|receta|recipe|antes|formerly|previously|no declara|no pone|no indica|no consta|sin comprobar|sin evaluar|sin certificar|no se ha comprobado|no se ha verificado|not tested|not verified|not certified|cereales|cereals)\b/.test(
          claimContext,
        )
      ) {
        ambiguous = true;
        add(clause.trim(), 'uncertainty');
      }
      if (
        uncertainty.test(claimContext) ||
        /\b(no verificado|no evaluado|no apt[oa]s?|not suitable|sin verificar)\b/.test(claimContext)
      ) {
        ambiguous = true;
        add(clause.trim(), 'uncertainty');
      }
      const free = [...clause.matchAll(freeClaim)];
      if (free.length) {
        const unclear =
          uncertainty.test(claimContext) ||
          /\b(no|not|never|nunca|fals[oa]|fake|without)\b/.test(claimContext) ||
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
      // A precaution applies only after its marker, and ends at an explicit ingredients/allergens section.
      rest = rest.replace(
        /\b(?:puede[n]? contener|puede tener|trazas de|may contain|may contain traces of|traces of|processed in|made in|elaborado en|fabricado en|contaminacion cruzada|cross contamination|peut contenir|traces de|traces eventuelles de|spuren von|puo contenere|tracce di|pode conter|vestigios de|pot contenir)\b.*?(?=\b(?:ingredientes|ingredients|alergenos|allergens|contiene:|contains:)\b|$)/g,
        (scope) => {
          if (scope.match(hazards)) {
            traces = true;
            add(scope.trim(), 'trace');
          } else if (
            /\b(cereales|cereals|alergenos|allergens)\b/.test(scope) ||
            /\b(contener|contain|trazas de|traces of)\s*$/.test(scope)
          ) {
            ambiguous = true;
            add(scope.trim(), 'uncertainty');
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
        // Judge each list item on its own, so a negation or a doubt about one
        // ingredient never hides a cereal named in another.
        let positive = false;
        for (const item of rest.split(/[,()]/)) {
          const doubtful = hedge.test(item);
          for (const hit of item.matchAll(hazards)) {
            // A negated cereal is not positive evidence, and never establishes gluten-free.
            if (doubtful || negationBefore.test(item.slice(0, hit.index))) ambiguous = true;
            else if (onlyOatsWord.test(hit[0])) oats = true;
            else positive = true;
          }
        }
        if (positive) {
          contains = true;
          add(clause.trim(), field === 'allergens' ? 'allergen' : 'ingredient');
        } else {
          add(clause.trim(), 'uncertainty');
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
  return { status, confidence, reasons, evidence, engineVersion: '1.1.0' };
}
