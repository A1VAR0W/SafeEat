import { useState } from 'react';
import { router, useLocalSearchParams } from 'expo-router';
import { useQueryClient } from '@tanstack/react-query';
import { RestaurantSchema } from '@safeeat/contracts';
import { RestaurantSupportPicker, type RestaurantSupport } from '../../src/restaurant-support';
import { api, useSession } from '../../src/session';
import { Button, Copy, ErrorNotice, Field, Screen, Title } from '../../src/ui';

export default function RestaurantReport() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { user } = useSession();
  const client = useQueryClient();
  const [support, setSupport] = useState<RestaurantSupport | null>(null);
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
            body: JSON.stringify({ support, note: note.trim() }),
          },
          true,
        ),
      );
      client.setQueryData(['restaurant', id], item);
      await client.invalidateQueries({ queryKey: ['restaurants'] });
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
