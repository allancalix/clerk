mod link;

use std::sync::Arc;

use serde::Serialize;
use sqlx::{pool::PoolConnection, Connection, Error as SqlxError, Row};
use thiserror::Error;

use crate::core::{Posting, Transaction};
use crate::upstream::TransactionEntry;

#[derive(Debug, Error)]
pub enum Error {
    #[error("conflicting data already exists")]
    AlreadyExists,
    #[error(transparent)]
    Parse(#[from] serde_json::Error),
    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error(transparent)]
    Database(#[from] SqlxError),
    #[error(transparent)]
    Encoding(#[from] rusty_money::MoneyError),
    #[error(transparent)]
    Decode(#[from] ulid::DecodeError),
    #[error(transparent)]
    Unknown(#[from] anyhow::Error),
}

impl PartialEq for Error {
    fn eq(&self, other: &Error) -> bool {
        self.to_string() == other.to_string()
    }
}

type Result<T> = ::std::result::Result<T, Error>;

pub struct SqliteStore {
    conn: Arc<sqlx::pool::Pool<sqlx::sqlite::Sqlite>>,
}

impl SqliteStore {
    pub async fn new(uri: &str) -> Result<Self> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().connect(uri).await?;

        let mut conn = pool.acquire().await?;
        sqlx::migrate!("./migrations").run(&mut conn).await?;

        Ok(Self {
            conn: Arc::new(pool),
        })
    }

    pub fn links(&mut self) -> link::LinkStore {
        link::LinkStore::new(self)
    }

    pub async fn tx_by_plaid_id(
        &mut self,
        item_id: &str,
        plaid_id: &str,
    ) -> Result<Option<String>> {
        let conn = &mut self.conn.acquire().await?;
        canonical_txn_id(conn, item_id, plaid_id).await
    }

    pub async fn save_tx<S: Serialize>(
        &mut self,
        item_id: &str,
        upstream_id: &str,
        tx: &TransactionEntry<S>,
    ) -> Result<()> {
        let source = tx.serialize_string()?;
        let canonical = tx.canonical.clone();
        let item_id = item_id.to_string();
        let upstream_id = upstream_id.to_string();

        self.conn
            .acquire()
            .await?
            .transaction(|conn| {
                Box::pin(async move {
                    let txn_id = canonical.id.to_string();
                    let postings = canonical.postings.clone();

                    if select_transaction_connection(conn, &item_id, &upstream_id)
                        .await?
                        .is_some()
                    {
                        return Err(crate::store::Error::AlreadyExists);
                    }

                    insert_transaction(conn, &canonical, source).await?;
                    for p in postings {
                        insert_posting(conn, &txn_id, &p).await?;
                    }

                    transaction_plaid_connection(conn, &item_id, &txn_id, &upstream_id).await?;

                    Ok(())
                })
            })
            .await
    }
}

async fn insert_transaction<'a>(
    conn: &mut sqlx::Transaction<'a, sqlx::sqlite::Sqlite>,
    entry: &Transaction,
    source: String,
) -> Result<()> {
    let result = sqlx::query(
        "INSERT INTO transactions (
            id,
            date,
            payee,
            narration,
            status,
            source
            ) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(entry.id.to_string())
    .bind(entry.date.format("%Y-%m-%d").to_string().as_str())
    .bind(entry.payee.clone())
    .bind(entry.narration.clone())
    .bind(entry.status.to_string())
    .bind(source)
    .execute(conn)
    .await;

    match result {
        Ok(_) => Ok(()),
        Err(e) => match e {
            SqlxError::Database(e) => {
                // Uniqueness check fails.
                if e.code() == Some(std::borrow::Cow::Borrowed("1555")) {
                    return Err(Error::AlreadyExists);
                }

                Err(Error::from(sqlx::Error::Database(e)))
            }
            _ => Err(Error::from(e)),
        },
    }
}

async fn canonical_txn_id<'a>(
    conn: &mut PoolConnection<sqlx::sqlite::Sqlite>,
    link_id: &str,
    plaid_txn_id: &str,
) -> Result<Option<String>> {
    let row = sqlx::query(
        "SELECT txn_id FROM int_transactions_links WHERE item_id = $1 AND plaid_txn_id = $2",
    )
    .bind(link_id)
    .bind(plaid_txn_id)
    .fetch_optional(conn)
    .await?;

    Ok(row.map(|row| {
        row.try_get("txn_id")
            .expect("connections must have an transaction id")
    }))
}

async fn select_transaction_connection<'a>(
    conn: &mut sqlx::Transaction<'a, sqlx::sqlite::Sqlite>,
    link_id: &str,
    plaid_txn_id: &str,
) -> Result<Option<String>> {
    let row = sqlx::query(
        "SELECT txn_id FROM int_transactions_links WHERE item_id = $1 AND plaid_txn_id = $2",
    )
    .bind(link_id)
    .bind(plaid_txn_id)
    .fetch_optional(conn)
    .await?;

    Ok(row.map(|row| {
        row.try_get("txn_id")
            .expect("connections must have an transaction id")
    }))
}

