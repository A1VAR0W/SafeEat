import { useCallback, useRef, useState, useEffect } from 'react';
import { AppState, View } from 'react-native';
import { CameraView, useCameraPermissions } from 'expo-camera';
import { router, useFocusEffect } from 'expo-router';
import { ProductSchema, type Product } from '@safeeat/contracts';
import { api } from '../../src/session';
import { queryClient } from '../../src/query';
import { Button, Copy, ErrorNotice, Field, Screen, Title } from '../../src/ui';
export default function Scan() {
  const [permission, requestPermission] = useCameraPermissions();
  const [active, setActive] = useState(false);
  const [foreground, setForeground] = useState(AppState.currentState === 'active');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [manual, setManual] = useState('');
  const [torch, setTorch] = useState(false);
  const locked = useRef(false);
  useFocusEffect(
    useCallback(() => {
      setActive(true);
      locked.current = false;
      setBusy(false);
      return () => {
        setActive(false);
        locked.current = true;
      };
    }, []),
  );
  useEffect(() => {
    const sub = AppState.addEventListener('change', (s) => setForeground(s === 'active'));
    return () => sub.remove();
  }, []);
  async function scan(code: string) {
    if (locked.current || !/^(?:\d{8}|\d{12,14})$/.test(code)) return;
    locked.current = true;
    setBusy(true);
    setError(null);
    try {
      const cached = queryClient.getQueryData<Product>(['barcode', code]);
      const item =
        cached ??
        (await queryClient.fetchQuery({
          queryKey: ['barcode', code],
          queryFn: async () => ProductSchema.parse(await api(`/products/barcode/${code}`)),
        }));
      if (!item) throw new Error('Producto no disponible.');
      queryClient.setQueryData(['product', item.id], item);
      router.push(`/product/${item.id}`);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Screen>
      <Title>Escanear</Title>
      <Copy muted>
        Apunta al código de barras. La cámara no puede determinar el gluten por el aspecto del
        envase.
      </Copy>
      {permission?.granted ? (
        <View
          style={{ height: 280, borderRadius: 24, overflow: 'hidden', backgroundColor: '#15231A' }}
        >
          {active && foreground && !busy && (
            <CameraView
              style={{ flex: 1 }}
              facing="back"
              enableTorch={torch}
              barcodeScannerSettings={{ barcodeTypes: ['ean13', 'ean8', 'upc_a', 'upc_e'] }}
              onBarcodeScanned={({ data }) => void scan(data)}
              accessible
              accessibilityLabel="Cámara para escanear códigos de barras"
            />
          )}
        </View>
      ) : (
        <Button title="Permitir acceso a la cámara" onPress={() => void requestPermission()} />
      )}
      <Button
        secondary
        title={torch ? 'Apagar linterna' : 'Encender linterna'}
        onPress={() => setTorch(!torch)}
      />
      {busy && <Copy>Consultando producto…</Copy>}
      {!!error && (
        <ErrorNotice
          error={error}
          retry={() => {
            locked.current = false;
            setError(null);
          }}
        />
      )}
      <Field
        label="O escribe el código de barras"
        value={manual}
        onChangeText={setManual}
        keyboardType="number-pad"
        maxLength={14}
      />
      <Button
        title="Consultar código"
        disabled={busy || !/^(?:\d{8}|\d{12,14})$/.test(manual)}
        onPress={() => {
          locked.current = false;
          void scan(manual);
        }}
      />
      <Button
        secondary
        title="Escanear ingredientes"
        onPress={() => router.push('/scan/ingredients')}
      />
    </Screen>
  );
}
