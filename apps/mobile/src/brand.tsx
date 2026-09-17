import { View } from 'react-native';
import { Image } from 'expo-image';
import mark from '../assets/safeeat-mark.png';
import wordmark from '../assets/safeeat-wordmark.png';

export function BrandMark({ size = 32 }: { size?: number }) {
  return (
    <Image
      source={mark}
      accessibilityLabel="Logotipo de SafeEat"
      style={{ width: size, height: size }}
      contentFit="contain"
    />
  );
}

export function BrandHeader() {
  return (
    <View
      accessibilityRole="header"
      accessibilityLabel="SafeEat"
      style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}
    >
      <BrandMark size={30} />
      <Image source={wordmark} style={{ width: 105, height: 25 }} contentFit="contain" />
    </View>
  );
}
