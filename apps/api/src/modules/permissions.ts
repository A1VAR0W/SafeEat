import type { Database } from '../db/pool';
import { ApiError } from '../plugins/errors';
export async function reviewNote(db: Database, userId: string, note = '') {
  const value = note.trim();
  const authorized = await db.query(
    "SELECT 1 FROM user_roles WHERE user_id=$1 AND role_code='authorized_celiac'",
    [userId],
  );
  if (!authorized.rowCount && value.length < 10)
    throw new ApiError(
      400,
      'REVIEW_REASON_REQUIRED',
      'Explica lo que has comprobado con al menos 10 caracteres.',
    );
  return value;
}
