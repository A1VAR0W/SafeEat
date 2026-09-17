import { useState } from 'react';
import { Alert, View } from 'react-native';
import Ionicons from '@expo/vector-icons/Ionicons';
import { router } from 'expo-router';
import { useQuery } from '@tanstack/react-query';
import { UserSchema } from '@safeeat/contracts';
import { api, logout, useSession } from '../src/session';
import { Button, Card, Copy, ErrorNotice, Screen, useTheme } from '../src/ui';
import { SettingRow } from '../src/compact';
export default function Profile() {
  const { user } = useSession(),
    t = useTheme();
  const [error, setError] = useState<unknown>(null),
    [busy, setBusy] = useState(false);
  const profile = useQuery({
    queryKey: ['profile', user?.id],
    enabled: !!user,
    queryFn: async () => UserSchema.parse(await api('/auth/me', {}, true)),
  });
  const p = profile.data ?? user;
  async function exit() {
    setBusy(true);
    setError(null);
    try {
      await logout();
      router.replace('/auth/login');
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Screen>
      {p ? (
        <>
          <Card>
            {[
              ['Nombre', p.firstName],
              ['Apellidos', p.lastName],
              ['Fecha de nacimiento', p.dateOfBirth?.split('-').reverse().join('/')],
            ].map(([label, value]) => (
              <View
                key={label}
                style={{ flexDirection: 'row', gap: 10, alignItems: 'center', minHeight: 48 }}
              >
                <View style={{ flex: 1 }}>
                  <Copy muted size={12}>
                    {label} · solo lectura
                  </Copy>
                  <Copy>{value || 'Sin indicar'}</Copy>
                </View>
                <Ionicons name="lock-closed-outline" color={t.muted} size={16} />
              </View>
            ))}
            {!p.firstName && !p.dateOfBirth && (
              <Button
                title="Completar mis datos"
                secondary
                onPress={() => router.push('/account-edit?mode=complete')}
              />
            )}
          </Card>
          <SettingRow
            icon="mail-outline"
            title="Correo electrónico"
            subtitle={p.email}
            onPress={() => router.push('/account-edit?mode=email')}
          />
          <SettingRow
            icon="key-outline"
            title="Cambiar contraseña"
            onPress={() => router.push('/account-edit?mode=password')}
          />
          <Button title="Cerrar sesión" secondary disabled={busy} onPress={() => void exit()} />
          <View
            style={{
              marginTop: 36,
              paddingTop: 20,
              borderTopWidth: 1,
              borderTopColor: t.line,
              gap: 10,
            }}
          >
            <Copy bold>Zona de peligro</Copy>
            <Copy muted size={13}>
              Eliminar tu cuenta es permanente y revoca los enlaces de tus etiquetas personales.
            </Copy>
            <Button
              title="Eliminar cuenta"
              destructive
              onPress={() =>
                Alert.alert(
                  'Eliminar cuenta',
                  'Se borrarán tu cuenta, favoritos y etiquetas privadas. Se revocarán todas las sesiones y los enlaces de tus etiquetas. Tendrás que confirmar tu contraseña antes de eliminarla.',
                  [
                    { text: 'Cancelar', style: 'cancel' },
                    {
                      text: 'Continuar',
                      style: 'destructive',
                      onPress: () => router.push('/delete-account'),
                    },
                  ],
                )
              }
            />
          </View>
        </>
      ) : (
        <>
          <Copy muted>Guarda tus descubrimientos en un solo lugar.</Copy>
          <Button title="Iniciar sesión" onPress={() => router.push('/auth/login')} />
          <Button title="Crear cuenta" secondary onPress={() => router.push('/auth/register')} />
        </>
      )}
      {!!(error || profile.error) && <ErrorNotice error={error || profile.error} />}
    </Screen>
  );
}
