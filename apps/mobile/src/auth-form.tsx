import { useState } from 'react';
import { Pressable, View } from 'react-native';
import Ionicons from '@expo/vector-icons/Ionicons';
import { registrationSchema } from '@safeeat/contracts';
import { router, useLocalSearchParams } from 'expo-router';
import { api, acceptSession, getSession } from './session';
import { queryClient } from './query';
import { Button, Copy, ErrorNotice, Field, Screen, useTheme } from './ui';
import { BirthDateField } from './birth-date-field';
export function AuthForm({ register = false }: { register?: boolean }) {
  const { returnTo } = useLocalSearchParams<{ returnTo?: string }>();
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [firstName, setFirstName] = useState('');
  const [lastName, setLastName] = useState('');
  const [dateOfBirth, setDateOfBirth] = useState('');
  const [consent, setConsent] = useState(false);
  const t = useTheme();
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  async function submit() {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const payload = {
        email: email.trim(),
        password,
        ...(register ? { firstName, lastName, dateOfBirth, productUpdatesConsent: consent } : {}),
      };
      if (register && !registrationSchema.safeParse(payload).success)
        throw new Error(
          'Revisa el nombre, la fecha de nacimiento, el correo y la contraseña de al menos 12 caracteres.',
        );
      const data = await api(`/auth/${register ? 'register' : 'login'}`, {
        method: 'POST',
        body: JSON.stringify(payload),
      });
      const draft =
        !getSession().user && returnTo === 'label'
          ? queryClient.getQueryData(['local-label'])
          : undefined;
      await acceptSession(data);
      if (draft) queryClient.setQueryData(['local-label'], draft);
      router.replace(
        draft
          ? '/scan/result'
          : returnTo?.startsWith('ingredients:')
            ? {
                pathname: '/scan/ingredients',
                params: { barcode: returnTo.slice('ingredients:'.length) },
              }
            : '/(tabs)',
      );
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Screen>
      <Copy muted>
        {register
          ? 'Guarda productos y restaurantes para encontrarlos cuando los necesites.'
          : 'Accede a tus favoritos y etiquetas guardadas.'}
      </Copy>
      {register && (
        <>
          <Field
            label="Nombre"
            value={firstName}
            onChangeText={setFirstName}
            autoComplete="given-name"
            maxLength={80}
          />
          <Field
            label="Apellidos (opcional)"
            value={lastName}
            onChangeText={setLastName}
            autoComplete="family-name"
            maxLength={120}
          />
          <BirthDateField value={dateOfBirth} onChange={setDateOfBirth} />
        </>
      )}
      <Field
        label="Correo electrónico"
        value={email}
        onChangeText={setEmail}
        autoCapitalize="none"
        autoCorrect={false}
        keyboardType="email-address"
        autoComplete="email"
        textContentType="emailAddress"
      />
      <Field
        label="Contraseña"
        value={password}
        onChangeText={setPassword}
        secureTextEntry
        autoCapitalize="none"
        autoComplete={register ? 'new-password' : 'current-password'}
        textContentType={register ? 'newPassword' : 'password'}
        onSubmitEditing={() => void submit()}
      />
      {register && (
        <>
          <Copy muted size={13}>
            Utiliza al menos 12 caracteres.
          </Copy>
          <Pressable
            accessibilityRole="checkbox"
            accessibilityState={{ checked: consent }}
            onPress={() => setConsent(!consent)}
            style={{ flexDirection: 'row', alignItems: 'center', gap: 10, minHeight: 48 }}
          >
            <Ionicons name={consent ? 'checkbox' : 'square-outline'} size={24} color={t.accent} />
            <View style={{ flex: 1 }}>
              <Copy size={14}>Quiero recibir novedades y mejoras de SafeEat (opcional)</Copy>
            </View>
          </Pressable>
        </>
      )}
      {!!error && <ErrorNotice error={error} />}
      <Button
        title={busy ? 'Un momento…' : register ? 'Crear cuenta' : 'Iniciar sesión'}
        onPress={() => void submit()}
        disabled={busy || !email || password.length < (register ? 12 : 1)}
      />
      <Button
        secondary
        title={register ? 'Ya tengo cuenta' : 'Crear una cuenta'}
        onPress={() =>
          router.replace({
            pathname: register ? '/auth/login' : '/auth/register',
            params: { returnTo },
          })
        }
      />
    </Screen>
  );
}
