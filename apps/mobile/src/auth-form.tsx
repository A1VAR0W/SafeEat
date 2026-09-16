import { useState } from 'react';
import { router } from 'expo-router';
import { api, acceptSession } from './session';
import { clearPrivateCache } from './query';
import { Button, Copy, ErrorNotice, Field, Screen, Title } from './ui';
export function AuthForm({ register = false }: { register?: boolean }) {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  async function submit() {
    setBusy(true);
    setError(null);
    try {
      const data = await api(`/auth/${register ? 'register' : 'login'}`, {
        method: 'POST',
        body: JSON.stringify({ email: email.trim(), password }),
      });
      await clearPrivateCache();
      await acceptSession(data);
      router.replace('/(tabs)/home');
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Screen>
      <Title>{register ? 'Tu espacio,\nsin dudas.' : 'Hola de nuevo.'}</Title>
      <Copy muted>
        {register
          ? 'Guarda productos y restaurantes para encontrarlos cuando los necesites.'
          : 'Accede a tus favoritos y etiquetas guardadas.'}
      </Copy>
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
        <Copy muted size={13}>
          Utiliza al menos 12 caracteres.
        </Copy>
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
        onPress={() => router.replace(register ? '/auth/login' : '/auth/register')}
      />
    </Screen>
  );
}
