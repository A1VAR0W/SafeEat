import { useEffect } from 'react';
import { View, useWindowDimensions } from 'react-native';
import { useLocalSearchParams, router } from 'expo-router';
import { Image } from 'expo-image';
import { productDisclaimer } from '@safeeat/contracts';
import { queryClient, rememberProduct } from '../../src/query';
import { useProduct } from '../../src/product';
import { ItemActions } from '../../src/actions';
import { SettingRow } from '../../src/compact';
import {
  Card,
  Copy,
  ErrorNotice,
  ProductGlutenLabel,
  Screen,
  Skeleton,
  Title,
  useTheme,
} from '../../src/ui';
import { es } from '../../src/i18n';
import { CollectionPicker } from '../../src/collections';
import { CommunityReviews } from '../../src/community-reviews';
export default function ProductDetail() {
  const { id, shareToken } = useLocalSearchParams<{ id: string; shareToken?: string }>();
  const query = useProduct(id, shareToken),
    p = query.data,
    t = useTheme();
  useEffect(() => {
    if (!p || shareToken) return;
    rememberProduct(p);
    if (p.barcode) queryClient.setQueryData(['barcode', p.barcode], p);
  }, [p, shareToken]);
  const { fontScale } = useWindowDimensions();
  function open(section: string) {
    router.push({
      pathname: '/product-info',
      params: { id, section, ...(shareToken ? { shareToken } : {}) },
    });
  }
  return (
    <Screen>
      {query.error && <ErrorNotice error={query.error} retry={() => void query.refetch()} />}
      {!p && query.isPending && <Skeleton />}
      {p && (
        <>
          <View style={{ flexDirection: 'row', gap: 12, alignItems: 'center' }}>
            {p.imageUrl && (
              <Image
                source={p.imageUrl}
                contentFit="contain"
                style={{ width: 76, height: 92, borderRadius: 16 }}
                cachePolicy="memory-disk"
              />
            )}
            <View style={{ flex: 1, gap: 2 }}>
              <Title>{p.name}</Title>
              {p.brand && (
                <Copy muted size={13}>
                  {p.brand}
                </Copy>
              )}
            </View>
          </View>
          {!(shareToken && p.userCreated) && <ItemActions kind="products" item={p} compact />}
          {p.informationSource === 'community' && (
            <Copy muted size={12}>
              👤 Ingredientes aportados por un usuario · Comprueba la foto y la etiqueta actual
            </Copy>
          )}
          {p.communityContribution && p.informationSource === 'api' && (
            <Copy muted size={12}>
              👤 También hay una foto de ingredientes aportada por la comunidad.
            </Copy>
          )}
          {!shareToken && <CollectionPicker productId={p.id} />}
          {!shareToken && !p.userCreated && <CommunityReviews kind="products" id={p.id} />}
          {(p.stale ||
            query.isError ||
            Date.now() - new Date(p.fetchedAt).getTime() > 86400000) && (
            <Copy muted size={12}>
              {es.cached}
            </Copy>
          )}
          <Card
            style={{
              borderWidth: 1,
              borderColor: p.assessment.status === 'CONTAINS_GLUTEN' ? t.danger : t.line,
              gap: 5,
            }}
          >
            <ProductGlutenLabel product={p} />
            {!!p.glutenReports &&
              p.glutenReports.glutenFree + p.glutenReports.containsGluten > 0 && (
                <Copy muted size={12}>
                  Revisiones de usuarios: {p.glutenReports.glutenFree} indica sin gluten ·{' '}
                  {p.glutenReports.containsGluten} indica gluten. Comprueba la etiqueta actual.
                </Copy>
              )}
            <Copy muted size={13}>
              Evidencia{' '}
              {p.assessment.confidence >= 0.8
                ? 'alta'
                : p.assessment.confidence >= 0.5
                  ? 'moderada'
                  : 'limitada'}
            </Copy>
            <Copy size={13}>
              {p.assessment.reasons[0]?.message ?? 'Revisa la información disponible.'}
            </Copy>
          </Card>
          <View style={{ flexDirection: 'row', justifyContent: 'space-between', gap: 8 }}>
            <Copy size={14}>Calidad nutricional</Copy>
            <Copy bold size={14}>
              {p.quality.score === null ? 'Sin datos' : `${p.quality.score}/100`}
            </Copy>
          </View>
          <View
            style={{
              flexDirection: 'row',
              flexWrap: 'wrap',
              backgroundColor: t.card,
              borderRadius: 18,
            }}
          >
            {(
              [
                { icon: 'help-circle-outline', title: '¿Por qué?', section: 'why' },
                { icon: 'list-outline', title: 'Ingredientes', section: 'ingredients' },
                { icon: 'nutrition-outline', title: 'Nutrición', section: 'nutrition' },
                { icon: 'documents-outline', title: 'Fuentes', section: 'sources' },
              ] as const
            ).map((action) => (
              <View
                key={action.section}
                style={{ width: fontScale * t.textScale > 1.2 ? '100%' : '50%' }}
              >
                <SettingRow
                  compact
                  icon={action.icon}
                  title={action.title}
                  onPress={() => open(action.section)}
                />
              </View>
            ))}
          </View>
          <SettingRow
            icon="camera-outline"
            title={p.barcode ? 'Aportar foto de ingredientes' : 'Revisar mi etiqueta'}
            subtitle={p.barcode ? 'Otros usuarios podrán consultar la etiqueta' : undefined}
            onPress={() =>
              router.push({
                pathname: '/scan/ingredients',
                params: p.barcode ? { barcode: p.barcode } : {},
              })
            }
          />
          {!!p.barcode && !shareToken && (
            <SettingRow
              icon="alert-circle-outline"
              title="Aportar revisión sobre gluten"
              subtitle="Indica qué has comprobado en la etiqueta"
              onPress={() => router.push({ pathname: '/product/report', params: { id: p.id } })}
            />
          )}
          <Copy muted size={12}>
            {productDisclaimer}
          </Copy>
        </>
      )}
    </Screen>
  );
}