async fn transaction_plaid_connection<'a>(
    conn: &mut sqlx::Transaction<'a, sqlx::sqlite::Sqlite>,
    item_id: &str,
    txn_id: &str,
    plaid_txn_id: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO int_transactions_links (
            item_id,
            txn_id,
            plaid_txn_id
        ) VALUES ($1, $2, $3)",
    )
    .bind(item_id)
    .bind(txn_id)
    .bind(plaid_txn_id)
    .execute(conn)
    .await?;

    Ok(())
}

async fn insert_posting<'a>(
    conn: &mut sqlx::Transaction<'a, sqlx::sqlite::Sqlite>,
    txn_id: &str,
    post: &Posting,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO postings (
            id,
            txn_id,
            account,
            amount,
            currency,
            status
        ) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(ulid::Ulid::new().to_string())
    .bind(txn_id)
    .bind(&post.account.0)
    .bind(post.units.amount().to_string())
    .bind(post.units.currency().to_string())
    .bind("RESOLVED")
    .execute(conn)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use rplaid::model::Transaction as PlaidTransaction;
    use ulid::Ulid;

    use crate::core::Status;
    use crate::plaid::Link;

    use super::*;

    const UPSTREAM_TXN_ID: &str = "test-upstream-id";

    fn plaid_transaction() -> PlaidTransaction {
        PlaidTransaction {
            transaction_type: "".to_string(),
            pending_transaction_id: None,
            category_id: None,
            category: None,
            location: None,
            payment_meta: None,
            account_owner: None,
            name: "".to_string(),
            original_description: None,
            account_id: "".to_string(),
            amount: 33.into(),
            iso_currency_code: None,
            unofficial_currency_code: None,
            date: "2022-05-01".to_string(),
            pending: false,
            transaction_id: "1234-test".to_string(),
            payment_channel: "".to_string(),
            merchant_name: None,
            authorized_date: None,
            authorized_datetime: None,
            datetime: None,
            check_number: None,
            transaction_code: None,
        }
    }

    async fn test_store() -> SqliteStore {
        SqliteStore::new("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn can_save_transaction() {
        let mut store = test_store().await;
        let link = Link {
            alias: "test_link".to_string(),
            access_token: "1234".to_string(),
            item_id: "plaid-id-123".to_string(),
            state: crate::plaid::LinkStatus::Active,
            sync_cursor: None,
        };
        store.links().save(&link).await.unwrap();

        let entry = TransactionEntry {
            canonical: Transaction {
                id: Ulid::new(),
                date: NaiveDate::parse_from_str("2022-05-01", "%Y-%m-%d").unwrap(),
                narration: "Test Transaction".to_string(),
                payee: None,
                postings: Default::default(),
                links: Default::default(),
                tags: Default::default(),
                meta: Default::default(),
                status: Status::Resolved,
            },
            source: plaid_transaction(),
        };

        store
            .save_tx(&link.item_id, UPSTREAM_TXN_ID, &entry)
            .await
            .unwrap();
    }
}
