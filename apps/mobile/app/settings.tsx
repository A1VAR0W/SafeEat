import { useState } from 'react';
import { Alert, Linking, Platform } from 'react-native';
import { checkForUpdate } from '../src/updates/service';
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
      setMessage('Marcadores, grupos y etiquetas personales eliminados.');
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
                'Se eliminarán tus favoritos, grupos y etiquetas personales. Los enlaces públicos de esas etiquetas dejarán de funcionar. Las fotos que hayas aportado a productos seguirán visibles hasta que elimines tu cuenta.',
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
          Las etiquetas personales se leen en el dispositivo. Si aportas una foto a un producto con
          código de barras, se guarda en el servidor para otros usuarios. No usamos publicidad ni
          analítica.
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
      <Copy muted size={13}>
        SafeEat {Constants.expoConfig?.version}
      </Copy>
      {Platform.OS === 'android' && (
        <Button
          secondary
          title="Buscar actualizaciones"
          onPress={() => {
            setMessage('Buscando actualizaciones…');
            void checkForUpdate(true).then((result) =>
              setMessage(
                result === 'current'
                  ? 'Ya tienes la última versión.'
                  : result === 'available'
                    ? ''
                    : 'No hemos podido comprobarlo. Puedes volver a intentarlo más tarde.',
              ),
            );
          }}
        />
      )}
    </Screen>
  );
}
