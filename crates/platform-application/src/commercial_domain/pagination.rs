//! Cursores opacos y paginación keyset.

use super::*;

pub(super) fn take_page_cursor<T>(
    items: &mut Vec<T>,
    limit: u32,
    dataset: &str,
    key: impl Fn(&T) -> (OffsetDateTime, Uuid),
) -> Option<String> {
    if items.len() as u32 > limit {
        items.pop();
        items.last().map(|item| {
            let (timestamp, id) = key(item);
            encode_cursor(dataset, timestamp, id)
        })
    } else {
        None
    }
}

pub(super) fn encode_cursor(dataset: &str, timestamp: OffsetDateTime, id: Uuid) -> String {
    URL_SAFE_NO_PAD.encode(format!(
        "{}:{}:{}",
        dataset,
        timestamp.unix_timestamp_nanos(),
        id
    ))
}

pub(super) fn decode_cursor(value: &str, dataset: &str) -> CommercialResult<CursorKey> {
    if value.is_empty() || value.len() > MAX_CURSOR_LEN {
        return Err(CommercialError::InvalidInput);
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| CommercialError::InvalidInput)?;
    let text = String::from_utf8(decoded).map_err(|_| CommercialError::InvalidInput)?;
    let mut parts = text.splitn(3, ':');
    let actual_dataset = parts.next().ok_or(CommercialError::InvalidInput)?;
    let timestamp = parts.next().ok_or(CommercialError::InvalidInput)?;
    let id = parts.next().ok_or(CommercialError::InvalidInput)?;
    if actual_dataset != dataset {
        return Err(CommercialError::InvalidInput);
    }
    let timestamp = OffsetDateTime::from_unix_timestamp_nanos(
        timestamp
            .parse()
            .map_err(|_| CommercialError::InvalidInput)?,
    )
    .map_err(|_| CommercialError::InvalidInput)?;
    Ok(CursorKey {
        timestamp,
        id: Uuid::parse_str(id).map_err(|_| CommercialError::InvalidInput)?,
    })
}

pub(super) fn page_params(
    limit: Option<u32>,
    cursor: Option<&str>,
    dataset: &str,
) -> CommercialResult<(u32, Option<CursorKey>)> {
    let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT);
    if !(1..=MAX_PAGE_LIMIT).contains(&limit) {
        return Err(CommercialError::InvalidInput);
    }
    let after = cursor
        .map(|value| decode_cursor(value, dataset))
        .transpose()?;
    Ok((limit, after))
}
