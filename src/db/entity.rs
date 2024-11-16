//! Entities and their identifiers.

use scholarly_identifiers::identifiers::Identifier;
use sqlx::{Pool, Postgres};

/// Retrieve the entity_id for an identifier. Create if necessary.
/// This function is idempotent.
/// To be called from outside a transaction so that it can't be rolled back.
/// May be called from code subject to a READ COMMITTED transaction.
pub(crate) async fn resolve_identifier(
    identifier: &Identifier,
    pool: &Pool<Postgres>,
) -> Result<i64, sqlx::Error> {
    let (identifier_str, identifier_type) = identifier.to_id_string_pair();

    // Assume that most identifiers won't have been seen before. So start with
    // the INSERT ... IGNORE and query later on if it did already exist.
    let row: Option<(i64,)> = sqlx::query_as(
        "INSERT INTO entity
         (identifier_type, identifier)
        VALUES ($1, $2)
        ON CONFLICT (identifier_type, identifier) DO NOTHING
        RETURNING entity_id;",
    )
    .bind(identifier_type as i32)
    .bind(&identifier_str)
    .fetch_optional(pool)
    .await?;

    // If it was created, return it.
    if let Some((entity_id,)) = row {
        return Ok(entity_id);
    }

    // If it did already exist, the INSERT ... IGNORE will have done nothing.
    let row: (i64,) = sqlx::query_as(
        "SELECT entity_id FROM entity
                 WHERE identifier_type = $1 AND identifier = $2;",
    )
    .bind(identifier_type as i32)
    .bind(&identifier_str)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}
