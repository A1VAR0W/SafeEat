import { useState } from 'react';
import { Pressable, View } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { router } from 'expo-router';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { RestaurantSchema, type Restaurant } from '@safeeat/contracts';
import { z } from 'zod';
import { api, useSession } from './session';
import { Button, Card, Copy, ErrorNotice, useTheme } from './ui';

export function RestaurantRatingSummary({ restaurant }: { restaurant: Restaurant }) {
  const t = useTheme();
  return (
    <View style={{ gap: 4 }}>
      {!!restaurant.chain && (
        <Copy muted size={13}>
          Cadena {restaurant.chain.name}
        </Copy>
      )}
      {!!restaurant.rating && (
        <View style={{ flexDirection: 'row', gap: 6, alignItems: 'center' }}>
          <Ionicons name="star" size={17} color={t.accent} />
          <View style={{ flex: 1 }}>
            <Copy size={13}>
              {restaurant.rating.average.toLocaleString('es-ES', {
                minimumFractionDigits: 1,
                maximumFractionDigits: 1,
              })}{' '}
              · {restaurant.rating.count}{' '}
              {restaurant.rating.count === 1 ? 'valoración' : 'valoraciones'}
            </Copy>
          </View>
        </View>
      )}
    </View>
  );
}

export function RestaurantRatingEditor({ restaurant }: { restaurant: Restaurant }) {
  const t = useTheme();
  const { user } = useSession();
  const client = useQueryClient();
  const [chosen, setChosen] = useState<number | null>(null);
  const path = `/restaurants/${encodeURIComponent(restaurant.id)}/rating`;
  const mine = useQuery({
    queryKey: ['restaurant-rating', restaurant.id, user?.id],
    enabled: !!user,
    queryFn: async ({ signal }) =>
      z
        .object({ stars: z.number().int().min(1).max(5).nullable() })
        .parse(await api(path, { signal }, true)),
    staleTime: 60000,
  });
  const save = useMutation({
    mutationFn: async (stars: number | null) =>
      RestaurantSchema.parse(
        await api(
          path,
          stars === null
            ? { method: 'DELETE' }
            : { method: 'PUT', body: JSON.stringify({ stars }) },
          true,
        ),
      ),
    onSuccess: async (item, stars) => {
      client.setQueryData(['restaurant', item.id], item);
      client.setQueryData(['restaurant-rating', item.id, user?.id], { stars });
      setChosen(null);
      await client.invalidateQueries({ queryKey: ['restaurants'] });
    },
  });
  const stars = chosen ?? mine.data?.stars ?? 0;
  return (
    <Card style={{ gap: 12 }}>
      <Copy bold size={18}>
        Valoraciones
      </Copy>
      <RestaurantRatingSummary restaurant={restaurant} />
      {!restaurant.rating && <Copy muted>Sé la primera persona en valorar este local.</Copy>}
      <Copy muted size={13}>
        Puntúa tu experiencia en este restaurante. Las estrellas no verifican sus opciones sin
        gluten.
      </Copy>
      {user ? (
        <>
          <Copy size={14}>{mine.data?.stars ? 'Tu valoración' : '¿Qué te ha parecido?'}</Copy>
          <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 4 }}>
            {[1, 2, 3, 4, 5].map((value) => (
              <Pressable
                key={value}
                disabled={save.isPending || mine.isPending || !!mine.error}
                accessibilityRole="button"
                accessibilityLabel={`${value} ${value === 1 ? 'estrella' : 'estrellas'}`}
                accessibilityState={{
                  selected: stars === value,
                  disabled: save.isPending || mine.isPending || !!mine.error,
                }}
                onPress={() => setChosen(value)}
                style={{
                  minWidth: 44,
                  minHeight: 48,
                  alignItems: 'center',
                  justifyContent: 'center',
                }}
              >
                <Ionicons
                  name={value <= stars ? 'star' : 'star-outline'}
                  size={32}
                  color={t.accent}
                />
              </Pressable>
            ))}
          </View>
          {mine.error && <ErrorNotice error={mine.error} retry={() => void mine.refetch()} />}
          {save.error && <ErrorNotice error={save.error} />}
          <Button
            title={save.isPending ? 'Guardando…' : 'Guardar valoración'}
            disabled={!chosen || save.isPending}
            onPress={() => save.mutate(stars)}
          />
          {!!mine.data?.stars && (
            <Button
              secondary
              title="Eliminar mi valoración"
              disabled={save.isPending}
              onPress={() => save.mutate(null)}
            />
          )}
        </>
      ) : (
        <Button secondary title="Entrar para valorar" onPress={() => router.push('/auth/login')} />
      )}
    </Card>
  );
}
