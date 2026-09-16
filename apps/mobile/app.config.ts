import type { ExpoConfig } from 'expo/config';
import { version } from '../../package.json';
const production = process.env.APP_ENV === 'production' || process.env.APP_ENV === 'preproduction';
const apiUrl = process.env.API_PUBLIC_URL ?? 'http://localhost:4000';
const publicUrl = process.env.PUBLIC_APP_URL ?? 'http://localhost:3000';
if (
  production &&
  (!process.env.API_PUBLIC_URL ||
    !process.env.PUBLIC_APP_URL ||
    ![apiUrl, publicUrl].every((v) => new URL(v).protocol === 'https:'))
)
  throw new Error('Native release requires HTTPS API_PUBLIC_URL and PUBLIC_APP_URL');
const host = new URL(publicUrl).hostname;
const config: ExpoConfig = {
  name: 'SafeEat',
  slug: 'safeeat',
  version,
  scheme: 'safeeat',
  orientation: 'portrait',
  userInterfaceStyle: 'automatic',
  jsEngine: 'hermes',
  ios: {
    bundleIdentifier: 'com.a1var0w.safeeat',
    buildNumber: process.env.BUILD_NUMBER ?? '1',
    supportsTablet: true,
    associatedDomains: production ? [`applinks:${host}`] : [],
    infoPlist: { ITSAppUsesNonExemptEncryption: false },
  },
  android: {
    package: 'com.a1var0w.safeeat',
    versionCode: Number(process.env.BUILD_NUMBER ?? 1),
    allowBackup: false,
    blockedPermissions: [
      'android.permission.RECORD_AUDIO',
      'android.permission.SYSTEM_ALERT_WINDOW',
    ],
    intentFilters: production
      ? [
          {
            action: 'VIEW',
            autoVerify: true,
            data: [
              { scheme: 'https', host, pathPrefix: '/p/' },
              { scheme: 'https', host, pathPrefix: '/r/' },
            ],
            category: ['BROWSABLE', 'DEFAULT'],
          },
        ]
      : [],
  },
  plugins: [
    'expo-router',
    [
      'expo-camera',
      {
        cameraPermission: 'SafeEat necesita la cámara para leer códigos de barras y etiquetas.',
        recordAudioAndroid: false,
      },
    ],
    ['expo-secure-store', { configureAndroidBackup: true }],
    [
      'expo-location',
      {
        locationWhenInUsePermission:
          'Puedes compartir tu ubicación para buscar restaurantes cercanos.',
      },
    ],
  ],
  extra: { apiUrl, publicUrl, environment: process.env.APP_ENV ?? 'development' },
};
export default config;
