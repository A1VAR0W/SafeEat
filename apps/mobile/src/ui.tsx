import { useEffect, useState, type PropsWithChildren } from 'react';
import {
  AccessibilityInfo,
  Keyboard,
  Platform,
  Pressable,
  Text,
  TextInput,
  View,
  useColorScheme,
  type TextInputProps,
  type ViewStyle,
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { useHeaderHeight } from '@react-navigation/elements';
import { KeyboardAwareScrollView, KeyboardAvoidingView } from 'react-native-keyboard-controller';
import { Image } from 'expo-image';
import Ionicons from '@expo/vector-icons/Ionicons';
import { router } from 'expo-router';
import { useQuery } from '@tanstack/react-query';
import { type Product, statusLabels, productDisclaimer } from '@safeeat/contracts';
import { es } from './i18n';
import { usePreferences, textScales } from './preferences';
export function useTheme() {
  const preferences = usePreferences();
  const system = useColorScheme();
  const dark =
    preferences.theme === 'dark' || (preferences.theme === 'system' && system === 'dark');
  return {
    dark,
    textScale: textScales[preferences.textSize],
    bg: dark ? '#071C16' : '#F5F9F4',
    card: dark ? '#102B22' : '#FFFFFF',
    text: dark ? '#F0F8F0' : '#064B39',
    muted: dark ? '#B2C5B9' : '#557368',
    line: dark ? '#2C5141' : '#D8E8DE',
    accent: dark ? '#7AC943' : '#087653',
    onAccent: dark ? '#062318' : '#FFFFFF',
    brandDeep: '#006B4E',
    brandLime: '#7AC943',
    danger: dark ? '#FFB4A8' : '#A02D22',
  };
}
export function useReducedMotion() {
  const [reduced, setReduced] = useState(false);
  useEffect(() => {
    void AccessibilityInfo.isReduceMotionEnabled().then(setReduced);
    const sub = AccessibilityInfo.addEventListener('reduceMotionChanged', setReduced);
    return () => sub.remove();
  }, []);
  return reduced;
}
export function Copy({
  children,
  muted = false,
  size = 16,
  bold = false,
  numberOfLines,
  color,
}: PropsWithChildren<{
  muted?: boolean;
  size?: number;
  bold?: boolean;
  numberOfLines?: number;
  color?: string;
}>) {
  const t = useTheme();
  return (
    <Text
      numberOfLines={numberOfLines}
      style={{
        color: color ?? (muted ? t.muted : t.text),
        fontSize: size * t.textScale,
        lineHeight: size * 1.45 * t.textScale,
        fontWeight: bold ? '700' : '400',
        flexShrink: 1,
        minWidth: 0,
      }}
    >
      {children}
    </Text>
  );
}
export function Title({ children }: PropsWithChildren) {
  const t = useTheme();
  return (
    <Text
      accessibilityRole="header"
      style={{
        fontSize: 24 * t.textScale,
        lineHeight: 29 * t.textScale,
        fontWeight: '800',
        letterSpacing: -0.4,
        flexShrink: 1,
        minWidth: 0,
        color: t.text,
        marginBottom: 2,
      }}
    >
      {children}
    </Text>
  );
}
export function Screen({ children, scroll = true }: PropsWithChildren<{ scroll?: boolean }>) {
  const t = useTheme();
  const headerHeight = useHeaderHeight();
  return (
    <SafeAreaView edges={['left', 'right']} style={{ flex: 1, backgroundColor: t.bg }}>
      {scroll ? (
        <KeyboardAwareScrollView
          bottomOffset={20}
          keyboardShouldPersistTaps="handled"
          keyboardDismissMode={Platform.OS === 'ios' ? 'interactive' : 'on-drag'}
          contentContainerStyle={{ padding: 16, gap: 12, paddingBottom: 32 }}
        >
          {children}
        </KeyboardAwareScrollView>
      ) : (
        <KeyboardAvoidingView
          style={{ flex: 1 }}
          behavior="padding"
          keyboardVerticalOffset={headerHeight}
        >
          <View style={{ flex: 1, padding: 16, gap: 12 }}>{children}</View>
        </KeyboardAvoidingView>
      )}
    </SafeAreaView>
  );
}
export function Card({ children, style }: PropsWithChildren<{ style?: ViewStyle }>) {
  const t = useTheme();
  return (
    <View
      style={[
        { padding: 16, borderRadius: 20, backgroundColor: t.card, gap: 10, minWidth: 0 },
        style,
      ]}
    >
      {children}
    </View>
  );
}
export function Button({
  title,
  onPress,
  secondary = false,
  disabled = false,
  destructive = false,
}: {
  title: string;
  onPress: () => void;
  secondary?: boolean;
  disabled?: boolean;
  destructive?: boolean;
}) {
  const t = useTheme();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={title}
      accessibilityState={{ disabled }}
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => ({
        minHeight: 48,
        paddingHorizontal: 18,
        paddingVertical: 12,
        borderRadius: 16,
        alignItems: 'center',
        justifyContent: 'center',
        backgroundColor: secondary ? t.card : destructive ? t.danger : t.accent,
        opacity: disabled ? 0.45 : pressed ? 0.72 : 1,
        borderWidth: secondary ? 1 : 0,
        borderColor: t.line,
      })}
    >
      <Text
        style={{
          fontSize: 16 * t.textScale,
          fontWeight: '700',
          color: secondary ? t.text : t.onAccent,
          textAlign: 'center',
          flexShrink: 1,
          maxWidth: '100%',
        }}
      >
        {title}
      </Text>
    </Pressable>
  );
}
export function Field({ label, ...props }: TextInputProps & { label: string }) {
  const t = useTheme();
  return (
    <View style={{ gap: 6 }}>
      <Copy bold>{label}</Copy>
      <TextInput
        returnKeyType={props.multiline ? 'default' : 'done'}
        accessibilityLabel={label}
        placeholderTextColor={t.muted}
        {...props}
        onSubmitEditing={(event) => {
          props.onSubmitEditing?.(event);
          if (!props.multiline) Keyboard.dismiss();
        }}
        style={[
          {
            minHeight: 48,
            padding: 16,
            borderRadius: 14,
            borderWidth: 1,
            borderColor: t.line,
            backgroundColor: t.card,
            color: t.text,
            fontSize: 17 * t.textScale,
          },
          props.multiline ? { minHeight: 190, textAlignVertical: 'top' } : {},
          props.style,
        ]}
      />
    </View>
  );
}
export function ErrorNotice({ error, retry }: { error: unknown; retry?: () => void }) {
  const t = useTheme();
  return (
    <View
      accessibilityRole="alert"
      style={{ gap: 10, padding: 16, borderRadius: 16, borderWidth: 1, borderColor: t.danger }}
    >
      <Copy>
        {error instanceof Error ? error.message : 'No se ha podido completar la operación.'}
      </Copy>
      {retry && <Button title={es.retry} onPress={retry} secondary />}
    </View>
  );
}
export function Skeleton() {
  const t = useTheme();
  return (
    <View accessibilityLabel={es.loading} style={{ gap: 14 }}>
      {[110, 70, 70].map((h, i) => (
        <View key={i} style={{ height: h, borderRadius: 20, backgroundColor: t.line }} />
      ))}
    </View>
  );
}
export function ProductRow({ item, onOpen }: { item: Product; onOpen?: () => void }) {
  const t = useTheme();
  const needsDetail = item.stale && item.informationSource === 'unknown';
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`${item.name}. ${needsDetail ? 'Toca para comprobar los ingredientes' : statusLabels[item.assessment.status].label}`}
      onPress={() => {
        onOpen?.();
        router.push(`/product/${item.id}`);
      }}
      style={({ pressed }) => ({
        flexDirection: 'row',
        gap: 14,
        padding: 16,
        marginBottom: 12,
        borderRadius: 20,
        backgroundColor: t.card,
        opacity: pressed ? 0.7 : 1,
      })}
    >
      {item.imageUrl ? (
        <Image
          source={item.imageUrl}
          style={{ width: 62, height: 76, borderRadius: 12 }}
          contentFit="contain"
          cachePolicy="memory-disk"
        />
      ) : (
        <View style={{ width: 62, justifyContent: 'center', alignItems: 'center' }}>
          <Copy size={30}>▥</Copy>
        </View>
      )}
      <View style={{ flex: 1, gap: 4 }}>
        <Copy bold>{item.name}</Copy>
        {item.brand && (
          <Copy muted size={13}>
            {item.brand}
          </Copy>
        )}
        <ProductGlutenLabel product={item} compact />
        {needsDetail && (
          <Copy muted size={11}>
            Toca para comprobar los ingredientes
          </Copy>
        )}
        {item.informationSource === 'community' && (
          <Copy muted size={11}>
            👤 Etiqueta aportada por un usuario
          </Copy>
        )}
      </View>
    </Pressable>
  );
}
export function ProductGlutenLabel({
  product,
  compact = false,
}: {
  product: Product;
  compact?: boolean;
}) {
  const free = product.glutenReports?.glutenFree ?? 0;
  const contains = product.glutenReports?.containsGluten ?? 0;
  const unknown = product.assessment.status === 'UNKNOWN';
  const conflict =
    (free > 0 && contains > 0) ||
    (contains > 0 && product.assessment.status === 'GLUTEN_FREE') ||
    (free > 0 && product.assessment.status === 'CONTAINS_GLUTEN');
  const report = conflict
    ? 'Información sobre gluten contradictoria'
    : unknown && contains
      ? `${contains} usuario${contains === 1 ? '' : 's'} indica${contains === 1 ? '' : 'n'} gluten`
      : unknown && free
        ? `${free} usuario${free === 1 ? '' : 's'} indica${free === 1 ? '' : 'n'} sin gluten`
        : null;
  const warning = unknown || conflict;
  return (
    <View
      style={{
        flexDirection: 'row',
        alignItems: 'center',
        gap: 5,
        flexShrink: 1,
        flexWrap: 'wrap',
        minWidth: 0,
      }}
    >
      {warning ? (
        <Ionicons name="alert-circle-outline" size={compact ? 16 : 22} color="#D4A017" />
      ) : (
        <Copy size={compact ? 12 : 18}>{statusLabels[product.assessment.status].icon}</Copy>
      )}
      <View style={{ flexShrink: 1, minWidth: 0 }}>
        <Copy size={compact ? 12 : 17} bold={!compact}>
          {report ?? (unknown ? 'No verificado' : statusLabels[product.assessment.status].label)}
        </Copy>
      </View>
      {report && unknown && free > 0 && !contains && (
        <Copy muted size={11}>
          · sin confirmar
        </Copy>
      )}
    </View>
  );
}
export function ProductTile({
  item,
  columns,
  onOpen,
}: {
  item: Product;
  columns: 1 | 2 | 3;
  onOpen?: () => void;
}) {
  const t = useTheme();
  if (columns === 1) return <ProductRow item={item} onOpen={onOpen} />;
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`Ver ${item.name}`}
      onPress={() => {
        onOpen?.();
        router.push(`/product/${item.id}`);
      }}
      style={({ pressed }) => ({
        flex: 1,
        maxWidth: columns === 2 ? '50%' : '33.33%',
        margin: 3,
        padding: columns === 3 ? 6 : 10,
        borderRadius: 16,
        backgroundColor: t.card,
        opacity: pressed ? 0.7 : 1,
        minHeight: columns === 3 ? 145 : 190,
      })}
    >
      {item.imageUrl ? (
        <Image
          source={item.imageUrl}
          contentFit="contain"
          cachePolicy="memory-disk"
          style={{ width: '100%', height: columns === 3 ? 76 : 105 }}
        />
      ) : (
        <View
          style={{
            height: columns === 3 ? 76 : 105,
            alignItems: 'center',
            justifyContent: 'center',
          }}
        >
          <Copy muted>▥</Copy>
        </View>
      )}
      <Copy bold size={columns === 3 ? 11 : 13} numberOfLines={columns === 3 ? 2 : 3}>
        {item.name}
      </Copy>
      {columns === 2 && <ProductGlutenLabel product={item} compact />}
      {columns === 3 && item.assessment.status === 'UNKNOWN' && (
        <Ionicons name="alert-circle-outline" size={16} color="#D4A017" />
      )}
    </Pressable>
  );
}
export function Assessment({ product }: { product: Product }) {
  const t = useTheme();
  const { assessment: a } = product;
  const label = statusLabels[a.status];
  const color =
    a.status === 'CONTAINS_GLUTEN'
      ? t.danger
      : a.status === 'MAY_CONTAIN_TRACES'
        ? t.dark
          ? '#F3CC82'
          : '#805600'
        : a.status === 'UNKNOWN'
          ? '#D4A017'
          : t.accent;
  return (
    <Card style={{ borderWidth: 2, borderColor: color }}>
      <Text
        accessibilityRole="header"
        style={{ fontSize: 23 * t.textScale, fontWeight: '800', color }}
      >
        {label.icon} {label.label}
      </Text>
      <Copy muted size={13}>
        Confianza en la evidencia: {Math.round(a.confidence * 100)} %
      </Copy>
      {a.reasons.map((r) => (
        <Copy key={r.code}>{r.message}</Copy>
      ))}
      <Copy bold>¿En qué se basa?</Copy>
      {a.evidence.length ? (
        a.evidence.map((e, i) => (
          <View key={i} style={{ gap: 3 }}>
            <Copy>“{e.text}”</Copy>
          </View>
        ))
      ) : (
        <Copy muted>No hay evidencia suficiente.</Copy>
      )}
      <Copy muted size={12}>
        {productDisclaimer}
      </Copy>
    </Card>
  );
}
export function RecentProducts({ columns = 1 }: { columns?: 1 | 2 | 3 }) {
  const query = useQuery<Product[]>({
    queryKey: ['recent'],
    queryFn: async () => [],
    staleTime: Infinity,
  });
  return (
    <View>
      {query.data?.length ? (
        <View style={columns > 1 ? { flexDirection: 'row', flexWrap: 'wrap' } : undefined}>
          {query.data.slice(0, 12).map((p) => (
            <ProductTile key={p.id} item={p} columns={columns} />
          ))}
        </View>
      ) : (
        <Copy muted>Los productos que consultes aparecerán aquí, también sin conexión.</Copy>
      )}
    </View>
  );
}
