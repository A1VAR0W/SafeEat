import { useState } from 'react';
import { router } from 'expo-router';
import { useQueryClient } from '@tanstack/react-query';
import { api, useSession } from '../../src/session';
import { Button, Copy, ErrorNotice, Field, Screen, Title } from '../../src/ui';

export default function NewCollection() {
  const { user } = useSession();
  const client = useQueryClient();
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  async function create() {
    if (!name.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      await api(
        '/collections',
        { method: 'POST', body: JSON.stringify({ name: name.trim() }) },
        true,
      );
      await client.invalidateQueries({ queryKey: ['collections', user?.id] });
      router.back();
    } catch (cause) {
      setError(cause);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Screen>
      <Title>Nuevo marcador</Title>
      <Copy muted>Organiza los productos como quieras.</Copy>
      <Field
        label="Nombre"
        value={name}
        onChangeText={setName}
        maxLength={60}
        placeholder="Desayunos, viajes, recetas…"
        autoFocus
        onSubmitEditing={() => void create()}
      />
      {!!error && <ErrorNotice error={error} />}
      <Button
        title={busy ? 'Creando…' : 'Crear marcador'}
        disabled={busy || !name.trim()}
        onPress={() => void create()}
      />
    </Screen>
  );
}
