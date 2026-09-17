import { View } from 'react-native';
import { router } from 'expo-router';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { SettingRow, SheetHeader } from '../src/compact';
export default function ProfileMenu() {
  const insets = useSafeAreaInsets();
  return (
    <View style={{ padding: 16, paddingBottom: insets.bottom + 16 }}>
      <SheetHeader>Tu espacio</SheetHeader>
      <SettingRow
        icon="person-outline"
        title="Ver perfil"
        onPress={() => router.replace('/profile')}
      />
      <SettingRow
        icon="settings-outline"
        title="Ajustes y privacidad"
        onPress={() => router.replace('/settings')}
      />
      <SettingRow
        icon="information-circle-outline"
        title="Sobre SafeEat"
        onPress={() => router.replace('/about')}
      />
    </View>
  );
}
