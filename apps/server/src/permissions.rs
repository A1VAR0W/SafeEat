//! Review permissions (port of apps/api/src/modules/permissions.ts).

use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::extract::has_role;
use crate::state::AppState;
use crate::util::js_len;

/// Untrusted reviewers must explain what they checked (10+ characters).
pub async fn review_note(state: &AppState, user: Uuid, note: &str) -> ApiResult<String> {
    let value = note.trim().to_string();
    if js_len(&value) < 10 && !has_role(state, user, "authorized_celiac").await? {
        return Err(ApiError::new(
            400,
            "REVIEW_REASON_REQUIRED",
            "Explica lo que has comprobado con al menos 10 caracteres.",
        ));
    }
    Ok(value)
}
