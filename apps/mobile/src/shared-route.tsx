import { useLocalSearchParams, router } from 'expo-router';
import { useQuery } from '@tanstack/react-query';
import { ShareSchema } from '@safeeat/contracts';
import { api } from './session';
import { queryClient } from './query';
import { Assessment, Button, Copy, ErrorNotice, Screen, Skeleton, Title } from './ui';
export function SharedRoute({ type }: { type: 'product' | 'restaurant' }) {
  const { token } = useLocalSearchParams<{ token: string }>();
  const query = useQuery({
    queryKey: ['share', token],
    queryFn: async () =>
      ShareSchema.parse(await api(`/public/shares/${encodeURIComponent(token)}`)),
    gcTime: 0,
  });
  const data = query.data;
  return (
    <Screen>
      {query.isPending && <Skeleton />}
      {query.error && <ErrorNotice error={query.error} retry={() => void query.refetch()} />}{' '}
      {data && data.type === type && (
        <>
          <Title>{data.item.name}</Title>
          {data.type === 'product' ? (
            <Assessment product={data.item} />
          ) : (
            <Copy>{data.item.address}</Copy>
          )}
          <Button
            title="Ver todos los detalles"
            onPress={() => {
              queryClient.setQueryData([data.type, data.item.id], data.item);
              router.replace(
                data.type === 'product'
                  ? `/product/${data.item.id}`
                  : `/restaurant/${data.item.id}`,
              );
            }}
          />
        </>
      )}
    </Screen>
  );
}
