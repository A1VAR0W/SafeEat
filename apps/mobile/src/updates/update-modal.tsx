import { useCallback, useEffect, useRef, useState } from 'react';
import { ActivityIndicator, AppState, Modal, ScrollView, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import Ionicons from '@expo/vector-icons/Ionicons';
import { Button, Copy, useReducedMotion, useTheme } from '../ui';
import type { AndroidUpdate } from './model';
import { nativeUpdater } from './native';
import { checkForUpdate, subscribeToUpdates } from './service';

type Phase = 'available' | 'downloading' | 'permission' | 'ready' | 'installer' | 'error';
export function UpdateModal() {
  const t = useTheme();
  const reduced = useReducedMotion();
  const [update, setUpdate] = useState<AndroidUpdate | null>(null);
  const [phase, setPhase] = useState<Phase>('available');
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState('');
  const busy = useRef(false);
  const waitingForSettings = useRef(false);
  const pendingInstaller = useRef(false);
  const cancelled = useRef(false);
  const downloadReady = useRef(false);

  const install = useCallback(async () => {
    if (!nativeUpdater) return;
    try {
      if (!nativeUpdater.canInstall()) {
        setPhase('permission');
        return;
      }
      setError('');
      setPhase('installer');
      pendingInstaller.current = true;
      await nativeUpdater.install();
    } catch (e) {
      pendingInstaller.current = false;
      setPhase('error');
      setError(e instanceof Error ? e.message : 'No se ha podido abrir el instalador.');
    }
  }, []);
  useEffect(() => {
    if (!nativeUpdater) return;
    const unsubscribe = subscribeToUpdates((next) => {
      if (busy.current) return;
      downloadReady.current = false;
      setPhase('available');
      setError('');
      setUpdate(next);
    });
    const progressListener = nativeUpdater.addListener('downloadProgress', ({ received, total }) =>
      setProgress(total ? Math.min(100, Math.floor((received / total) * 100)) : 0),
    );
    const app = AppState.addEventListener('change', (state) => {
      if (state !== 'active') return;
      if (waitingForSettings.current) {
        waitingForSettings.current = false;
        if (nativeUpdater?.canInstall()) void install();
        else setPhase('permission');
      } else if (pendingInstaller.current) {
        pendingInstaller.current = false;
        setPhase('ready');
      }
    });
    const timer = setTimeout(() => {
      void checkForUpdate();
    }, 1800);
    return () => {
      clearTimeout(timer);
      unsubscribe();
      progressListener.remove();
      app.remove();
      nativeUpdater?.cancelDownload();
    };
  }, [install]);

  async function download() {
    if (!update || !nativeUpdater || busy.current) return;
    if (downloadReady.current) return install();
    busy.current = true;
    cancelled.current = false;
    setError('');
    setProgress(0);
    setPhase('downloading');
    try {
      const a = update.asset;
      await nativeUpdater.download(a.url, a.sha256, a.size, update.version, a.versionCode);
      if (cancelled.current) return;
      downloadReady.current = true;
      await install();
    } catch (e) {
      if (cancelled.current) return;
      setError(
        e instanceof Error ? e.message : 'La descarga se ha interrumpido. Puedes reintentar.',
      );
      setPhase('error');
    } finally {
      busy.current = false;
    }
  }
  function dismiss() {
    if (update?.mandatory) return;
    cancelled.current = true;
    nativeUpdater?.cancelDownload();
    setUpdate(null);
  }
  async function permissions() {
    try {
      waitingForSettings.current = true;
      await nativeUpdater?.openInstallSettings();
    } catch {
      waitingForSettings.current = false;
      setError('Abre los ajustes del móvil y permite instalar aplicaciones desde SafeEat.');
    }
  }
  if (!nativeUpdater) return null;
  return (
    <Modal
      transparent
      visible={!!update}
      animationType={reduced ? 'none' : 'fade'}
      onRequestClose={dismiss}
    >
      <SafeAreaView
        style={{ flex: 1, backgroundColor: '#00000088', justifyContent: 'center', padding: 20 }}
      >
        <View
          accessibilityViewIsModal
          style={{
            maxHeight: '90%',
            backgroundColor: t.card,
            borderRadius: 28,
            padding: 24,
            gap: 16,
          }}
        >
          <Ionicons name="cloud-download-outline" size={36} color={t.accent} />
          <Copy bold size={23}>
            {phase === 'downloading' ? 'Actualizando SafeEat' : 'Nueva actualización'}
          </Copy>
          <Copy bold>SafeEat {update?.version}</Copy>
          {phase === 'downloading' ? (
            <>
              <Copy>Descargando nueva versión…</Copy>
              <View
                accessibilityRole="progressbar"
                accessibilityValue={{ min: 0, max: 100, now: progress }}
                style={{ height: 8, borderRadius: 4, backgroundColor: t.line, overflow: 'hidden' }}
              >
                <View style={{ height: 8, width: `${progress}%`, backgroundColor: t.accent }} />
              </View>
              <Copy bold>{progress} %</Copy>
              {progress === 100 && (
                <Copy muted size={13}>
                  Comprobando la actualización…
                </Copy>
              )}
            </>
          ) : phase === 'permission' ? (
            <Copy>
              Para continuar, permite «Instalar aplicaciones de esta fuente» en la siguiente
              pantalla. Después vuelve a SafeEat; abriremos el instalador.
            </Copy>
          ) : phase === 'ready' || phase === 'installer' ? (
            <Copy>
              La descarga está lista. Confirma la instalación en la pantalla de Android. Si la has
              cancelado, puedes volver a abrirla.
            </Copy>
          ) : (
            <ScrollView style={{ flexShrink: 1, maxHeight: 230 }}>
              <Copy size={15}>
                {update?.notes ||
                  'Hemos mejorado SafeEat. Actualiza para disfrutar de la nueva versión.'}
              </Copy>
            </ScrollView>
          )}
          {!!error && (
            <Copy color={t.danger} size={14}>
              {error}
            </Copy>
          )}
          {phase === 'downloading' ? (
            <ActivityIndicator color={t.accent} />
          ) : phase === 'permission' ? (
            <Button title="Permitir instalación" onPress={() => void permissions()} />
          ) : (
            <Button
              title={
                phase === 'error'
                  ? 'Reintentar'
                  : downloadReady.current
                    ? 'Instalar actualización'
                    : 'Actualizar'
              }
              onPress={() => void download()}
            />
          )}
          {!update?.mandatory && (
            <Button
              secondary
              title={phase === 'downloading' ? 'Cancelar descarga' : 'Ahora no'}
              onPress={dismiss}
            />
          )}
          {update?.mandatory && (
            <Copy muted size={12}>
              Esta actualización es necesaria para seguir usando SafeEat.
            </Copy>
          )}
        </View>
      </SafeAreaView>
    </Modal>
  );
}
