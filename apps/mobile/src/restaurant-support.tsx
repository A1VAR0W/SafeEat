import { Pressable } from 'react-native';
import { Copy, useTheme } from './ui';

export type RestaurantSupport =
  'DEDICATED_GLUTEN_FREE' | 'GLUTEN_FREE_OPTIONS' | 'NO_GLUTEN_FREE_OPTIONS';
const options: { value: RestaurantSupport; label: string }[] = [
  { value: 'DEDICATED_GLUTEN_FREE', label: 'El local indica que su comida es sin gluten' },
  { value: 'GLUTEN_FREE_OPTIONS', label: 'Ofrece opciones sin gluten' },
  { value: 'NO_GLUTEN_FREE_OPTIONS', label: 'No ofrece opciones sin gluten' },
];
export function RestaurantSupportPicker({
  value,
  onChange,
}: {
  value: RestaurantSupport | null;
  onChange: (choice: RestaurantSupport) => void;
}) {
  const t = useTheme();
  return options.map((option) => (
    <Pressable
      key={option.value}
      accessibilityRole="radio"
      accessibilityState={{ selected: value === option.value }}
      onPress={() => onChange(option.value)}
      style={{
        padding: 14,
        minHeight: 54,
        borderRadius: 16,
        backgroundColor: t.card,
        borderWidth: value === option.value ? 2 : 1,
        borderColor: value === option.value ? t.accent : t.line,
      }}
    >
      <Copy bold>{option.label}</Copy>
    </Pressable>
  ));
}
