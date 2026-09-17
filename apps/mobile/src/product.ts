import { useQuery } from '@tanstack/react-query';
import { ProductSchema, ShareSchema } from '@safeeat/contracts';
import { api } from './session';
export function useProduct(id: string, shareToken?: string) {
  return useQuery({
    queryKey: shareToken ? ['shared-product', id, shareToken] : ['product', id],
    gcTime: shareToken ? 0 : 7 * 86400000,
    queryFn: async ({ signal }) => {
      if (!shareToken)
        return ProductSchema.parse(await api(`/products/${encodeURIComponent(id)}`, { signal }));
      const share = ShareSchema.parse(
        await api(`/public/shares/${encodeURIComponent(shareToken)}`, { signal }),
      );
      if (share.type !== 'product' || share.item.id !== id) throw new Error('Enlace no válido.');
      return share.item;
    },
  });
}
