import { Platform } from 'react-native';
import { requireOptionalNativeModule } from 'expo';
type NativeUpdater = {
  installedVersion(): { version: string; versionCode: number };
  canInstall(): boolean;
  download(
    url: string,
    hash: string,
    size: number,
    version: string,
    versionCode: number,
  ): Promise<void>;
  cancelDownload(): void;
  openInstallSettings(): Promise<void>;
  install(): Promise<void>;
  addListener(
    event: 'downloadProgress',
    listener: (event: { received: number; total: number }) => void,
  ): { remove(): void };
};
export const nativeUpdater =
  Platform.OS === 'android' ? requireOptionalNativeModule<NativeUpdater>('SafeEatUpdater') : null;
