import { useState } from 'react';
import { router, Stack, useLocalSearchParams } from 'expo-router';
import { emailChangeSchema, passwordChangeSchema, profileFieldsSchema } from '@safeeat/contracts';
import { api, acceptSession, refresh, useSession } from '../src/session';
import { queryClient } from '../src/query';
import { Button, Copy, ErrorNotice, Field, Screen } from '../src/ui';
export default function AccountEdit() {
  const { mode } = useLocalSearchParams<{ mode: 'email' | 'password' | 'complete' }>();
  const { user } = useSession();
  const [email, setEmail] = useState(user?.email ?? ''),
    [currentPassword, setCurrentPassword] = useState('');
  const [newPassword, setNewPassword] = useState(''),
    [confirmPassword, setConfirmPassword] = useState('');
  const [firstName, setFirstName] = useState(''),
    [lastName, setLastName] = useState(''),
    [dateOfBirth, setDateOfBirth] = useState('');
  const [error, setError] = useState<unknown>(null),
    [busy, setBusy] = useState(false);
  async function save() {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const parsed =
        mode === 'complete'
          ? profileFieldsSchema.safeParse({ firstName, lastName, dateOfBirth })
          : mode === 'email'
            ? emailChangeSchema.safeParse({ email: email.trim(), currentPassword })
            : passwordChangeSchema.safeParse({ currentPassword, newPassword });
      if (!parsed.success)
        throw new Error(
          'Revisa los datos. La fecha usa AAAA-MM-DD y la nueva contraseña debe tener al menos 12 caracteres.',
        );
      if (mode === 'password' && newPassword !== confirmPassword)
        throw new Error('Las nuevas contraseñas no coinciden.');
      if (!['email', 'password', 'complete'].includes(mode))
        throw new Error('Operación no válida.');
      const result = await api(
        `/auth/${mode === 'complete' ? 'profile/complete' : mode}`,
        { method: 'POST', body: JSON.stringify(parsed.data) },
        true,
      );
      if (mode === 'complete') await refresh();
      else await acceptSession(result);
      await queryClient.invalidateQueries({ queryKey: ['profile'] });
      router.back();
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Screen>
      <Stack.Screen
        options={{
          title:
            mode === 'email'
              ? 'Cambiar correo'
              : mode === 'complete'
                ? 'Completar perfil'
                : 'Cambiar contraseña',
        }}
      />
      {mode === 'complete' ? (
        <>
          <Copy muted size={13}>
            Completa tus datos una vez. Después se mostrarán como solo lectura.
          </Copy>
          <Field label="Nombre" value={firstName} onChangeText={setFirstName} maxLength={80} />
          <Field
            label="Apellidos (opcional)"
            value={lastName}
            onChangeText={setLastName}
            maxLength={120}
          />
          <Field
            label="Fecha de nacimiento"
            placeholder="AAAA-MM-DD"
            value={dateOfBirth}
            onChangeText={setDateOfBirth}
            maxLength={10}
          />
        </>
      ) : (
        <>
          <Copy muted size={13}>
            Confirma tu contraseña actual. Se cerrarán las otras sesiones de tu cuenta.
          </Copy>
          {mode === 'email' && (
            <Field
              label="Nuevo correo electrónico"
              value={email}
              onChangeText={setEmail}
              keyboardType="email-address"
              autoCapitalize="none"
              autoCorrect={false}
            />
          )}
          <Field
            label="Contraseña actual"
            value={currentPassword}
            onChangeText={setCurrentPassword}
            secureTextEntry
            autoCapitalize="none"
            autoComplete="current-password"
          />
          {mode === 'password' && (
            <>
              <Field
                label="Nueva contraseña"
                value={newPassword}
                onChangeText={setNewPassword}
                secureTextEntry
                autoCapitalize="none"
                autoComplete="new-password"
              />
              <Field
                label="Confirmar nueva contraseña"
                value={confirmPassword}
                onChangeText={setConfirmPassword}
                secureTextEntry
                autoCapitalize="none"
                autoComplete="new-password"
              />
            </>
          )}
        </>
      )}
      {!!error && <ErrorNotice error={error} />}
      <Button
        title={busy ? 'Guardando…' : 'Guardar cambios'}
        disabled={busy}
        onPress={() => void save()}
      />
    </Screen>
  );
}
