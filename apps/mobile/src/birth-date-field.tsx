import { useState } from 'react';
import { Platform, Pressable, View } from 'react-native';
import DateTimePicker, { type DateTimePickerEvent } from '@react-native-community/datetimepicker';
import Ionicons from '@expo/vector-icons/Ionicons';
import { Copy, useTheme } from './ui';

const latest = new Date(new Date().getFullYear(), new Date().getMonth(), new Date().getDate() - 1);
const earliest = new Date(1900, 0, 1);
const format = (date: Date) =>
  `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;

export function BirthDateField({
  value,
  onChange,
}: {
  value: string;
  onChange: (date: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const t = useTheme();
  const parts = value.split('-').map(Number);
  const chosen =
    parts.length === 3 && parts.every(Number.isFinite)
      ? new Date(parts[0]!, parts[1]! - 1, parts[2]!)
      : new Date(2000, 0, 1);
  function change(event: DateTimePickerEvent, date?: Date) {
    if (Platform.OS !== 'ios') setOpen(false);
    if (event.type === 'set' && date) onChange(format(date));
  }
  return (
    <View style={{ gap: 6 }}>
      <Copy bold>Fecha de nacimiento</Copy>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel="Elegir fecha de nacimiento"
        onPress={() => setOpen(!open)}
        style={{
          minHeight: 52,
          paddingHorizontal: 16,
          borderRadius: 14,
          borderWidth: 1,
          borderColor: t.line,
          backgroundColor: t.card,
          flexDirection: 'row',
          alignItems: 'center',
          gap: 12,
        }}
      >
        <Ionicons name="calendar-outline" size={21} color={t.accent} />
        <View style={{ flex: 1 }}>
          <Copy>{value ? value.split('-').reverse().join('/') : 'Seleccionar fecha'}</Copy>
        </View>
        <Ionicons name="chevron-down" size={18} color={t.muted} />
      </Pressable>
      {open && (
        <View style={{ backgroundColor: t.card, borderRadius: 14 }}>
          <DateTimePicker
            value={chosen}
            mode="date"
            display={Platform.OS === 'ios' ? 'spinner' : 'default'}
            minimumDate={earliest}
            maximumDate={latest}
            onChange={change}
          />
          {Platform.OS === 'ios' && (
            <Pressable
              accessibilityRole="button"
              accessibilityLabel="Aceptar fecha"
              onPress={() => {
                if (!value) onChange(format(chosen));
                setOpen(false);
              }}
              style={{ alignSelf: 'flex-end', padding: 14 }}
            >
              <Copy bold>Aceptar</Copy>
            </Pressable>
          )}
        </View>
      )}
    </View>
  );
}
