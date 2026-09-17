import { useState } from 'react';
import { Alert, Linking } from 'react-native';
import Constants from 'expo-constants';
import { Button, Card, Copy, ErrorNotice, Screen } from '../src/ui';
import { SegmentedControl, SettingRow } from '../src/compact';
import { clearHistory, clearPrivateCache } from '../src/query';
import { api, useSession } from '../src/session';
import { setPreferences, usePreferences } from '../src/preferences';
export default function Settings() {
  const preferences = usePreferences(),
    { user } = useSession();
  const [error, setError] = useState<unknown>(null),
    [message, setMessage] = useState(''),
    [busy, setBusy] = useState(false),
    [connection, setConnection] = useState('');
  const publicUrl = String(Constants.expoConfig?.extra?.publicUrl ?? '');
  async function clearSaved() {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await api(
        '/saved',
        { method: 'DELETE', body: JSON.stringify({ confirmation: 'BORRAR' }) },
        true,
      );
      await clearPrivateCache();
      setMessage('Marcadores y etiquetas personales eliminados.');
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Screen>
      <Copy bold>Apariencia</Copy>
      <SegmentedControl
        value={preferences.theme}
        options={[
          { value: 'system', label: 'Sistema' },
          { value: 'light', label: 'Claro' },
          { value: 'dark', label: 'Oscuro' },
        ]}
        onChange={(theme) => void setPreferences({ theme }).catch(setError)}
      />
      <Copy bold>Tamaño del texto</Copy>
      <SegmentedControl
        value={preferences.textSize}
        options={[
          { value: 'small', label: 'Pequeño' },
          { value: 'standard', label: 'Estándar' },
          { value: 'large', label: 'Grande' },
        ]}
        onChange={(textSize) => void setPreferences({ textSize }).catch(setError)}
      />
      <Card>
        <Copy bold>Tu actividad</Copy>
        <SettingRow
          icon="time-outline"
          title="Borrar historial de escaneos"
          subtitle="Escaneos, búsquedas y productos recientes"
          onPress={() =>
            Alert.alert(
              '¿Borrar el historial de escaneos?',
              'Se borrará la actividad reciente de este dispositivo. Tus favoritos, etiquetas y enlaces se conservarán.',
              [
                { text: 'Cancelar', style: 'cancel' },
                {
                  text: 'Borrar historial',
                  style: 'destructive',
                  onPress: () =>
                    void clearHistory()
                      .then(() => setMessage('Historial eliminado.'))
                      .catch(setError),
                },
              ],
            )
          }
        />
        {user && (
          <Button
            title={busy ? 'Borrando…' : 'Borrar marcadores'}
            secondary
            disabled={busy}
            onPress={() =>
              Alert.alert(
                '¿Borrar todos los marcadores?',
                'Se eliminarán tus favoritos de productos y restaurantes y todas tus etiquetas personales. Los enlaces públicos de esas etiquetas dejarán de funcionar. Se conservará tu cuenta.',
                [
                  { text: 'Cancelar', style: 'cancel' },
                  {
                    text: 'Borrar marcadores',
                    style: 'destructive',
                    onPress: () => void clearSaved(),
                  },
                ],
              )
            }
          />
        )}
      </Card>
      {!!message && <Copy>{message}</Copy>}
      {!!error && <ErrorNotice error={error} />}
      <Card>
        <Copy bold>Privacidad</Copy>
        <Copy size={14}>
          Las fotos de etiquetas se procesan en el dispositivo y se eliminan tras leerlas. Solo
          enviamos el texto que decides guardar. No usamos publicidad ni analítica.
        </Copy>
        <SettingRow
          icon="shield-checkmark-outline"
          title="Privacidad y fuentes"
          onPress={() => void Linking.openURL(`${publicUrl}/privacy`)}
        />
      </Card>
      <Button
        secondary
        title="Comprobar conexión con SafeEat"
        onPress={() => {
          setConnection('Comprobando…');
          void api<{ environment: string; version: string }>('/meta')
            .then((meta) => {
              const expected = Constants.expoConfig?.extra?.environment;
              setConnection(
                meta.environment === expected
                  ? `Conexión correcta · ${meta.version}${expected === 'production' ? '' : ` · ${meta.environment}`}`
                  : 'Esta versión no corresponde con el servidor. Instala la versión actualizada.',
              );
            })
            .catch((e: unknown) =>
              setConnection(e instanceof Error ? e.message : 'No se puede contactar con SafeEat.'),
            );
        }}
      />
      {!!connection && <Copy size={13}>{connection}</Copy>}
    </Screen>
  );
}
