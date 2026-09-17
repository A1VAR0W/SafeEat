import { Linking } from 'react-native';
import { useLocalSearchParams } from 'expo-router';
import { useQuery } from '@tanstack/react-query';
import { RestaurantSchema, restaurantDisclaimer } from '@safeeat/contracts';
import { api } from '../../src/session';
import { ItemActions } from '../../src/actions';
import { Button, Card, Copy, ErrorNotice, Screen, Skeleton, Title } from '../../src/ui';
export default function RestaurantDetail() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const query = useQuery({
    queryKey: ['restaurant', id],
    queryFn: async () =>
      RestaurantSchema.parse(await api(`/restaurants/${encodeURIComponent(id)}`)),
    gcTime: 0,
  });
  const r = query.data;
  return (
    <Screen>
      {query.isPending && <Skeleton />}
      {query.error && <ErrorNotice error={query.error} retry={() => void query.refetch()} />}
      {r && (
        <>
          <Title>{r.name}</Title>
          <Copy muted>{r.address}</Copy>
          <Card>
            <Copy bold size={21}>
              {r.glutenSupport === 'UNKNOWN'
                ? '? Sin información verificada'
                : r.glutenSupport === 'DEDICATED_GLUTEN_FREE'
                  ? '✓ Dedicado a comida sin gluten'
                  : '! Opciones sin gluten'}
            </Copy>
            {r.evidence.map((e, i) => (
              <Copy key={i}>
                {e.text} · {e.source}
              </Copy>
            ))}
            <Copy>{restaurantDisclaimer}</Copy>
          </Card>
          <Button title="Cómo llegar" onPress={() => void Linking.openURL(r.mapsUrl)} />
          {r.website && (
            <Button
              secondary
              title="Web del restaurante"
              onPress={() => void Linking.openURL(r.website!)}
            />
          )}
          <ItemActions kind="restaurants" item={r} />
          <Copy muted>Google Maps · {r.attributions.join(' · ')}</Copy>
        </>
      )}
    </Screen>
  );
}
