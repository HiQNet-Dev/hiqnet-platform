//! Cursor opaco de instalaciones por organización.

use super::*;

pub(super) const CURSOR_VERSION: u8 = 1;
pub(super) const INSTALLATIONS_KIND: &str = "installations";
pub(super) const MAX_CURSOR_LEN: usize = 2048;
pub(super) const ENROLLMENT_SCOPE: &str = "enrollment";
pub(super) const BEGIN_ENDPOINT: &str = "begin";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InstallationCursor {
    pub(super) v: u8,
    pub(super) k: String,
    pub(super) o: Uuid,
    pub(super) n: String,
    pub(super) i: Uuid,
}

pub(super) fn encode_cursor(cursor: &InstallationCursor) -> EnrollmentResult<String> {
    let json = serde_json::to_vec(cursor).map_err(storage)?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json))
}

pub(super) fn decode_cursor(
    value: &str,
    organization: OrganizationId,
) -> EnrollmentResult<InstallationCursor> {
    if value.len() > MAX_CURSOR_LEN {
        return Err(EnrollmentError::Invalid);
    }
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| EnrollmentError::Invalid)?;
    let cursor: InstallationCursor =
        serde_json::from_slice(&bytes).map_err(|_| EnrollmentError::Invalid)?;
    if cursor.v != CURSOR_VERSION
        || cursor.k != INSTALLATIONS_KIND
        || cursor.o != organization.as_uuid()
    {
        return Err(EnrollmentError::Invalid);
    }
    Ok(cursor)
}
