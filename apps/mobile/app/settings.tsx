import { useState } from 'react';
import { Appearance } from 'react-native';
import { Button, Card, Copy, ErrorNotice, Screen, Title } from '../src/ui';
import { clearAllCache } from '../src/query';
import { api } from '../src/session';
import Constants from 'expo-constants';
export default function Settings() {
  const [error, setError] = useState<unknown>(null);
  const [cleared, setCleared] = useState(false);
  const [connection, setConnection] = useState('');
  return (
    <Screen>
      <Title>A tu manera</Title>
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
                  ? `Conexión correcta · ${meta.environment} · ${meta.version}`
                  : `El servidor responde como ${meta.environment}; esta app es ${expected}. Instala la versión correspondiente.`,
              );
            })
            .catch((e: unknown) =>
              setConnection(e instanceof Error ? e.message : 'No se puede contactar con SafeEat.'),
            );
        }}
      />
      {!!connection && <Copy>{connection}</Copy>}
      <Copy bold>Apariencia</Copy>
      <Button secondary title="Sistema" onPress={() => Appearance.setColorScheme('unspecified')} />
      <Button secondary title="Modo claro" onPress={() => Appearance.setColorScheme('light')} />
      <Button secondary title="Modo oscuro" onPress={() => Appearance.setColorScheme('dark')} />
      <Card>
        <Copy bold>Tu privacidad</Copy>
        <Copy>
          Las fotos de etiquetas se procesan en el dispositivo y se eliminan tras extraer el texto.
          Solo se envía el texto si decides guardarlo. No utilizamos analítica ni publicidad.
        </Copy>
        <Copy>
          Los productos proceden de Open Food Facts (ODbL; imágenes CC BY-SA). La búsqueda de
          restaurantes usa Google Maps y requiere conexión. Las búsquedas y la ubicación opcional se
          envían al proveedor.
        </Copy>
        <Copy>
          SafeEat no certifica alimentos ni restaurantes. Comprueba siempre la etiqueta y los
          protocolos de contaminación cruzada.
        </Copy>
      </Card>
      <Button
        secondary
        title="Borrar información guardada en este dispositivo"
        onPress={() => {
          void clearAllCache()
            .then(() => setCleared(true))
            .catch(setError);
        }}
      />
      {cleared && <Copy>Caché eliminada.</Copy>}
      {!!error && <ErrorNotice error={error} />}
      <Copy muted size={12}>
        SafeEat 0.1.0 · Sin gluten y calidad nutricional son conceptos independientes.
      </Copy>
    </Screen>
  );
}
