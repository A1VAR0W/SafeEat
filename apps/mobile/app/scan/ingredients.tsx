import { useRef, useState } from 'react';
import { View, Switch } from 'react-native';
import { CameraView, useCameraPermissions } from 'expo-camera';
import { extractTextFromImage, isSupported } from 'expo-text-extractor';
import { File } from 'expo-file-system';
import { router } from 'expo-router';
import { assessGluten } from '@safeeat/gluten-engine';
import { scoreNutrition } from '@safeeat/nutrition-engine';
import type { Product } from '@safeeat/contracts';
import { queryClient } from '../../src/query';
import { Button, Copy, ErrorNotice, Field, Screen, Title } from '../../src/ui';
export default function Ingredients() {
  const [permission, requestPermission] = useCameraPermissions();
  const camera = useRef<CameraView>(null);
  const [capture, setCapture] = useState(false);
  const [busy, setBusy] = useState(false);
  const [text, setText] = useState('');
  const [name, setName] = useState('Etiqueta escaneada');
  const [reviewed, setReviewed] = useState(false);
  const [error, setError] = useState<unknown>(null);
  async function photograph() {
    setBusy(true);
    setError(null);
    let uri: string | undefined;
    try {
      const photo = await camera.current?.takePictureAsync({ quality: 0.8 });
      if (!photo) throw new Error('No se ha podido capturar la imagen.');
      uri = photo.uri;
      setCapture(false);
      const lines = await extractTextFromImage(uri);
      setText(lines.join('\n'));
      setReviewed(false);
      if (!lines.length)
        throw new Error('No se ha reconocido texto. Prueba con más luz o escríbelo manualmente.');
    } catch (e) {
      setError(e);
    } finally {
      if (uri) {
        const file = new File(uri);
        if (file.exists) file.delete();
      }
      setBusy(false);
    }
  }
  function analyze() {
    const p: Product = {
      id: `local-${Date.now()}`,
      name,
      ingredients: text,
      assessment: assessGluten({
        ingredients: text,
        source: 'Etiqueta local',
        origin: 'ocr',
        reviewed,
      }),
      nutrition: {},
      quality: scoreNutrition({}),
      source: 'Etiqueta local',
      fetchedAt: new Date().toISOString(),
      stale: false,
      userCreated: true,
    };
    queryClient.setQueryData(['local-label'], { product: p, reviewed });
    router.push('/scan/result');
  }
  return (
    <Screen>
      <Title>Lee la etiqueta</Title>
      <Copy muted>
        Fotografía todos los ingredientes y las advertencias de alérgenos. La foto se procesa en
        este dispositivo y se elimina después.
      </Copy>
      {capture && permission?.granted && (
        <View style={{ height: 300, borderRadius: 20, overflow: 'hidden' }}>
          <CameraView
            ref={camera}
            style={{ flex: 1 }}
            facing="back"
            accessibilityLabel="Fotografiar la etiqueta de ingredientes"
          />
        </View>
      )}
      {isSupported && (
        <Button
          title={
            busy ? 'Leyendo texto…' : capture ? 'Capturar etiqueta' : 'Fotografiar ingredientes'
          }
          disabled={busy}
          onPress={() => {
            if (capture) {
              void photograph();
              return;
            }
            if (permission?.granted) setCapture(true);
            else void requestPermission().then((p) => setCapture(p.granted));
          }}
        />
      )}
      {!!error && <ErrorNotice error={error} />}
      <Field label="Nombre del producto" value={name} onChangeText={setName} maxLength={160} />
      <Field
        label="Revisa y corrige el texto"
        value={text}
        onChangeText={(v) => {
          setText(v);
          setReviewed(false);
        }}
        multiline
        maxLength={12000}
        placeholder="Ingredientes, alérgenos y posibles trazas…"
      />
      <View style={{ flexDirection: 'row', gap: 12, alignItems: 'center' }}>
        <Switch
          accessibilityLabel="He revisado toda la etiqueta"
          value={reviewed}
          onValueChange={setReviewed}
        />
        <View style={{ flex: 1 }}>
          <Copy size={14}>
            He revisado el texto completo comparándolo con la etiqueta original.
          </Copy>
        </View>
      </View>
      <Button
        title="Analizar etiqueta"
        onPress={analyze}
        disabled={text.trim().length < 3 || !name.trim() || busy}
      />
    </Screen>
  );
}
