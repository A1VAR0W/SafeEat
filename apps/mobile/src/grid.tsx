import { useEffect, useSyncExternalStore } from 'react';
import { Pressable } from 'react-native';
import AsyncStorage from '@react-native-async-storage/async-storage';
import { Ionicons } from '@expo/vector-icons';
import { Copy, useTheme } from './ui';

export type GridColumns = 1 | 2 | 3;
const key = 'safeeat.product-columns.v1';
let current: GridColumns = 1;
let loaded = false;
const listeners = new Set<() => void>();
function setColumns(value: GridColumns) {
  current = value;
  listeners.forEach((listener) => listener());
}
function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
function load() {
  if (loaded) return;
  loaded = true;
  void AsyncStorage.getItem(key).then((stored) => {
    if (stored === '1' || stored === '2' || stored === '3')
      setColumns(Number(stored) as GridColumns);
    else if (stored === '4') setColumns(3);
  });
}
export function useProductColumns() {
  const columns = useSyncExternalStore(subscribe, () => current);
  useEffect(load, []);
  function choose(value: GridColumns) {
    setColumns(value);
    void AsyncStorage.setItem(key, String(value));
  }
  return { columns, choose };
}
export function GridPicker({
  columns,
  onChange,
}: {
  columns: GridColumns;
  onChange: (value: GridColumns) => void;
}) {
  const t = useTheme();
  const next = columns === 3 ? 1 : ((columns + 1) as GridColumns);
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`${columns} producto${columns === 1 ? '' : 's'} por fila. Cambiar a ${next}`}
      onPress={() => onChange(next)}
      style={{
        alignSelf: 'flex-end',
        minHeight: 44,
        flexDirection: 'row',
        alignItems: 'center',
        gap: 8,
        paddingHorizontal: 13,
        borderRadius: 14,
        backgroundColor: t.card,
      }}
    >
      <Ionicons name="grid-outline" size={18} color={t.accent} />
      <Copy bold size={13}>
        {columns} por fila
      </Copy>
    </Pressable>
  );
}
