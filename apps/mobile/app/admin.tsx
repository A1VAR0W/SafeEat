import { useEffect, useState } from 'react';
import { ActivityIndicator, Alert, Modal, Pressable, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { Ionicons } from '@expo/vector-icons';
import { router } from 'expo-router';
import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { ProductSchema, type Product } from '@safeeat/contracts';
import { api, RequestError, useSession } from '../src/session';
import { usePermissions } from '../src/permissions';
import { useDebounced } from '../src/use-debounced';
import { IconButton, SearchField } from '../src/compact';
import { Button, Card, Copy, ErrorNotice, Field, Screen, Title, useTheme } from '../src/ui';

const person = z.object({
  id: z.string().uuid(),
  email: z.string(),
  firstName: z.string().nullable(),
  lastName: z.string().nullable(),
  roles: z.array(z.string()),
});
const review = z.object({
  id: z.string().uuid(),
  revision: z.string().uuid(),
  resourceId: z.string(),
  resourceName: z.string(),
  status: z.string(),
  note: z.string(),
  scope: z.string(),
  authorId: z.string().uuid(),
  authorName: z.string(),
  authorEmail: z.string(),
  updatedAt: z.string(),
  verifiedAt: z.string().nullable(),
  verificationMethod: z.enum(['authorized', 'manual']).nullable(),
  confirmations: z.number().int(),
  revokedAt: z.string().nullable(),
  revocationReason: z.string().nullable(),
});
type Person = z.infer<typeof person>;
type Review = z.infer<typeof review>;
const labels: Record<string, string> = {
  GLUTEN_FREE: 'Sin gluten',
  CONTAINS_GLUTEN: 'Contiene gluten',
  GLUTEN_FREE_OPTIONS: 'Opciones sin gluten',
  DEDICATED_GLUTEN_FREE: 'Totalmente sin gluten',
  NO_GLUTEN_FREE_OPTIONS: 'Sin opciones sin gluten',
};

function Choices<T extends string>({
  items,
  value,
  onChange,
}: {
  items: readonly (readonly [T, string])[];
  value: T;
  onChange: (value: T) => void;
}) {
  const t = useTheme();
  return (
    <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 8 }}>
      {items.map(([key, label]) => (
        <Pressable
          key={key}
          accessibilityRole="button"
          accessibilityState={{ selected: key === value }}
          onPress={() => onChange(key)}
          style={{
            flexGrow: 1,
            minHeight: 46,
            padding: 12,
            borderRadius: 16,
            alignItems: 'center',
            backgroundColor: key === value ? t.accent : t.card,
            borderWidth: 1,
            borderColor: key === value ? t.accent : t.line,
          }}
        >
          <Copy bold size={14} color={key === value ? t.onAccent : t.text}>
            {label}
          </Copy>
        </Pressable>
      ))}
    </View>
  );
}
export default function Admin() {
  const t = useTheme();
  const client = useQueryClient();
  const { user } = useSession();
  const { admin, profile } = usePermissions();
  const [tab, setTab] = useState<'users' | 'reviews'>('users');
  const [kind, setKind] = useState<'restaurants' | 'products'>('restaurants');
  const [state, setState] = useState<'active' | 'revoked'>('active');
  const [text, setText] = useState('');
  const q = useDebounced(text.trim());
  const [selected, setSelected] = useState<{
    item: Review;
    kind: 'restaurants' | 'products';
  } | null>(null);
  const [reason, setReason] = useState('');
  const [notice, setNotice] = useState('');
  const users = useInfiniteQuery({
    queryKey: ['admin-users', user?.id, q],
    enabled: admin && tab === 'users',
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam, signal }) =>
      z
        .object({ items: z.array(person), nextCursor: z.string().nullable() })
        .parse(
          await api(
            `/admin/users?${new URLSearchParams({ q, ...(pageParam ? { after: pageParam } : {}) })}`,
            { signal },
            true,
          ),
        ),
    getNextPageParam: (last) => last.nextCursor ?? undefined,
    staleTime: 10000,
    gcTime: 60000,
  });
  const reviews = useInfiniteQuery({
    queryKey: ['admin-reviews', user?.id, kind, state, q],
    enabled: admin && tab === 'reviews',
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam, signal }) =>
      z
        .object({ items: z.array(review), nextCursor: z.string().nullable() })
        .parse(
          await api(
            `/admin/reviews?${new URLSearchParams({ kind, state, q, ...(pageParam ? { after: pageParam } : {}) })}`,
            { signal },
            true,
          ),
        ),
    getNextPageParam: (last) => last.nextCursor ?? undefined,
    staleTime: 10000,
    gcTime: 60000,
  });
  const grant = useMutation({
    mutationFn: ({ id, authorized }: { id: string; authorized: boolean }) =>
      api(`/admin/users/${id}/authorized`, { method: authorized ? 'PUT' : 'DELETE' }, true),
    onSuccess: async (_data, input) => {
      setNotice(input.authorized ? 'Autorización concedida' : 'Autorización retirada');
      await Promise.all([
        client.invalidateQueries({ queryKey: ['admin-users'] }),
        client.invalidateQueries({ queryKey: ['profile'] }),
        ...['reviews', 'product', 'barcode', 'favorites-products', 'restaurant', 'restaurants'].map(
          (key) => client.invalidateQueries({ queryKey: [key] }),
        ),
      ]);
    },
  });
  const verifyProduct = useMutation({
    mutationFn: (item: Review) =>
      api(
        `/admin/reviews/products/${item.id}/verify`,
        {
          method: 'POST',
          body: JSON.stringify({ revision: item.revision }),
        },
        true,
      ),
    onSuccess: async () => {
      setNotice('Producto verificado desde el panel de administración.');
      await Promise.all(
        ['admin-reviews', 'reviews', 'product', 'barcode', 'favorites-products', 'recent'].map(
          (key) => client.invalidateQueries({ queryKey: [key] }),
        ),
      );
    },
    onError: () => {
      void client.invalidateQueries({ queryKey: ['admin-reviews'] });
    },
  });
  const moderation = useMutation({
    mutationFn: (input: {
      id: string;
      revision: string;
      kind: 'restaurants' | 'products';
      action: 'revoke' | 'restore';
      reason: string;
      resourceId: string;
    }) =>
      api(
        `/admin/reviews/${input.kind}/${input.id}/moderate`,
        {
          method: 'POST',
          body: JSON.stringify({
            revision: input.revision,
            action: input.action,
            reason: input.reason,
          }),
        },
        true,
      ),
    onSuccess: async (_data, input) => {
      setSelected(null);
      setNotice(input.action === 'revoke' ? 'Revisión revocada' : 'Revisión restaurada');
      await Promise.all(
        [
          'admin-reviews',
          'reviews',
          'restaurants',
          'restaurant',
          'product',
          'barcode',
          'favorites-products',
          'favorites-restaurants',
          'recent',
        ].map((key) => client.invalidateQueries({ queryKey: [key] })),
      );
      if (input.kind === 'products') {
        void client
          .fetchQuery({
            queryKey: ['product', input.resourceId],
            queryFn: async ({ signal }) =>
              ProductSchema.parse(
                await api(`/products/${encodeURIComponent(input.resourceId)}`, { signal }, true),
              ),
          })
          .then((item) =>
            client.setQueryData<Product[]>(['recent'], (old) =>
              old?.map((product) => (product.id === item.id ? item : product)),
            ),
          )
          .catch(() => {});
      }
    },
    onError: () => {
      void client.invalidateQueries({ queryKey: ['admin-reviews'] });
    },
  });
  const failure =
    users.error ?? reviews.error ?? grant.error ?? verifyProduct.error ?? moderation.error;
  const denied = failure instanceof RequestError && failure.status === 403;
  useEffect(() => {
    if (denied) {
      void client.invalidateQueries({ queryKey: ['profile'] });
      client.removeQueries({ queryKey: ['admin-users'] });
      client.removeQueries({ queryKey: ['admin-reviews'] });
    }
  }, [denied, client]);
  function changeRole(item: Person) {
    const authorized = item.roles.includes('authorized_celiac');
    grant.reset();
    Alert.alert(
      authorized ? 'Retirar autorización' : 'Autorizar celíaco',
      `${item.firstName ?? item.email}${authorized ? ' dejará de verificar sus revisiones automáticamente.' : ' podrá enviar revisiones verificadas y sin motivo obligatorio.'}`,
      [
        { text: 'Cancelar', style: 'cancel' },
        {
          text: authorized ? 'Retirar' : 'Autorizar',
          style: authorized ? 'destructive' : 'default',
          onPress: () => grant.mutate({ id: item.id, authorized: !authorized }),
        },
      ],
    );
  }
  if (!user)
    return (
      <Screen>
        <Title>Administración</Title>
        <Copy muted>Inicia sesión con tu cuenta de administrador.</Copy>
        <Button title="Iniciar sesión" onPress={() => router.push('/auth/login')} />
      </Screen>
    );
  if (!admin || denied)
    return (
      <Screen>
        <Title>Administración</Title>
        {profile.isFetching ? (
          <ActivityIndicator color={t.accent} />
        ) : (
          <Copy muted>No tienes permisos para acceder a este apartado.</Copy>
        )}
        {profile.error && (
          <ErrorNotice error={profile.error} retry={() => void profile.refetch()} />
        )}
      </Screen>
    );
  const current = tab === 'users' ? users : reviews;
  return (
    <>
      <Screen>
        <View style={{ flexDirection: 'row', gap: 12, alignItems: 'center' }}>
          <Ionicons name="shield-checkmark-outline" size={30} color={t.accent} />
          <View style={{ flex: 1 }}>
            <Title>Cuida la comunidad</Title>
            <Copy muted size={13}>
              Gestiona autorizaciones y revisa aportaciones.
            </Copy>
          </View>
        </View>
        <Choices
          items={[
            ['users', 'Usuarios'],
            ['reviews', 'Revisiones'],
          ]}
          value={tab}
          onChange={(value) => {
            setTab(value);
            setText('');
            setNotice('');
          }}
        />
        {tab === 'reviews' && (
          <>
            <Choices
              items={[
                ['restaurants', 'Restaurantes'],
                ['products', 'Productos'],
              ]}
              value={kind}
              onChange={(value) => {
                setKind(value);
                setText('');
              }}
            />
            <Choices
              items={[
                ['active', 'Activas'],
                ['revoked', 'Revocadas'],
              ]}
              value={state}
              onChange={setState}
            />
          </>
        )}
        <SearchField
          label={tab === 'users' ? 'Buscar usuarios' : 'Buscar revisiones'}
          value={text}
          onChangeText={setText}
          placeholder={tab === 'users' ? 'Nombre o correo' : 'Nombre, correo o motivo'}
          maxLength={120}
        />
        {!!notice && (
          <Copy color={t.accent} bold>
            {notice}
          </Copy>
        )}
        {current.isPending && <ActivityIndicator color={t.accent} />}
        {current.error && (
          <ErrorNotice error={current.error} retry={() => void current.refetch()} />
        )}
        {grant.error && <ErrorNotice error={grant.error} />}
        {verifyProduct.error && <ErrorNotice error={verifyProduct.error} />}
        {tab === 'users'
          ? users.data?.pages
              .flatMap((page) => page.items)
              .map((item) => (
                <Card key={item.id}>
                  <Copy bold>
                    {[item.firstName, item.lastName].filter(Boolean).join(' ') ||
                      'Cuenta sin nombre'}
                  </Copy>
                  <Copy muted size={13}>
                    {item.email}
                  </Copy>
                  {item.roles.includes('ADMIN') && (
                    <Copy muted size={12}>
                      Administrador
                    </Copy>
                  )}
                  <Copy
                    color={item.roles.includes('authorized_celiac') ? t.accent : t.muted}
                    size={13}
                  >
                    {item.roles.includes('authorized_celiac')
                      ? 'Celíaco autorizado'
                      : 'Sin autorización de verificación'}
                  </Copy>
                  <Button
                    title={
                      grant.isPending && grant.variables?.id === item.id
                        ? 'Guardando…'
                        : item.roles.includes('authorized_celiac')
                          ? 'Retirar autorización'
                          : 'Autorizar celíaco'
                    }
                    secondary={!item.roles.includes('authorized_celiac')}
                    destructive={item.roles.includes('authorized_celiac')}
                    disabled={grant.isPending}
                    onPress={() => changeRole(item)}
                  />
                </Card>
              ))
          : reviews.data?.pages
              .flatMap((page) => page.items)
              .map((item) => (
                <Card key={item.id}>
                  <Copy bold>
                    {item.resourceName === item.resourceId
                      ? kind === 'restaurants'
                        ? 'Restaurante'
                        : 'Producto'
                      : item.resourceName}
                  </Copy>
                  <Copy color={item.revokedAt ? t.danger : t.accent} size={14} bold>
                    {labels[item.status] ?? item.status}
                  </Copy>
                  <Copy size={14}>{item.note || 'Sin motivo añadido'}</Copy>
                  <Copy muted size={12}>
                    {item.authorName || 'Cuenta sin nombre'} · {item.authorEmail}
                  </Copy>
                  <Copy muted size={12}>
                    {new Date(item.updatedAt).toLocaleDateString('es-ES')}
                    {item.scope === 'chain' ? ' · Compartida con otros locales' : ''}
                  </Copy>
                  {!!item.revokedAt && (
                    <Copy muted size={13}>
                      Revocada: {item.revocationReason}
                    </Copy>
                  )}
                  {item.verifiedAt && (
                    <Copy color={t.accent} size={13} bold>
                      {item.verificationMethod === 'authorized'
                        ? 'Verificado por celíaco autorizado'
                        : `Verificado manualmente · ${item.confirmations} confirmación${item.confirmations === 1 ? '' : 'es'}`}{' '}
                      · {new Date(item.verifiedAt).toLocaleDateString('es-ES')}
                    </Copy>
                  )}
                  {kind === 'products' &&
                    !item.revokedAt &&
                    !item.verifiedAt &&
                    item.authorId !== user?.id && (
                      <Button
                        title={
                          verifyProduct.isPending && verifyProduct.variables?.id === item.id
                            ? 'Verificando…'
                            : 'Verificar producto'
                        }
                        disabled={verifyProduct.isPending}
                        onPress={() =>
                          Alert.alert(
                            'Verificar producto',
                            `Comprueba personalmente esta aportación antes de verificarla.\n\n${item.resourceName}\n${item.note || 'Sin motivo añadido'}`,
                            [
                              { text: 'Cancelar', style: 'cancel' },
                              {
                                text: 'Verificar',
                                onPress: () => verifyProduct.mutate(item),
                              },
                            ],
                          )
                        }
                      />
                    )}
                  <Button
                    secondary
                    title="Ver ficha"
                    onPress={() =>
                      router.push({
                        pathname: kind === 'restaurants' ? '/restaurant/[id]' : '/product/[id]',
                        params: { id: item.resourceId },
                      })
                    }
                  />
                  <Button
                    title={item.revokedAt ? 'Restaurar revisión' : 'Revocar revisión'}
                    destructive={!item.revokedAt}
                    disabled={moderation.isPending}
                    onPress={() => {
                      moderation.reset();
                      setReason('');
                      setSelected({ item, kind });
                    }}
                  />
                </Card>
              ))}
        {current.isSuccess && current.data?.pages.every((page) => !page.items.length) && (
          <Card>
            <Copy muted>
              {q ? 'No encontramos coincidencias.' : 'No hay entradas en este apartado.'}
            </Copy>
          </Card>
        )}
        {current.hasNextPage && (
          <Button
            secondary
            title={current.isFetchingNextPage ? 'Cargando…' : 'Cargar más'}
            disabled={current.isFetchingNextPage}
            onPress={() => void current.fetchNextPage()}
          />
        )}
      </Screen>
      <Modal
        visible={!!selected}
        onRequestClose={() => setSelected(null)}
        presentationStyle="pageSheet"
      >
        <SafeAreaView style={{ flex: 1, backgroundColor: t.bg }} edges={['top', 'bottom']}>
          <View style={{ padding: 16, flexDirection: 'row', alignItems: 'center', gap: 12 }}>
            <View style={{ flex: 1 }}>
              <Copy bold size={20}>
                {selected?.item.revokedAt ? 'Restaurar revisión' : 'Revocar revisión'}
              </Copy>
            </View>
            <IconButton icon="close" label="Cerrar revisión" onPress={() => setSelected(null)} />
          </View>
          <Screen>
            <Card>
              <Copy bold>{selected?.item.resourceName}</Copy>
              <Copy>{selected?.item.note || 'Sin motivo añadido'}</Copy>
              <Copy muted size={13}>
                {selected?.item.authorEmail}
              </Copy>
            </Card>
            <Copy muted>
              {selected?.item.revokedAt
                ? 'La aportación volverá a ser visible. Las confirmaciones manuales deberán revisarse de nuevo.'
                : 'La aportación dejará de ser visible y de contar en la clasificación de gluten, también en los locales con los que se compartió.'}
            </Copy>
            <Field
              label="Motivo de la decisión"
              value={reason}
              onChangeText={setReason}
              multiline
              maxLength={500}
              placeholder="Explica por qué debe retirarse o recuperarse…"
            />
            {moderation.error && <ErrorNotice error={moderation.error} />}
            <Button
              title={
                moderation.isPending
                  ? 'Guardando…'
                  : selected?.item.revokedAt
                    ? 'Confirmar restauración'
                    : 'Confirmar revocación'
              }
              destructive={!selected?.item.revokedAt}
              disabled={moderation.isPending || reason.trim().length < 5}
              onPress={() => {
                if (selected)
                  moderation.mutate({
                    kind: selected.kind,
                    id: selected.item.id,
                    revision: selected.item.revision,
                    action: selected.item.revokedAt ? 'restore' : 'revoke',
                    reason: reason.trim(),
                    resourceId: selected.item.resourceId,
                  });
              }}
            />
          </Screen>
        </SafeAreaView>
      </Modal>
    </>
  );
}
