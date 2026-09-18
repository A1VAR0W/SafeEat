import { Linking } from 'react-native';
import { router, useLocalSearchParams } from 'expo-router';
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
                ? '⚠ No verificado'
                : r.glutenSupport === 'DEDICATED_GLUTEN_FREE'
                  ? '✓ Se indica comida sin gluten'
                  : r.glutenSupport === 'GLUTEN_FREE_OPTIONS'
                    ? '✓ Se indican opciones sin gluten'
                    : 'Sin opciones sin gluten indicadas'}
            </Copy>
            {!!r.glutenReports &&
              r.glutenReports.dedicated + r.glutenReports.options + r.glutenReports.noOptions >
                0 && (
                <Copy muted size={13}>
                  Revisiones de usuarios:{' '}
                  {r.glutenReports.dedicated + r.glutenReports.options + r.glutenReports.noOptions}
                </Copy>
              )}
            {r.evidence.map((e, i) => (
              <Copy key={i}>{e.text}</Copy>
            ))}
            <Copy>{restaurantDisclaimer}</Copy>
          </Card>
          <Button
            secondary
            title="Aportar revisión sobre opciones sin gluten"
            onPress={() => router.push({ pathname: '/restaurant/report', params: { id: r.id } })}
          />
          <Button title="Cómo llegar" onPress={() => void Linking.openURL(r.mapsUrl)} />
          {r.website && (
            <Button
              secondary
              title="Web del restaurante"
              onPress={() => void Linking.openURL(r.website!)}
            />
          )}
          <ItemActions kind="restaurants" item={r} />
        </>
      )}
    </Screen>
  );
}
