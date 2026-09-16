import { Tabs } from 'expo-router';
import Ionicons from '@expo/vector-icons/Ionicons';
import { useTheme } from '../../src/ui';
export default function TabLayout() {
  const t = useTheme();
  return (
    <Tabs
      screenOptions={{
        headerShown: false,
        tabBarActiveTintColor: t.accent,
        tabBarInactiveTintColor: t.muted,
        tabBarStyle: { backgroundColor: t.card, borderTopColor: t.line },
        tabBarLabelStyle: { fontSize: 11, fontWeight: '600' },
      }}
    >
      {(
        [
          { name: 'home', title: 'Inicio', icon: 'home-outline' },
          { name: 'scan', title: 'Escanear', icon: 'scan' },
          { name: 'restaurants', title: 'Restaurantes', icon: 'restaurant-outline' },
          { name: 'favorites', title: 'Favoritos', icon: 'heart-outline' },
          { name: 'profile', title: 'Perfil', icon: 'person-outline' },
        ] as const
      ).map((tab) => (
        <Tabs.Screen
          key={tab.name}
          name={tab.name}
          options={{
            title: tab.title,
            tabBarIcon: ({ color, size }) => (
              <Ionicons name={tab.icon} size={tab.name === 'scan' ? 30 : size} color={color} />
            ),
          }}
        />
      ))}
    </Tabs>
  );
}
