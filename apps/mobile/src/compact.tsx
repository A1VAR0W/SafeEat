import type { PropsWithChildren, ComponentProps } from 'react';
import { Pressable, View, TextInput, type TextInputProps } from 'react-native';
import { router } from 'expo-router';
import Ionicons from '@expo/vector-icons/Ionicons';
import { Copy, useTheme } from './ui';
type Icon = ComponentProps<typeof Ionicons>['name'];
export function IconButton({
  icon,
  label,
  onPress,
  selected = false,
  disabled = false,
}: {
  icon: Icon;
  label: string;
  onPress: () => void;
  selected?: boolean;
  disabled?: boolean;
}) {
  const t = useTheme();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ selected, disabled }}
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => ({
        minWidth: 48,
        minHeight: 48,
        borderRadius: 16,
        alignItems: 'center',
        justifyContent: 'center',
        opacity: pressed || disabled ? 0.5 : 1,
        backgroundColor: selected ? t.accent : t.card,
      })}
    >
      <Ionicons name={icon} size={23} color={selected ? t.onAccent : t.text} />
    </Pressable>
  );
}
export function ProfileButton() {
  return (
    <IconButton
      icon="person-circle-outline"
      label="Abrir menú de perfil"
      onPress={() => router.push('/profile-menu')}
    />
  );
}
export function SearchField({ label, ...props }: TextInputProps & { label: string }) {
  const t = useTheme();
  return (
    <View
      style={{
        flexDirection: 'row',
        alignItems: 'center',
        gap: 10,
        paddingHorizontal: 14,
        borderRadius: 16,
        backgroundColor: t.card,
        borderWidth: 1,
        borderColor: t.line,
      }}
    >
      <Ionicons name="search-outline" size={21} color={t.muted} />
      <TextInput
        {...props}
        accessibilityLabel={label}
        placeholderTextColor={t.muted}
        returnKeyType="search"
        autoCorrect={false}
        style={{
          flex: 1,
          minHeight: 48,
          paddingVertical: 12,
          color: t.text,
          fontSize: 16 * t.textScale,
        }}
      />
    </View>
  );
}
export function SettingRow({
  icon,
  title,
  subtitle,
  onPress,
  compact = false,
}: {
  icon: Icon;
  title: string;
  subtitle?: string;
  onPress: () => void;
  compact?: boolean;
}) {
  const t = useTheme();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={title}
      onPress={onPress}
      style={({ pressed }) => ({
        flexDirection: 'row',
        alignItems: 'center',
        gap: 12,
        minHeight: 52,
        padding: 10,
        borderRadius: 14,
        opacity: pressed ? 0.6 : 1,
      })}
    >
      <Ionicons name={icon} size={22} color={t.accent} />
      <View style={{ flex: 1 }}>
        <Copy bold size={compact ? 14 : 16}>
          {title}
        </Copy>
        {subtitle && (
          <Copy muted size={12}>
            {subtitle}
          </Copy>
        )}
      </View>
      {!compact && <Ionicons name="chevron-forward" size={17} color={t.muted} />}
    </Pressable>
  );
}
export function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: readonly { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  const t = useTheme();
  return (
    <View
      accessibilityRole="radiogroup"
      style={{ flexDirection: 'row', backgroundColor: t.line, padding: 3, borderRadius: 15 }}
    >
      {options.map((option) => (
        <Pressable
          key={option.value}
          accessibilityRole="radio"
          accessibilityState={{ checked: value === option.value }}
          onPress={() => onChange(option.value)}
          style={{
            flex: 1,
            minHeight: 48,
            justifyContent: 'center',
            alignItems: 'center',
            padding: 6,
            borderRadius: 12,
            backgroundColor: value === option.value ? t.card : 'transparent',
          }}
        >
          <Copy size={13} bold={value === option.value}>
            {option.label}
          </Copy>
        </Pressable>
      ))}
    </View>
  );
}
export function SheetHeader({ children }: PropsWithChildren) {
  return (
    <View style={{ flexDirection: 'row', alignItems: 'center', gap: 12 }}>
      <View style={{ flex: 1 }}>
        <Copy bold size={20}>
          {children}
        </Copy>
      </View>
      <IconButton label="Cerrar panel" icon="close" onPress={() => router.back()} />
    </View>
  );
}
