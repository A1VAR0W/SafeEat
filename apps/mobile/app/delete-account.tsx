import { useState } from 'react';
import { router } from 'expo-router';
import { api, clearSession } from '../src/session';
import { deleteAccountSchema } from '@safeeat/contracts';
import { Button, Copy, ErrorNotice, Field, Screen } from '../src/ui';
export default function DeleteAccount() {
  const [password, setPassword] = useState(''),
    [confirmation, setConfirmation] = useState('');
  const [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>(null);
  const ready = deleteAccountSchema.safeParse({ currentPassword: password, confirmation }).success;
  async function remove() {
    if (!ready || busy) return;
    setBusy(true);
    setError(null);
    try {
      await api(
        '/auth/me',
        { method: 'DELETE', body: JSON.stringify({ currentPassword: password, confirmation }) },
        true,
      );
      await clearSession();
      router.replace('/auth/login');
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Screen>
      <Copy bold>Esta acción no se puede deshacer.</Copy>
      <Copy>
        Se eliminarán tu cuenta, favoritos y etiquetas privadas. Tus sesiones y enlaces públicos de
        etiquetas dejarán de funcionar.
      </Copy>
      <Field
        label="Contraseña actual"
        value={password}
        onChangeText={setPassword}
        secureTextEntry
        autoCapitalize="none"
        autoComplete="current-password"
      />
      <Field
        label="Escribe ELIMINAR para confirmar"
        value={confirmation}
        onChangeText={setConfirmation}
        autoCapitalize="characters"
        autoCorrect={false}
      />
      {!!error && <ErrorNotice error={error} />}
      <Button
        title={busy ? 'Eliminando…' : 'Eliminar mi cuenta definitivamente'}
        destructive
        disabled={!ready || busy}
        onPress={() => void remove()}
      />
      <Button title="Conservar mi cuenta" secondary disabled={busy} onPress={() => router.back()} />
    </Screen>
  );
}
