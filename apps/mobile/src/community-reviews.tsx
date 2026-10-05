import { Alert, View } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { router } from 'expo-router';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { CommunityReviewSchema, type CommunityReview } from '@safeeat/contracts';
import { api, useSession } from './session';
import { Button, Card, Copy, ErrorNotice, useTheme } from './ui';

const labels: Record<string, string> = {
  GLUTEN_FREE: 'Sin gluten',
  CONTAINS_GLUTEN: 'Contiene gluten',
  DEDICATED_GLUTEN_FREE: 'Totalmente sin gluten',
  GLUTEN_FREE_OPTIONS: 'Con opciones sin gluten',
  NO_GLUTEN_FREE_OPTIONS: 'Sin opciones sin gluten',
};
export function CommunityReviews({ kind, id }: { kind: 'products' | 'restaurants'; id: string }) {
  const t = useTheme();
  const { user } = useSession();
  const client = useQueryClient();
  const path = `/${kind}/${encodeURIComponent(id)}/reviews`;
  const query = useQuery({
    queryKey: ['reviews', kind, id, user?.id],
    queryFn: async ({ signal }) =>
      z
        .object({ items: z.array(CommunityReviewSchema) })
        .parse(await api(path, { signal }, !!user)),
    staleTime: 0,
  });
  const confirm = useMutation({
    mutationFn: (review: CommunityReview) =>
      api(
        `${path}/${review.id}/confirm`,
        {
          method: 'POST',
          body: JSON.stringify({ revision: review.revision }),
        },
        true,
      ),
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: ['reviews'] });
    },
    onError: () => {
      void query.refetch();
    },
  });
  function ask(review: CommunityReview) {
    if (!user) {
      router.push('/auth/login');
      return;
    }
    Alert.alert(
      'Confirmar este motivo',
      'Confirma solo si has comprobado personalmente lo que explica esta revisión.',
      [
        { text: 'Cancelar', style: 'cancel' },
        { text: 'Lo he comprobado', onPress: () => confirm.mutate(review) },
      ],
    );
  }
  return (
    <View style={{ gap: 12 }}>
      {query.isPending && <Copy muted>Cargando revisiones…</Copy>}
      {query.error && <ErrorNotice error={query.error} retry={() => void query.refetch()} />}
      {confirm.error && <ErrorNotice error={confirm.error} />}
      {query.data?.items.map((review) => (
        <Card
          key={review.id}
          style={{ borderWidth: 1, borderColor: review.verifiedAt ? t.accent : t.line, gap: 10 }}
        >
          <Copy bold>{labels[review.status] ?? 'Revisión'}</Copy>
          <Copy size={15}>{review.note}</Copy>
          <Copy muted size={12}>
            {review.own ? 'Tu aportación' : 'Aportación de la comunidad'} ·{' '}
            {new Date(review.updatedAt).toLocaleDateString('es-ES')}
          </Copy>
          {review.inherited && (
            <Copy muted size={13}>
              Compartido desde otro local de esta cadena. Confirma la preparación en este
              establecimiento.
            </Copy>
          )}
          {review.verifiedAt ? (
            <View style={{ flexDirection: 'row', gap: 8, alignItems: 'center' }}>
              <Ionicons name="checkmark-circle" color={t.accent} size={22} />
              <View style={{ flex: 1 }}>
                <Copy bold color={t.accent} size={14}>
                  {review.verificationMethod === 'authorized'
                    ? 'Verificado · Celíaco autorizado'
                    : 'Verificado manual'}{' '}
                  · {new Date(review.verifiedAt).toLocaleDateString('es-ES')}
                </Copy>
                <Copy muted size={12}>
                  {review.verificationMethod === 'authorized'
                    ? 'Revisión de una persona autorizada'
                    : `Motivo confirmado por ${review.confirmations} ${review.confirmations === 1 ? 'persona' : 'personas'}`}
                </Copy>
              </View>
            </View>
          ) : (
            <Copy muted size={13}>
              Pendiente de confirmación por otra persona
            </Copy>
          )}
          {review.confirmedByMe ? (
            <Copy color={t.accent} size={13}>
              Has confirmado este motivo
            </Copy>
          ) : (
            !review.own &&
            review.verificationMethod !== 'authorized' && (
              <Button
                secondary
                title={
                  confirm.isPending && confirm.variables?.id === review.id
                    ? 'Confirmando…'
                    : 'Confirmar este motivo'
                }
                disabled={confirm.isPending}
                onPress={() => ask(review)}
              />
            )
          )}
        </Card>
      ))}
    </View>
  );
}
