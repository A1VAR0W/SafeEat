import { Alert, Linking, Pressable, View } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import type { Restaurant } from '@safeeat/contracts';
import { Copy, useTheme } from './ui';
import { directionsUrl, restaurantStatus } from './restaurant-presentation';

export function RestaurantBadge({ restaurant }: { restaurant: Restaurant }) {
  const t = useTheme();
  const status = restaurantStatus(restaurant.glutenSupport);
  const color =
    restaurant.glutenSupport === 'UNKNOWN'
      ? t.dark
        ? '#F3CC82'
        : status.color
      : restaurant.glutenSupport === 'NO_GLUTEN_FREE_OPTIONS'
        ? t.muted
        : t.accent;
  return (
    <View style={{ flexDirection: 'row', alignItems: 'center', gap: 6 }}>
      <Ionicons name={status.icon} color={color} size={18} />
      <View style={{ flex: 1 }}>
        <Copy size={13} bold color={color}>
          {status.label}
        </Copy>
      </View>
    </View>
  );
}

export async function openRestaurantLink(url: string) {
  try {
    await Linking.openURL(url);
  } catch {
    Alert.alert(
      'No se ha podido abrir',
      'Inténtalo de nuevo o comprueba que tienes un navegador instalado.',
    );
  }
}

export function DirectionsButton({ restaurant }: { restaurant: Restaurant }) {
  const t = useTheme();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel="Cómo llegar"
      accessibilityHint="Abre la ruta en Google Maps"
      onPress={() => void openRestaurantLink(directionsUrl(restaurant))}
      style={({ pressed }) => ({
        flexDirection: 'row',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 8,
        minHeight: 48,
        padding: 12,
        borderRadius: 16,
        backgroundColor: t.accent,
        opacity: pressed ? 0.75 : 1,
      })}
    >
      <Ionicons name="navigate" color={t.onAccent} size={19} />
      <Copy bold color={t.onAccent}>
        Cómo llegar
      </Copy>
    </Pressable>
  );
}
