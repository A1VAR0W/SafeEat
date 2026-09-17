import { useEffect } from 'react';
import { AppState, View } from 'react-native';
import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { PersistQueryClientProvider } from '@tanstack/react-query-persist-client';
import { focusManager, onlineManager } from '@tanstack/react-query';
import NetInfo from '@react-native-community/netinfo';
import { queryClient, persister, shouldPersist } from '../src/query';
import { restoreSession } from '../src/session';
import { ProfileButton } from '../src/compact';
import { Copy, useReducedMotion, useTheme } from '../src/ui';
import { useNetInfo } from '@react-native-community/netinfo';
export default function Layout() {
  const t = useTheme();
  const reduced = useReducedMotion();
  const network = useNetInfo();
  useEffect(() => {
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
          headerRight: () => <ProfileButton />,
          animation: reduced ? 'none' : 'default',
          contentStyle: { backgroundColor: t.bg },
        }}
      >
        <Stack.Screen name="(tabs)" options={{ headerShown: false }} />
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
        <Stack.Screen name="restaurant/[id]" options={{ title: 'Restaurante' }} />
        <Stack.Screen name="scan/ingredients" options={{ title: 'Escanear ingredientes' }} />
        <Stack.Screen name="scan/result" options={{ title: 'Tu etiqueta' }} />
        <Stack.Screen name="settings" options={{ title: 'Ajustes y privacidad' }} />
      </Stack>
    </PersistQueryClientProvider>
  );
}
