import { useEffect, useState } from 'react';
import { Pressable, View } from 'react-native';
import AsyncStorage from '@react-native-async-storage/async-storage';
import { Copy, useTheme } from './ui';

export type GridColumns = 1 | 2 | 4;
const key = 'safeeat.product-columns.v1';
export function useProductColumns() {
  const [columns, setColumns] = useState<GridColumns>(1);
  useEffect(() => {
    void AsyncStorage.getItem(key).then((stored) => {
      if (stored === '1' || stored === '2' || stored === '4')
        setColumns(Number(stored) as GridColumns);
    });
  }, []);
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
  return (
    <View style={{ flexDirection: 'row', alignItems: 'center', gap: 6 }}>
      <Copy muted size={12}>
        Por fila
      </Copy>
      {([1, 2, 4] as const).map((value) => (
        <Pressable
          key={value}
          accessibilityRole="button"
          accessibilityLabel={`${value} producto${value === 1 ? '' : 's'} por fila`}
          accessibilityState={{ selected: columns === value }}
          onPress={() => onChange(value)}
          style={{
            minWidth: 38,
            minHeight: 38,
            alignItems: 'center',
            justifyContent: 'center',
            borderRadius: 12,
            backgroundColor: columns === value ? t.accent : t.card,
          }}
        >
          <Copy bold size={13}>
            {value}
          </Copy>
        </Pressable>
      ))}
    </View>
  );
}
