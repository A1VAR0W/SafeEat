import { useState } from 'react';
import { router } from 'expo-router';
import { useQuery } from '@tanstack/react-query';
import { ProductSchema, type Product } from '@safeeat/contracts';
import { api, useSession } from '../../src/session';
import { queryClient } from '../../src/query';
import { Assessment, Button, Copy, ErrorNotice, Screen, Title } from '../../src/ui';
export default function Result() {
  const { user } = useSession();
  const data = useQuery<{ product: Product; reviewed: boolean }>({
    queryKey: ['local-label'],
    enabled: false,
  }).data;
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  async function save() {
    if (!data) return;
    if (!user) {
      router.push('/auth/login');
      return;
    }
    setBusy(true);
    try {
      const product = ProductSchema.parse(
        await api(
          '/products/analyze-text',
          {
            method: 'POST',
            body: JSON.stringify({
              name: data.product.name,
              text: data.product.ingredients,
              reviewed: data.reviewed,
            }),
          },
          true,
        ),
      );
      await api(`/favorites/products/${product.id}`, { method: 'POST' }, true);
      queryClient.setQueryData(['product', product.id], product);
      await queryClient.invalidateQueries({ queryKey: ['favorites-products'] });
      router.replace(`/product/${product.id}`);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Screen>
      {data ? (
        <>
          <Title>{data.product.name}</Title>
          <Assessment product={data.product} />
          <Copy muted>
            El análisis se ha realizado en tu dispositivo. Al guardar, se enviará únicamente el
            texto revisado a tu cuenta.
          </Copy>
          <Button
            title={busy ? 'Guardando…' : 'Guardar etiqueta en mi cuenta'}
            disabled={busy}
            onPress={() => void save()}
          />
          {!!error && <ErrorNotice error={error} />}
        </>
      ) : (
        <Copy>Escanea una etiqueta para ver su resultado.</Copy>
      )}
      <Button
        title="Volver a ingredientes"
        secondary
        onPress={() => router.replace('/scan/ingredients')}
      />
    </Screen>
  );
}
