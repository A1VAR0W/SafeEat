import { Linking } from 'react-native';
import Constants from 'expo-constants';
import { Copy, Screen, Button } from '../src/ui';
import { SheetHeader } from '../src/compact';
export default function About() {
  const base = String(Constants.expoConfig?.extra?.publicUrl ?? '');
  return (
    <Screen>
      <SheetHeader>Sobre SafeEat</SheetHeader>
      <Copy>Información clara para decidir con más tranquilidad.</Copy>
      <Copy muted>
        SafeEat reúne ingredientes, posibles alérgenos, trazas y sus fuentes. Te ayuda a revisar la
        evidencia disponible; no sustituye la etiqueta oficial ni el consejo de un profesional
        sanitario.
      </Copy>
      <Copy size={12} muted>
        Versión {Constants.expoConfig?.version}
        {Constants.expoConfig?.extra?.environment !== 'production'
          ? ` · ${Constants.expoConfig?.extra?.environment}`
          : ''}
      </Copy>
      <Button
        title="Privacidad"
        secondary
        onPress={() => void Linking.openURL(`${base}/privacy`)}
      />
      <Button
        title="Cómo analizamos la información"
        secondary
        onPress={() => void Linking.openURL(`${base}/sources`)}
      />
    </Screen>
  );
}
