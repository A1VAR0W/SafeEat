import { useCallback, useRef, useState, useEffect } from 'react';
import {
  AppState,
  View,
  StyleSheet,
  ScrollView,
  Platform,
  StatusBar as NativeStatusBar,
} from 'react-native';
import { StatusBar } from 'expo-status-bar';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { CameraView, useCameraPermissions } from 'expo-camera';
import { router, useFocusEffect } from 'expo-router';
import * as Haptics from 'expo-haptics';
import { ProductSchema, type Product } from '@safeeat/contracts';
import { api, RequestError } from '../../src/session';
import { queryClient } from '../../src/query';
import { Button, Copy, ErrorNotice, Field, useTheme } from '../../src/ui';
import { IconButton } from '../../src/compact';
export default function Scan() {
  const t = useTheme();
  const insets = useSafeAreaInsets();
  const [permission, requestPermission] = useCameraPermissions();
  const [active, setActive] = useState(false);
  const [foreground, setForeground] = useState(AppState.currentState === 'active');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [manual, setManual] = useState('');
  const [manualOpen, setManualOpen] = useState(false);
  const [torch, setTorch] = useState(false);
  const locked = useRef(false);
  const alive = useRef(false);
  const request = useRef<AbortController | null>(null);
  useFocusEffect(
    useCallback(() => {
      alive.current = true;
      setActive(true);
      locked.current = false;
      setBusy(false);
      return () => {
        alive.current = false;
        setActive(false);
        setTorch(false);
        locked.current = true;
        request.current?.abort();
      };
    }, []),
  );
  useEffect(() => {
    const sub = AppState.addEventListener('change', (s) => {
      setForeground(s === 'active');
      if (s !== 'active') setTorch(false);
    });
    return () => sub.remove();
  }, []);
  async function scan(code: string) {
    if (locked.current || !/^(?:\d{8}|\d{12,14})$/.test(code)) return;
    locked.current = true;
    setBusy(true);
    setError(null);
    setTorch(false);
    void Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(() => {});
    const controller = new AbortController();
    request.current = controller;
    try {
      const cached = queryClient.getQueryData<Product>(['barcode', code]);
      const item =
        cached ??
        ProductSchema.parse(await api(`/products/barcode/${code}`, { signal: controller.signal }));
      if (!alive.current) return;
      queryClient.setQueryData(['barcode', code], item);
      queryClient.setQueryData(['product', item.id], item);
      router.replace(`/product/${item.id}`);
    } catch (e) {
      if (alive.current && e instanceof RequestError && e.code === 'PRODUCT_NOT_FOUND') {
        router.replace({ pathname: '/scan/ingredients', params: { barcode: code } });
      } else if (alive.current) setError(e);
    } finally {
      if (alive.current) setBusy(false);
    }
  }
  return (
    <View style={{ flex: 1, backgroundColor: t.brandDeep }}>
      <StatusBar style="light" />
      {permission?.granted && active && foreground && !busy && !error && !manualOpen && (
        <CameraView
          style={StyleSheet.absoluteFill}
          facing="back"
          enableTorch={torch}
          barcodeScannerSettings={{ barcodeTypes: ['ean13', 'ean8', 'upc_a', 'upc_e'] }}
          onBarcodeScanned={({ data }) => void scan(data)}
          accessible
          accessibilityLabel="Cámara para escanear códigos de barras"
        />
      )}
      <View
        style={{
          flex: 1,
          paddingTop:
            Math.max(
              insets.top,
              Platform.OS === 'ios' ? 59 : (NativeStatusBar.currentHeight ?? 24),
            ) + 12,
          paddingBottom: Math.max(insets.bottom, 12) + 12,
          paddingHorizontal: 16 + Math.max(insets.left, insets.right),
        }}
      >
        <View style={{ flexDirection: 'row', justifyContent: 'space-between' }}>
          <IconButton label="Cerrar escáner" icon="close" onPress={() => router.back()} />
          <IconButton
            label={torch ? 'Apagar linterna' : 'Encender linterna'}
            icon={torch ? 'flash' : 'flash-off-outline'}
            selected={torch}
            disabled={!permission?.granted || busy || manualOpen}
            onPress={() => setTorch(!torch)}
          />
        </View>
        <View
          pointerEvents="none"
          style={{ flex: 1, justifyContent: 'center', alignItems: 'center', minHeight: 120 }}
        >
          <View
            style={{
              width: '82%',
              maxWidth: 320,
              height: 140,
              borderWidth: 2,
              borderColor: busy ? t.brandLime : '#FFFFFF',
              borderRadius: 24,
            }}
          />
        </View>
        <ScrollView
          style={{ maxHeight: '56%', flexGrow: 0, borderRadius: 20, backgroundColor: t.card }}
          contentContainerStyle={{ padding: 16, gap: 10 }}
          keyboardShouldPersistTaps="handled"
        >
          <Copy bold>{busy ? 'Código detectado · consultando…' : 'Escanea el código'}</Copy>
          {!permission?.granted && (
            <Button title="Permitir acceso a la cámara" onPress={() => void requestPermission()} />
          )}
          {!!error && (
            <ErrorNotice
              error={error}
              retry={() => {
                locked.current = false;
                setError(null);
              }}
            />
          )}
          {manualOpen && (
            <>
              <Field
                label="Código de barras"
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
            </>
          )}
          <Button
            secondary
            title={manualOpen ? 'Volver a la cámara' : 'Introducir código'}
            onPress={() => {
              setManualOpen(!manualOpen);
              setTorch(false);
            }}
          />
          <Button
            secondary
            title="Escanear ingredientes"
            onPress={() => router.replace('/scan/ingredients')}
          />
        </ScrollView>
      </View>
    </View>
  );
}
