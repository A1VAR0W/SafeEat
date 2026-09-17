import { useEffect, useState, type PropsWithChildren } from 'react';
import {
  AccessibilityInfo,
  KeyboardAvoidingView,
  Platform,
  Pressable,
  ScrollView,
  Text,
  TextInput,
  View,
  useColorScheme,
  type TextInputProps,
  type ViewStyle,
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { Image } from 'expo-image';
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
}: PropsWithChildren<{ muted?: boolean; size?: number; bold?: boolean }>) {
  const t = useTheme();
  return (
    <Text
      style={{
        color: muted ? t.muted : t.text,
        fontSize: size * t.textScale,
        lineHeight: size * 1.45 * t.textScale,
        fontWeight: bold ? '700' : '400',
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
  return (
    <SafeAreaView edges={['left', 'right']} style={{ flex: 1, backgroundColor: t.bg }}>
      <KeyboardAvoidingView
        style={{ flex: 1 }}
        behavior={Platform.OS === 'ios' ? 'padding' : undefined}
      >
        {scroll ? (
          <ScrollView
            keyboardShouldPersistTaps="handled"
            contentContainerStyle={{ padding: 16, gap: 12, paddingBottom: 32 }}
          >
            {children}
          </ScrollView>
        ) : (
          <View style={{ flex: 1, padding: 16, gap: 12 }}>{children}</View>
        )}
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
}
export function Card({ children, style }: PropsWithChildren<{ style?: ViewStyle }>) {
  const t = useTheme();
  return (
    <View style={[{ padding: 16, borderRadius: 20, backgroundColor: t.card, gap: 10 }, style]}>
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
        accessibilityLabel={label}
        placeholderTextColor={t.muted}
        {...props}
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
        <Copy size={12}>
          {needsDetail
            ? '○ Toca para comprobar los ingredientes'
            : `${statusLabels[item.assessment.status].icon} ${statusLabels[item.assessment.status].label}`}
        </Copy>
        {item.informationSource !== 'unknown' && (
          <Copy muted size={11}>
            {item.informationSource === 'community'
              ? '👤 Etiqueta revisada por usuario'
              : '🌐 Datos de API'}
          </Copy>
        )}
      </View>
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
            <Copy muted size={12}>
              {e.source}
            </Copy>
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
export function RecentProducts() {
  const query = useQuery<Product[]>({
    queryKey: ['recent'],
    queryFn: async () => [],
    staleTime: Infinity,
  });
  return (
    <View>
      {query.data?.length ? (
        <View>
          {query.data.slice(0, 12).map((p) => (
            <ProductRow key={p.id} item={p} />
          ))}
        </View>
      ) : (
        <Copy muted>Los productos que consultes aparecerán aquí, también sin conexión.</Copy>
      )}
    </View>
  );
}
