import { useCallback, useEffect, useRef, useState } from 'react';
import { AppState, View, Switch } from 'react-native';
import { CameraView, useCameraPermissions } from 'expo-camera';
import { extractTextFromImage, isSupported } from 'expo-text-extractor';
import { File } from 'expo-file-system';
import { router, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { assessGluten } from '@safeeat/gluten-engine';
import { scoreNutrition } from '@safeeat/nutrition-engine';
import { ProductSchema, type Product } from '@safeeat/contracts';
import { api, useSession } from '../../src/session';
import { queryClient } from '../../src/query';
import { Button, Copy, ErrorNotice, Field, Screen, Title } from '../../src/ui';
export default function Ingredients() {
  const { barcode } = useLocalSearchParams<{ barcode?: string }>();
  const { user } = useSession();
  const known = barcode ? queryClient.getQueryData<Product>(['barcode', barcode]) : undefined;
  const [permission, requestPermission] = useCameraPermissions();
  const camera = useRef<CameraView>(null);
  const [capture, setCapture] = useState(false);
  const [active, setActive] = useState(false);
  const [foreground, setForeground] = useState(AppState.currentState === 'active');
  useFocusEffect(
    useCallback(() => {
      setActive(true);
      return () => setActive(false);
    }, []),
  );
  useEffect(() => {
    const sub = AppState.addEventListener('change', (s) => setForeground(s === 'active'));
    return () => sub.remove();
  }, []);
  const [busy, setBusy] = useState(false);
  const [text, setText] = useState('');
  const [name, setName] = useState(known?.name ?? (barcode ? '' : 'Etiqueta escaneada'));
  const [reviewed, setReviewed] = useState(false);
  const [photoUri, setPhotoUri] = useState<string | null>(null);
  const photoRef = useRef<string | null>(null);
  useEffect(
    () => () => {
      if (photoRef.current) {
        const file = new File(photoRef.current);
        if (file.exists) file.delete();
      }
    },
    [],
  );
  const [error, setError] = useState<unknown>(null);
  async function photograph() {
    setBusy(true);
    setError(null);
    let uri: string | undefined;
    try {
      const photo = await camera.current?.takePictureAsync({ quality: 0.5, exif: false });
      if (!photo) throw new Error('No se ha podido capturar la imagen.');
      uri = photo.uri;
      const file = new File(uri);
      if (file.size > 1500000)
        throw new Error('La foto ocupa demasiado. Acércate a la etiqueta y vuelve a intentarlo.');
      setCapture(false);
      const lines = await extractTextFromImage(uri);
      setText(lines.join('\n'));
      setReviewed(false);
      if (photoUri) {
        const previous = new File(photoUri);
        if (previous.exists) previous.delete();
      }
      setPhotoUri(uri);
      photoRef.current = uri;
      uri = undefined;
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
  async function contribute() {
    if (!barcode || !photoUri || !user || !reviewed || busy) return;
    setBusy(true);
    setError(null);
    try {
      const file = new File(photoUri);
      if (!file.exists || file.size > 1500000)
        throw new Error('Haz una nueva foto de la etiqueta.');
      const item = ProductSchema.parse(
        await api(
          `/products/${barcode}/ingredients-photo`,
          {
            method: 'POST',
            body: JSON.stringify({
              name: name.trim(),
              ingredients: text.trim(),
              imageBase64: await file.base64(),
            }),
          },
          true,
        ),
      );
      queryClient.setQueryData(['barcode', barcode], item);
      queryClient.setQueryData(['product', item.id], item);
      if (file.exists) file.delete();
      photoRef.current = null;
      router.replace(`/product/${item.id}`);
    } catch (e) {
      setError(e);
    } finally {
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
      informationSource: 'community',
      communityContribution: false,
    };
    queryClient.setQueryData(['local-label'], { product: p, reviewed });
    router.push('/scan/result');
  }
  return (
    <Screen>
      <Title>{barcode ? 'Aporta esta etiqueta' : 'Lee la etiqueta'}</Title>
      <Copy muted>
        {barcode
          ? `Código ${barcode}. Fotografía todos los ingredientes y alérgenos. Al guardar, la foto y el texto corregido serán visibles para otros usuarios.`
          : 'Fotografía todos los ingredientes y las advertencias de alérgenos. La foto se procesa en este dispositivo y se elimina después.'}
      </Copy>
      {barcode && !user && (
        <Button
          title="Iniciar sesión para aportar"
          onPress={() =>
            router.push({ pathname: '/auth/login', params: { returnTo: `ingredients:${barcode}` } })
          }
        />
      )}
      {capture && active && foreground && permission?.granted && (
        <View style={{ height: 300, borderRadius: 20, overflow: 'hidden' }}>
          <CameraView
            ref={camera}
            style={{ flex: 1 }}
            facing="back"
            accessibilityLabel="Fotografiar la etiqueta de ingredientes"
          />
        </View>
      )}
      {isSupported && (!barcode || !!user) && (
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
      <Field
        label="Nombre del producto"
        value={name}
        onChangeText={setName}
        maxLength={160}
        placeholder="Nombre tal como aparece en el envase"
      />
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
      {barcode ? (
        <Button
          title={busy ? 'Guardando etiqueta…' : 'Guardar foto e ingredientes'}
          onPress={() => void contribute()}
          disabled={
            !user || !photoUri || !reviewed || text.trim().length < 3 || !name.trim() || busy
          }
        />
      ) : (
        <Button
          title="Analizar etiqueta"
          onPress={analyze}
          disabled={text.trim().length < 3 || !name.trim() || busy}
        />
      )}
    </Screen>
  );
}
