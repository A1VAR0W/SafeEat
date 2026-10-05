import { useState } from 'react';
import { Pressable, View } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { router, useLocalSearchParams } from 'expo-router';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { RestaurantSchema } from '@safeeat/contracts';
import { RestaurantSupportPicker, type RestaurantSupport } from '../../src/restaurant-support';
import { api, useSession } from '../../src/session';
import { Button, Copy, ErrorNotice, Field, Screen, Title, useTheme } from '../../src/ui';

export default function RestaurantReport() {
  const t = useTheme();
  const { id } = useLocalSearchParams<{ id: string }>();
  const { user } = useSession();
  const client = useQueryClient();
  const restaurant = useQuery({
    queryKey: ['restaurant', id],
    queryFn: async () =>
      RestaurantSchema.parse(await api(`/restaurants/${encodeURIComponent(id)}`)),
  });
  const [support, setSupport] = useState<RestaurantSupport | null>(null);
  const [scope, setScope] = useState<'local' | 'chain'>('local');
  const group = restaurant.data?.chain ?? restaurant.data?.reviewGroup;
  const [note, setNote] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  async function save() {
    if (!support || note.trim().length < 10 || busy) return;
    if (!user) return router.push('/auth/login');
    setBusy(true);
    setError(null);
    try {
      const item = RestaurantSchema.parse(
        await api(
          `/restaurants/${encodeURIComponent(id)}/gluten-report`,
          {
            method: 'POST',
            body: JSON.stringify({ support, note: note.trim(), scope }),
          },
          true,
        ),
      );
      client.setQueryData(['restaurant', id], item);
      await client.invalidateQueries({ queryKey: ['restaurants'] });
      await client.invalidateQueries({ queryKey: ['restaurant'] });
      await client.invalidateQueries({ queryKey: ['reviews'] });
      router.back();
    } catch (cause) {
      setError(cause);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Screen>
      <Title>Revisar restaurante</Title>
      <Copy muted>
        Indica lo que te ha confirmado el restaurante. Tu aportación aparecerá como revisión de
        usuario.
      </Copy>
      <RestaurantSupportPicker value={support} onChange={setSupport} />
      {!!group && (
        <View style={{ gap: 10 }}>
          <Copy bold>¿Dónde se aplica tu revisión?</Copy>
          {(['local', 'chain'] as const).map((value) => (
            <Pressable
              key={value}
              accessibilityRole="radio"
              accessibilityState={{ checked: scope === value }}
              onPress={() => setScope(value)}
              style={{
                flexDirection: 'row',
                gap: 12,
                padding: 14,
                minHeight: 52,
                borderRadius: 16,
                borderWidth: 1,
                borderColor: scope === value ? t.accent : t.line,
                alignItems: 'center',
              }}
            >
              <Ionicons
                name={scope === value ? 'radio-button-on' : 'radio-button-off'}
                color={t.accent}
                size={24}
              />
              <View style={{ flex: 1 }}>
                <Copy bold>
                  {value === 'local'
                    ? 'Solo este restaurante'
                    : `Todos los locales de ${group.name}`}
                </Copy>
              </View>
            </Pressable>
          ))}
          {scope === 'chain' && (
            <Copy muted size={13}>
              {restaurant.data?.chain
                ? 'La revisión se compartirá con las sucursales reconocidas de esta cadena.'
                : `Se compartirá con los ${restaurant.data?.reviewGroup?.count} locales que tienen este mismo nombre.`}{' '}
              La preparación puede variar en cada local.
            </Copy>
          )}
        </View>
      )}
      <Field
        label="¿Qué has comprobado?"
        value={note}
        onChangeText={setNote}
        multiline
        maxLength={500}
        placeholder="Por ejemplo, qué te explicó el personal sobre ingredientes y preparación…"
      />
      <Copy muted size={12}>
        Confirma siempre ingredientes y contaminación cruzada antes de comer.
      </Copy>
      {!!error && <ErrorNotice error={error} />}
      <Button
        title={busy ? 'Guardando…' : 'Enviar revisión'}
        disabled={busy || !support || note.trim().length < 10}
        onPress={() => void save()}
      />
    </Screen>
  );
}
