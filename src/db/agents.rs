//! Functions for operating agents

use sqlx::{Postgres, Transaction};
use time::OffsetDateTime;

/// Get a named checkpoint, or None if it wasn't set.
pub(crate) async fn get_checkpoint<'a>(
    id: &str,
    tx: &mut Transaction<'a, Postgres>,
) -> Result<Option<OffsetDateTime>, sqlx::Error> {
    let date: Option<OffsetDateTime> =
        sqlx::query_scalar("SELECT date FROM checkpoint WHERE id = $1;")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?;

    Ok(date)
}

/// Get a named checkpoint, or None if it wasn't set.
pub(crate) async fn set_checkpoint<'a>(
    id: &str,
    value: OffsetDateTime,
    tx: &mut Transaction<'a, Postgres>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO checkpoint (id, date)
        VALUES ($1, $2)
        ON CONFLICT (id) DO
        UPDATE SET date = $2",
    )
    .bind(id)
    .bind(value)
    .execute(&mut **tx)
    .await?;

    Ok(())
}
