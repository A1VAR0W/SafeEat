import { useQuery } from '@tanstack/react-query';
import { UserSchema } from '@safeeat/contracts';
import { api, useSession } from './session';
export function usePermissions() {
  const { user } = useSession();
  const profile = useQuery({
    queryKey: ['profile', 'permissions', user?.id],
    enabled: !!user,
    queryFn: async ({ signal }) => UserSchema.parse(await api('/auth/me', { signal }, true)),
    staleTime: 30000,
  });
  const roles = profile.data?.roles ?? user?.roles ?? [];
  return {
    profile,
    admin: roles.includes('ADMIN'),
    authorized: roles.includes('authorized_celiac'),
  };
}
