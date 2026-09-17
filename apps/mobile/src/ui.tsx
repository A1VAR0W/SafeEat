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
    bg: dark ? '#101815' : '#F4F7F3',
    card: dark ? '#1B2721' : '#FFFFFF',
    text: dark ? '#F0F6F1' : '#173C2A',
    muted: dark ? '#AABCAF' : '#5B6D60',
    line: dark ? '#35483B' : '#DCE5DD',
    accent: dark ? '#AAE5B9' : '#245F3D',
    onAccent: dark ? '#11301D' : '#FFFFFF',
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
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`${item.name}. ${statusLabels[item.assessment.status].label}`}
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
          cachePolicy="disk"
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
          {statusLabels[item.assessment.status].icon} {statusLabels[item.assessment.status].label}
        </Copy>
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
        <ScrollView
          horizontal
          showsHorizontalScrollIndicator={false}
          contentContainerStyle={{ gap: 10 }}
        >
          {query.data.slice(0, 8).map((p) => (
            <Pressable
              key={p.id}
              accessibilityRole="button"
              accessibilityLabel={`${p.name}. ${statusLabels[p.assessment.status].label}`}
              onPress={() => router.push(`/product/${p.id}`)}
              style={{ width: 136, gap: 6, paddingVertical: 8 }}
            >
              {p.imageUrl && (
                <Image
                  source={p.imageUrl}
                  contentFit="contain"
                  cachePolicy="disk"
                  style={{ height: 66, borderRadius: 12 }}
                />
              )}
              <Copy size={13} bold>
                {p.name.length > 44 ? `${p.name.slice(0, 44)}…` : p.name}
              </Copy>
              <Copy size={11}>
                {statusLabels[p.assessment.status].icon} {statusLabels[p.assessment.status].label}
              </Copy>
            </Pressable>
          ))}
        </ScrollView>
      ) : (
        <Copy muted>Los productos que consultes aparecerán aquí, también sin conexión.</Copy>
      )}
    </View>
  );
}
