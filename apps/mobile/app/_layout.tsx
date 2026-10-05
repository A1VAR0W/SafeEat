import { useEffect } from 'react';
import { AppState, View } from 'react-native';
import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { PersistQueryClientProvider } from '@tanstack/react-query-persist-client';
import { focusManager, onlineManager } from '@tanstack/react-query';
import NetInfo from '@react-native-community/netinfo';
import { queryClient, persister, shouldPersist } from '../src/query';
import { restoreSession } from '../src/session';
import { loadPreferences } from '../src/preferences';
import { Copy, useReducedMotion, useTheme } from '../src/ui';
import { useNetInfo } from '@react-native-community/netinfo';
import { UpdateModal } from '../src/updates/update-modal';
import { KeyboardProvider } from 'react-native-keyboard-controller';
export default function Layout() {
  const t = useTheme();
  const reduced = useReducedMotion();
  const network = useNetInfo();
  useEffect(() => {
    void loadPreferences();
    const app = AppState.addEventListener('change', (state) =>
      focusManager.setFocused(state === 'active'),
    );
    const off = NetInfo.addEventListener((state) =>
      onlineManager.setOnline(state.isConnected !== false),
    );
    return () => {
      app.remove();
      off();
    };
  }, []);
  return (
    <PersistQueryClientProvider
      client={queryClient}
      onSuccess={() => restoreSession()}
      persistOptions={{
        persister,
        buster: 'v1',
        maxAge: 7 * 86400000,
        dehydrateOptions: { shouldDehydrateQuery: shouldPersist },
      }}
    >
      <KeyboardProvider>
        <StatusBar style={t.dark ? 'light' : 'dark'} />
        {network.isConnected === false && (
          <View
            style={{
              backgroundColor: t.card,
              paddingTop: 44,
              paddingHorizontal: 16,
              paddingBottom: 8,
            }}
          >
            <Copy size={13}>Sin conexión · mostrando información guardada</Copy>
          </View>
        )}
        <Stack
          screenOptions={{
            headerStyle: { backgroundColor: t.bg },
            headerTintColor: t.text,
            headerShadowVisible: false,
            headerBackTitle: 'Atrás',
            animation: reduced ? 'none' : 'default',
            contentStyle: { backgroundColor: t.bg },
          }}
        >
          <Stack.Screen name="(tabs)" options={{ headerShown: false }} />
          <Stack.Screen name="account-edit" options={{ title: 'Editar cuenta' }} />
          <Stack.Screen name="delete-account" options={{ title: 'Eliminar cuenta' }} />
          <Stack.Screen name="profile" options={{ title: 'Tu perfil' }} />
          <Stack.Screen
            name="scan/index"
            options={{ headerShown: false, presentation: 'fullScreenModal' }}
          />
          <Stack.Screen
            name="profile-menu"
            options={{
              presentation: 'formSheet',
              headerShown: false,
              sheetAllowedDetents: 'fitToContents',
              sheetGrabberVisible: true,
              sheetCornerRadius: 28,
            }}
          />
          <Stack.Screen
            name="about"
            options={{
              presentation: 'formSheet',
              headerShown: false,
              sheetAllowedDetents: [0.65, 0.9],
              sheetGrabberVisible: true,
              sheetCornerRadius: 28,
            }}
          />
          <Stack.Screen
            name="product-info"
            options={{
              presentation: 'formSheet',
              headerShown: false,
              sheetAllowedDetents: [0.65, 0.9],
              sheetGrabberVisible: true,
              sheetCornerRadius: 28,
            }}
          />
          <Stack.Screen name="auth/login" options={{ title: 'Iniciar sesión' }} />
          <Stack.Screen name="auth/register" options={{ title: 'Crear cuenta' }} />
          <Stack.Screen name="product/[id]" options={{ title: 'Producto' }} />
          <Stack.Screen name="product/report" options={{ title: 'Revisar producto' }} />
          <Stack.Screen name="restaurant/[id]" options={{ title: 'Restaurante' }} />
          <Stack.Screen name="restaurant/report" options={{ title: 'Revisar restaurante' }} />
          <Stack.Screen name="restaurant/new" options={{ title: 'Añadir restaurante' }} />
          <Stack.Screen name="collection/new" options={{ title: 'Nuevo marcador' }} />
          <Stack.Screen name="scan/ingredients" options={{ title: 'Escanear ingredientes' }} />
          <Stack.Screen name="scan/result" options={{ title: 'Tu etiqueta' }} />
          <Stack.Screen name="settings" options={{ title: 'Ajustes y privacidad' }} />
        </Stack>
        <UpdateModal />
      </KeyboardProvider>
    </PersistQueryClientProvider>
  );
}
