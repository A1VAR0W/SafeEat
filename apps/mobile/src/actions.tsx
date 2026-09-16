import { useState } from 'react';
import { Share } from 'react-native';
import { router } from 'expo-router';
import type { Product } from '@safeeat/contracts';
import { api } from './session';
import { useFavorites } from './favorites';
import { Button, Copy, ErrorNotice } from './ui';
export function ItemActions({
  kind,
  item,
}: {
  kind: 'products' | 'restaurants';
  item: Product | { id: string };
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
      <Button
        title={saved ? '♥  Quitar de favoritos' : '♡  Guardar en favoritos'}
        disabled={favorites.mutation.isPending || (!!favorites.user && !favorites.membership.data)}
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
