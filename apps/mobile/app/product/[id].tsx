import { useEffect } from 'react';
import { Linking } from 'react-native';
import { useLocalSearchParams, router } from 'expo-router';
import { useQuery } from '@tanstack/react-query';
import { Image } from 'expo-image';
import { ProductSchema, ShareSchema, type Product } from '@safeeat/contracts';
import { api } from '../../src/session';
import { queryClient } from '../../src/query';
import { ItemActions } from '../../src/actions';
import { Assessment, Button, Card, Copy, ErrorNotice, Screen, Skeleton, Title } from '../../src/ui';
import { es } from '../../src/i18n';
export default function ProductDetail() {
  const { id, shareToken } = useLocalSearchParams<{ id: string; shareToken?: string }>();
  const query = useQuery({
    queryKey: shareToken ? ['shared-product', id, shareToken] : ['product', id],
    gcTime: shareToken ? 0 : 7 * 86400000,
    queryFn: async () => {
      if (!shareToken) return ProductSchema.parse(await api(`/products/${encodeURIComponent(id)}`));
      const share = ShareSchema.parse(
        await api(`/public/shares/${encodeURIComponent(shareToken)}`),
      );
      if (share.type !== 'product' || share.item.id !== id) throw new Error('Enlace no válido.');
      return share.item;
    },
  });
  const p = query.data;
  useEffect(() => {
    if (!p || shareToken) return;
    queryClient.setQueryData<Product[]>(['recent'], (old) =>
      [p, ...(old ?? []).filter((x) => x.id !== p.id)].slice(0, 20),
    );
    if (p.barcode) queryClient.setQueryData(['barcode', p.barcode], p);
  }, [p, shareToken]);
  return (
    <Screen>
      {query.error && <ErrorNotice error={query.error} retry={() => void query.refetch()} />}
      {!p && query.isPending && <Skeleton />}
      {p && (
        <>
          {p.imageUrl && (
            <Image
              source={p.imageUrl}
              contentFit="contain"
              style={{ height: 190, borderRadius: 24 }}
              cachePolicy="disk"
            />
          )}
          <Title>{p.name}</Title>
          {p.brand && <Copy muted>{p.brand}</Copy>}
          {(p.stale ||
            query.isError ||
            Date.now() - new Date(p.fetchedAt).getTime() > 86400000) && (
            <Card>
              <Copy>{es.cached}</Copy>
            </Card>
          )}
          <Assessment product={p} />
          <Button
            secondary
            title="Escanear ingredientes"
            onPress={() => router.push('/scan/ingredients')}
          />
          <Card>
            <Copy bold size={21}>
              Calidad nutricional
            </Copy>
            <Copy size={30} bold>
              {p.quality.score === null ? 'Sin datos suficientes' : `${p.quality.score}/100`}
            </Copy>
            <Copy muted size={13}>
              {p.quality.explanation}
            </Copy>
            {p.quality.breakdown.map((b) => (
              <Copy key={b.label}>
                {b.label}: {b.points > 0 ? '+' : ''}
                {b.points}
              </Copy>
            ))}
            {p.nutriScore && <Copy bold>Nutri-Score del proveedor: {p.nutriScore}</Copy>}
            <Copy muted size={12}>
              Valores por 100 g/ml
            </Copy>
            {Object.entries(p.nutrition).map(([key, value]) => (
              <Copy key={key} size={14}>
                {
                  (
                    {
                      energyKcal: 'Energía (kcal)',
                      sugars: 'Azúcar (g)',
                      saturatedFat: 'Grasas saturadas (g)',
                      salt: 'Sal (g)',
                      fibre: 'Fibra (g)',
                      protein: 'Proteína (g)',
                    } as Record<string, string>
                  )[key]
                }
                : {value}
              </Copy>
            ))}
          </Card>
          {p.ingredients && (
            <Card>
              <Copy bold>Ingredientes</Copy>
              <Copy>{p.ingredients}</Copy>
            </Card>
          )}
          {!(shareToken && p.userCreated) && <ItemActions kind="products" item={p} />}
          <Copy muted size={12}>
            {p.source} · Consultado el {new Date(p.fetchedAt).toLocaleDateString('es')}
          </Copy>
          {p.sourceUrl && (
            <Button
              secondary
              title="Consultar fuente"
              onPress={() => void Linking.openURL(p.sourceUrl!)}
            />
          )}
        </>
      )}
    </Screen>
  );
}
