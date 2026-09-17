import { useState } from 'react';
import { Share, View } from 'react-native';
import { router } from 'expo-router';
import type { Product } from '@safeeat/contracts';
import { api } from './session';
import { useFavorites } from './favorites';
import { Button, Copy, ErrorNotice } from './ui';
import { IconButton } from './compact';
export function ItemActions({
  kind,
  item,
  compact = false,
}: {
  kind: 'products' | 'restaurants';
  item: Product | { id: string };
  compact?: boolean;
}) {
  const favorites = useFavorites(kind, item.id);
  const [error, setError] = useState<unknown>(null);
  const [sharing, setSharing] = useState(false);
  const saved = favorites.membership.data?.saved ?? false;
  async function share() {
    setSharing(true);
    setError(null);
    try {
      const data = await api<{ url: string }>(`/shares/${kind}/${encodeURIComponent(item.id)}`, {
        method: 'POST',
      });
      await Share.share({
        message: `Consulta la información y sus fuentes en SafeEat: ${data.url}`,
        url: data.url,
      });
    } catch (e) {
      setError(e);
    } finally {
      setSharing(false);
    }
  }
  return (
    <>
      {compact ? (
        <View style={{ flexDirection: 'row', gap: 8 }}>
          <IconButton
            icon={saved ? 'bookmark' : 'bookmark-outline'}
            label={saved ? 'Quitar de favoritos' : 'Guardar en favoritos'}
            selected={saved}
            disabled={
              favorites.mutation.isPending || (!!favorites.user && !favorites.membership.data)
            }
            onPress={() => {
              if (!favorites.user) router.push('/auth/login');
              else favorites.mutation.mutate({ item, remove: saved });
            }}
          />
          <IconButton
            icon="share-outline"
            label="Compartir"
            disabled={sharing}
            onPress={() => void share()}
          />
        </View>
      ) : (
        <>
          <Button
            title={saved ? '♥  Quitar de favoritos' : '♡  Guardar en favoritos'}
            disabled={
              favorites.mutation.isPending || (!!favorites.user && !favorites.membership.data)
            }
            onPress={() => {
              if (!favorites.user) {
                router.push('/auth/login');
                return;
              }
              favorites.mutation.mutate({ item, remove: saved });
            }}
          />
          <Button
            secondary
            title={sharing ? 'Preparando enlace…' : 'Compartir'}
            disabled={sharing}
            onPress={() => void share()}
          />
        </>
      )}
      {'userCreated' in item && item.userCreated && (
        <Copy muted size={12}>
          Al compartir esta etiqueta, cualquier persona con el enlace podrá leer su texto. Elimina
          tu cuenta para revocar estos enlaces.
        </Copy>
      )}
      {(error || favorites.mutation.error || favorites.membership.error) && (
        <ErrorNotice
          error={error || favorites.mutation.error || favorites.membership.error}
          retry={() => void favorites.membership.refetch()}
        />
      )}
    </>
  );
}
