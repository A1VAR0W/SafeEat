import { useState } from 'react';
import { Alert } from 'react-native';
import { router } from 'expo-router';
import { api, clearSession, logout, useSession } from '../src/session';
import { clearPrivateCache } from '../src/query';
import { Button, Card, Copy, ErrorNotice, Screen, Title } from '../src/ui';
export default function Profile() {
  const { user } = useSession();
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  async function exit(remove = false) {
    setBusy(true);
    setError(null);
    try {
      if (remove) {
        await api('/auth/me', { method: 'DELETE' }, true);
        await clearSession();
      } else await logout();
      await clearPrivateCache();
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Screen>
      <Title>Tu perfil</Title>
      {user ? (
        <>
          <Card>
            <Copy bold>{user.email}</Copy>
            <Copy muted>Tu cuenta sincroniza favoritos y etiquetas guardadas.</Copy>
          </Card>
          <Button secondary title="Cerrar sesión" disabled={busy} onPress={() => void exit()} />
          <Button
            destructive
            title="Eliminar mi cuenta"
            disabled={busy}
            onPress={() =>
              Alert.alert(
                'Eliminar cuenta',
                'Se eliminarán tus favoritos, etiquetas y enlaces de etiquetas personales. Esta acción no se puede deshacer.',
                [
                  { text: 'Cancelar', style: 'cancel' },
                  { text: 'Eliminar cuenta', style: 'destructive', onPress: () => void exit(true) },
                ],
              )
            }
          />
        </>
      ) : (
        <>
          <Copy muted>Guarda tus descubrimientos en un solo lugar.</Copy>
          <Button title="Iniciar sesión" onPress={() => router.push('/auth/login')} />
          <Button secondary title="Crear cuenta" onPress={() => router.push('/auth/register')} />
        </>
      )}
      {!!error && <ErrorNotice error={error} />}
      <Button title="Ajustes y privacidad" secondary onPress={() => router.push('/settings')} />
    </Screen>
  );
}
