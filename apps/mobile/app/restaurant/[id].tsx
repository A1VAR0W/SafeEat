import { View } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { router, useLocalSearchParams } from 'expo-router';
import { useQuery } from '@tanstack/react-query';
import { RestaurantSchema, restaurantDisclaimer } from '@safeeat/contracts';
import { api } from '../../src/session';
import { ItemActions } from '../../src/actions';
import { SettingRow } from '../../src/compact';
import { Button, Card, Copy, ErrorNotice, Screen, Skeleton, Title, useTheme } from '../../src/ui';
import {
  DirectionsButton,
  openRestaurantLink,
  RestaurantBadge,
} from '../../src/restaurant-components';
import { restaurantStatus } from '../../src/restaurant-presentation';

export default function RestaurantDetail() {
  const t = useTheme();
  const { id } = useLocalSearchParams<{ id: string }>();
  const query = useQuery({
    queryKey: ['restaurant', id],
    queryFn: async ({ signal }) =>
      RestaurantSchema.parse(await api(`/restaurants/${encodeURIComponent(id)}`, { signal })),
    staleTime: 0,
    gcTime: 300000,
  });
  const r = query.data;
  const reports = r?.glutenReports;
  const count = reports ? reports.dedicated + reports.options + reports.noOptions : 0;
  return (
    <Screen>
      {query.isPending && <Skeleton />}
      {query.error && <ErrorNotice error={query.error} retry={() => void query.refetch()} />}
      {r && (
        <>
          <Card style={{ gap: 16, padding: 22 }}>
            <View
              style={{
                flexDirection: 'row',
                alignItems: 'center',
                justifyContent: 'space-between',
                gap: 12,
              }}
            >
              <View
                style={{
                  width: 58,
                  height: 58,
                  borderRadius: 20,
                  backgroundColor: t.bg,
                  alignItems: 'center',
                  justifyContent: 'center',
                }}
              >
                <Ionicons name="restaurant-outline" color={t.accent} size={30} />
              </View>
              <View style={{ flex: 1 }}>
                <RestaurantBadge restaurant={r} />
              </View>
            </View>
            <Title>{r.name}</Title>
            <View style={{ flexDirection: 'row', gap: 8 }}>
              <Ionicons name="location-outline" size={19} color={t.muted} />
              <View style={{ flex: 1 }}>
                <Copy muted size={14}>
                  {r.address || 'Dirección no disponible'}
                </Copy>
              </View>
            </View>
            <DirectionsButton restaurant={r} />
            <Copy muted size={12}>
              La ruta se abrirá en Google Maps.
            </Copy>
            <ItemActions kind="restaurants" item={r} compact />
          </Card>
          <Card style={{ gap: 12 }}>
            <Copy bold size={19}>
              Antes de pedir
            </Copy>
            <Copy size={15}>{restaurantStatus(r.glutenSupport).detail}</Copy>
            {r.evidence.map((evidence, index) => (
              <Copy key={index} muted size={13}>
                {evidence.text}
              </Copy>
            ))}
            <View
              style={{
                flexDirection: 'row',
                gap: 10,
                padding: 12,
                borderRadius: 14,
                backgroundColor: t.bg,
              }}
            >
              <Ionicons name="chatbubbles-outline" color={t.accent} size={22} />
              <View style={{ flex: 1 }}>
                <Copy size={13}>{restaurantDisclaimer}</Copy>
              </View>
            </View>
          </Card>
          {(r.phone || r.website) && (
            <Card style={{ padding: 8 }}>
              {r.phone && (
                <SettingRow
                  icon="call-outline"
                  title="Llamar al restaurante"
                  subtitle={r.phone}
                  onPress={() => void openRestaurantLink(`tel:${r.phone!.replace(/[^\d+]/g, '')}`)}
                />
              )}
              {r.website && (
                <SettingRow
                  icon="globe-outline"
                  title="Visitar su web"
                  subtitle="Consulta la carta y más información"
                  onPress={() => void openRestaurantLink(r.website!)}
                />
              )}
            </Card>
          )}
          <Card>
            <View style={{ flexDirection: 'row', gap: 10, alignItems: 'center' }}>
              <Ionicons name="people-outline" size={23} color={t.accent} />
              <Copy bold size={18}>
                La comunidad
              </Copy>
            </View>
            <Copy muted size={14}>
              {count
                ? `${count} ${count === 1 ? 'persona ha compartido lo que ha comprobado' : 'personas han compartido lo que han comprobado'} en este restaurante.`
                : '¿Has comido aquí? Ayuda a otras personas contando qué opciones te ha confirmado el restaurante.'}
            </Copy>
            <Button
              secondary
              title="Aportar una revisión"
              onPress={() => router.push({ pathname: '/restaurant/report', params: { id: r.id } })}
            />
          </Card>
        </>
      )}
    </Screen>
  );
}
