import { Tabs } from 'expo-router';
import Ionicons from '@expo/vector-icons/Ionicons';
import { ProfileButton } from '../../src/compact';
import { BrandHeader } from '../../src/brand';
import { useTheme } from '../../src/ui';
export default function TabLayout() {
  const t = useTheme();
  return (
    <Tabs
      screenOptions={{
        headerShown: true,
        headerTitle: () => <BrandHeader />,
        headerRight: () => <ProfileButton />,
        headerRightContainerStyle: { paddingRight: 16 },
        headerStyle: { backgroundColor: t.bg },
        headerTintColor: t.text,
        headerShadowVisible: false,
        animation: 'none',
        tabBarActiveTintColor: t.accent,
        tabBarInactiveTintColor: t.muted,
        tabBarHideOnKeyboard: true,
        tabBarStyle: { backgroundColor: t.card, borderTopColor: t.line },
        tabBarLabelStyle: { fontSize: 11, fontWeight: '600' },
      }}
    >
      {(
        [
          { name: 'index', title: 'Explorar', icon: 'compass-outline' },
          { name: 'restaurants', title: 'Restaurantes', icon: 'restaurant-outline' },
          { name: 'favorites', title: 'Guardados', icon: 'bookmark-outline' },
        ] as const
      ).map((tab) => (
        <Tabs.Screen
          key={tab.name}
          name={tab.name}
          options={{
            title: tab.title,
            tabBarIcon: ({ color, size }) => <Ionicons name={tab.icon} size={size} color={color} />,
          }}
        />
      ))}
    </Tabs>
  );
}
