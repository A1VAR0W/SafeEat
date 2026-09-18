import { Linking } from 'react-native';
import { Image } from 'expo-image';
import { useLocalSearchParams } from 'expo-router';
import { useProduct } from '../src/product';
import { SheetHeader } from '../src/compact';
import { Assessment, Button, Card, Copy, ErrorNotice, Screen, Skeleton } from '../src/ui';
const titles: Record<string, string> = {
  why: '¿Por qué este resultado?',
  ingredients: 'Ingredientes',
  nutrition: 'Nutrición',
  sources: 'Fuentes',
};
const nutrients: Record<string, string> = {
  energyKcal: 'Energía (kcal)',
  sugars: 'Azúcar (g)',
  saturatedFat: 'Grasas saturadas (g)',
  salt: 'Sal (g)',
  fibre: 'Fibra (g)',
  protein: 'Proteína (g)',
};
export default function ProductInfo() {
  const {
    id,
    shareToken,
    section = 'why',
  } = useLocalSearchParams<{ id: string; shareToken?: string; section?: string }>();
  const query = useProduct(id, shareToken),
    p = query.data;
  return (
    <Screen>
      <SheetHeader>{titles[section] ?? 'Información'}</SheetHeader>
      {query.error && <ErrorNotice error={query.error} />}
      {!p && query.isPending && <Skeleton />}
      {p && (
        <>
          {section === 'why' && <Assessment product={p} />}
          {section === 'ingredients' && (
            <>
              <Card>
                <Copy bold>Lista de ingredientes</Copy>
                <Copy>
                  {p.ingredients
                    ? p.ingredients
                        .replace(/\\_/g, ' ')
                        .replace(/_/g, ' ')
                        .replace(/\s+/g, ' ')
                        .trim()
                    : 'No hay una lista de ingredientes disponible.'}
                </Copy>
                {p.informationSource === 'community' && (
                  <Copy muted size={12}>
                    Texto aportado por un usuario
                  </Copy>
                )}
              </Card>
              {p.ingredientsImageUrl && (
                <Card>
                  <Copy bold>Foto de la etiqueta</Copy>
                  <Image
                    source={p.ingredientsImageUrl}
                    contentFit="contain"
                    cachePolicy="memory-disk"
                    style={{ width: '100%', height: 280, borderRadius: 12 }}
                    accessibilityLabel="Foto de los ingredientes aportada por un usuario"
                  />
                  <Copy muted size={12}>
                    Foto aportada por la comunidad. Comprueba que coincida con el envase actual.
                  </Copy>
                </Card>
              )}
              {p.assessment.evidence
                .filter((e) => ['ingredient', 'allergen', 'trace'].includes(e.kind))
                .map((e, i) => (
                  <Card key={i}>
                    <Copy bold>
                      {e.kind === 'trace' ? 'Trazas' : 'Ingrediente o alérgeno relevante'}
                    </Copy>
                    <Copy>{e.text}</Copy>
                  </Card>
                ))}
              <Copy muted size={12}>
                Revisa la etiqueta actual; una lista incompleta no demuestra ausencia de gluten.
              </Copy>
            </>
          )}
          {section === 'nutrition' && (
            <>
              <Copy bold size={24}>
                {p.quality.score === null ? 'Sin datos suficientes' : `${p.quality.score}/100`}
              </Copy>
              <Copy>{p.quality.explanation}</Copy>
              {p.quality.breakdown.map((b) => (
                <Copy key={b.label}>
                  {b.label}: {b.points > 0 ? '+' : ''}
                  {b.points}
                </Copy>
              ))}
              {p.nutriScore && <Copy bold>Nutri-Score: {p.nutriScore}</Copy>}
              <Copy muted size={12}>
                Valores por 100 g/ml
              </Copy>
              {Object.entries(p.nutrition).map(([key, value]) => (
                <Copy key={key}>
                  {nutrients[key] ?? key}: {value}
                </Copy>
              ))}
            </>
          )}
          {section === 'sources' && (
            <>
              <Copy>Datos de la ficha del producto y aportaciones de usuarios, si las hay.</Copy>
              <Copy muted>Consultado el {new Date(p.fetchedAt).toLocaleString('es')}</Copy>
              {p.sourceUrl && (
                <Button
                  title="Ver ficha original"
                  secondary
                  onPress={() => void Linking.openURL(p.sourceUrl!)}
                />
              )}
              <Copy muted size={12}>
                La confianza describe la calidad de la evidencia, no una probabilidad de seguridad.
              </Copy>
            </>
          )}
        </>
      )}
    </Screen>
  );
}
