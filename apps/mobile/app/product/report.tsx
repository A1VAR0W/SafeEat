import { useState } from 'react';
import { Pressable } from 'react-native';
import { router, useLocalSearchParams } from 'expo-router';
import { useQueryClient } from '@tanstack/react-query';
import { ProductSchema } from '@safeeat/contracts';
import { api, useSession } from '../../src/session';
import { Button, Copy, ErrorNotice, Field, Screen, Title, useTheme } from '../../src/ui';

type Choice = 'GLUTEN_FREE' | 'CONTAINS_GLUTEN';
export default function ProductReport() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { user } = useSession();
  const client = useQueryClient();
  const t = useTheme();
  const [status, setStatus] = useState<Choice | null>(null);
  const [note, setNote] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  async function save() {
    if (!status || note.trim().length < 10 || busy) return;
    if (!user) return router.push('/auth/login');
    setBusy(true);
    setError(null);
    try {
      const product = ProductSchema.parse(
        await api(
          `/products/${encodeURIComponent(id)}/gluten-report`,
          { method: 'POST', body: JSON.stringify({ status, note: note.trim() }) },
          true,
        ),
      );
      client.setQueryData(['product', id], product);
      await client.invalidateQueries({ queryKey: ['product', id] });
      await client.invalidateQueries({ queryKey: ['favorites-products'] });
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
      <Title>Revisar el producto</Title>
      <Copy muted>
        Cuenta qué indica la etiqueta actual. Otras personas verán tu aportación identificada como
        revisión de usuario.
      </Copy>
      {(
        [
          ['GLUTEN_FREE', 'La etiqueta indica sin gluten'],
          ['CONTAINS_GLUTEN', 'La etiqueta indica que contiene gluten'],
        ] as const
      ).map(([value, label]) => (
        <Pressable
          key={value}
          accessibilityRole="radio"
          accessibilityState={{ selected: status === value }}
          onPress={() => setStatus(value)}
          style={{
            padding: 16,
            minHeight: 56,
            borderRadius: 16,
            backgroundColor: t.card,
            borderWidth: status === value ? 2 : 1,
            borderColor: status === value ? t.accent : t.line,
          }}
        >
          <Copy bold>{label}</Copy>
        </Pressable>
      ))}
      <Field
        label="¿Qué has comprobado?"
        value={note}
        onChangeText={setNote}
        multiline
        maxLength={500}
        placeholder="Por ejemplo, la declaración de la etiqueta y su fecha…"
      />
      <Copy muted size={12}>
        Una revisión de usuario no sustituye a la etiqueta ni garantiza ausencia de contaminación
        cruzada.
      </Copy>
      {!!error && <ErrorNotice error={error} />}
      <Button
        title={busy ? 'Guardando…' : 'Enviar revisión'}
        disabled={busy || !status || note.trim().length < 10}
        onPress={() => void save()}
      />
    </Screen>
  );
}
