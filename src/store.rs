use std::sync::Arc;

use rplaid::client::Environment;
use rusty_money::{iso, Money};
use serde::Serialize;
use sqlx::{pool::PoolConnection, Connection, Error as SqlxError, FromRow, Row};
use thiserror::Error;
use tracing::debug;

use crate::core::{Account, Posting, Status, Transaction};
use crate::plaid::{Link, LinkStatus};
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

impl<'r, R: sqlx::Row> sqlx::FromRow<'r, R> for Link
where
    std::string::String: sqlx::Decode<'r, <R as Row>::Database> + sqlx::Type<<R as Row>::Database>,
    &'r str: sqlx::Decode<'r, <R as Row>::Database> + sqlx::Type<<R as Row>::Database>,
    &'static str: sqlx::ColumnIndex<R>,
{
    fn from_row(row: &'r R) -> ::std::result::Result<Self, SqlxError> {
        Ok(Link {
            item_id: row.try_get("item_id")?,
            alias: row.try_get("alias")?,
            access_token: row.try_get("access_token")?,
            env: from_enum(row.try_get("environment")?).unwrap(),
            state: from_status_enum(row.try_get("link_state")?).unwrap(),
        })
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

    pub async fn update_link(&mut self, link: &Link) -> Result<()> {
        sqlx::query("UPDATE plaid_links SET access_token = $1, link_state = $2, alias = $3 WHERE item_id = $4")
            .bind(&link.access_token)
            .bind(to_status_enum(&link.state))
            .bind(&link.alias)
            .bind(&link.item_id)
            .execute(&mut self.conn.acquire().await?).await?;

        Ok(())
    }

    pub async fn link(&mut self, id: &str) -> Result<Link> {
        let row = sqlx::query(
            "SELECT item_id, alias, access_token, link_state, environment FROM plaid_links WHERE item_id = $1")
        .bind(id)
        .fetch_one(&mut self.conn.acquire().await?)
        .await?;

        Ok(Link::from_row(&row)?)
    }

    pub async fn links(&mut self) -> Result<Vec<Link>> {
        let rows = sqlx::query(
            "SELECT item_id, alias, access_token, link_state, environment FROM plaid_links",
        )
        .fetch_all(&mut self.conn.acquire().await?)
        .await?;

        let mut links = vec![];
        for row in rows {
            links.push(Link::from_row(&row)?);
        }

        Ok(links)
    }

    pub async fn save_link(&mut self, link: &Link) -> Result<()> {
        sqlx::query("INSERT INTO plaid_links (item_id, alias, access_token, link_state, environment) VALUES ($1, $2, $3, $4, $5)")
            .bind(&link.item_id)
            .bind(&link.alias)
            .bind(&link.access_token)
            .bind(to_status_enum(&link.state))
            .bind(to_enum(&link.env))
            .execute(&mut self.conn.acquire().await?).await?;

        Ok(())
    }

    pub async fn delete_link(&mut self, id: &str) -> Result<Link> {
        let row = sqlx::query("DELETE FROM plaid_links WHERE item_id = $1 RETURNING item_id, alias, access_token, link_state, environment")
            .bind(id)
            .fetch_one(&mut self.conn.acquire().await?).await?;

        Ok(Link::from_row(&row)?)
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
                        debug!("transaction for {} already present", &upstream_id);

                        return Ok(());
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

    pub async fn transactions(&mut self) -> Result<Vec<Transaction>> {
        let conn = &mut self.conn.acquire().await?;
        let mut txns = select_transactions(conn).await?;
        for tx in &mut txns {
            tx.postings = select_postings(conn, tx.id.to_string().as_str()).await?;
        }

        Ok(txns)
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

async fn select_transactions(
    conn: &mut PoolConnection<sqlx::sqlite::Sqlite>,
) -> Result<Vec<Transaction>> {
    let rows = sqlx::query("SELECT id, payee, date, narration, status FROM transactions")
        .fetch_all(conn)
        .await?;

    Ok(rows
        .iter()
        .map(|row| {
            Ok(Transaction {
                id: ulid::Ulid::from_string(row.try_get("id")?)?,
                payee: row.try_get("payee")?,
                date: row.try_get("date")?,
                narration: row.try_get("narration")?,
                status: Status::from(row.try_get::<'_, String, &str>("status")?),
                postings: Default::default(),
                tags: Default::default(),
                links: Default::default(),
                meta: Default::default(),
            })
        })
        .map(Result::unwrap)
        .collect())
}

async fn select_postings<'a>(
    conn: &mut PoolConnection<sqlx::sqlite::Sqlite>,
    txn_id: &str,
) -> Result<Vec<Posting>> {
    let rows = sqlx::query("SELECT id, account, amount, currency FROM postings WHERE txn_id = $1")
        .bind(txn_id)
        .fetch_all(conn)
        .await?;

    Ok(rows
        .iter()
        .map(|row| {
            Ok(Posting {
                account: Account(row.try_get("account")?),
                units: Money::from_str(
                    row.try_get("amount")?,
                    iso::find(row.try_get("currency")?).expect("currency must be not null"),
                )?,
                meta: Default::default(),
                status: Status::Resolved,
            })
        })
        .map(Result::unwrap)
        .collect())
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

fn to_status_enum(status: &LinkStatus) -> String {
    match *status {
        LinkStatus::Degraded(_) => "REQUIRES_VERIFICATION".into(),
        LinkStatus::Active => "ACTIVE".into(),
    }
}

fn from_status_enum(status: &str) -> anyhow::Result<LinkStatus> {
    match status {
        "ACTIVE" => Ok(LinkStatus::Active),
        "REQUIRES_VERIFICATION" => Ok(LinkStatus::Degraded("requires verification".to_string())),
        s => Err(anyhow::anyhow!("unknown status {}", s)),
    }
}

fn to_enum(env: &Environment) -> String {
    match *env {
        Environment::Sandbox => "SANDBOX".into(),
        Environment::Development => "DEVELOPMENT".into(),
        Environment::Production => "PRODUCTION".into(),
        Environment::Custom(_) => "CUSTOM".into(),
    }
}

fn from_enum(env: &str) -> anyhow::Result<Environment> {
    match env {
        "SANDBOX" => Ok(Environment::Sandbox),
        "DEVELOPMENT" => Ok(Environment::Development),
        "PRODUCTION" => Ok(Environment::Production),
        s => Err(anyhow::anyhow!("unknown environment {}", s)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_store() -> SqliteStore {
        SqliteStore::new("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn save_plaid_link_to_table() {
        let mut store = test_store().await;

        let link = Link {
            alias: "test_link".to_string(),
            access_token: "1234".to_string(),
            item_id: "plaid-id-123".to_string(),
            state: crate::plaid::LinkStatus::Active,
            env: Environment::Development,
        };
        let result = store.save_link(&link).await;

        assert!(result.is_ok())
    }

    #[tokio::test]
    async fn list_plaid_links_to_table() {
        let mut store = test_store().await;
        let link = Link {
            alias: "test_link".to_string(),
            access_token: "1234".to_string(),
            item_id: "plaid-id-123".to_string(),
            state: crate::plaid::LinkStatus::Active,
            env: Environment::Development,
        };
        store.save_link(&link).await.unwrap();

        let second_link = Link {
            item_id: "plaid-id-456".to_string(),
            ..link
        };
        store.save_link(&second_link).await.unwrap();

        let links = store.links().await.unwrap();

        assert_eq!(links.len(), 2);
    }

    #[tokio::test]
    async fn update_plaid_link() {
        let mut store = test_store().await;
        let link = Link {
            alias: "test_link".to_string(),
            access_token: "1234".to_string(),
            item_id: "plaid-id-123".to_string(),
            state: crate::plaid::LinkStatus::Active,
            env: Environment::Development,
        };
        store.save_link(&link).await.unwrap();

        let mut updated_link = link.clone();
        updated_link.alias = "updated name".to_string();
        let result = store.update_link(&updated_link).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn can_save_transaction() {
        let mut store = test_store().await;
        let link = Link {
            alias: "test_link".to_string(),
            access_token: "1234".to_string(),
            item_id: "plaid-id-123".to_string(),
            state: crate::plaid::LinkStatus::Active,
            env: Environment::Development,
        };
        store.save_link(&link).await.unwrap();

        let transaction = Transaction {
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
            amount: 33.25,
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
        };

        let result = store.save_tx(&link.item_id, &transaction).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn saving_error_with_conflicting_key_returns_error() {
        let mut store = test_store().await;
        let link = Link {
            alias: "test_link".to_string(),
            access_token: "1234".to_string(),
            item_id: "plaid-id-123".to_string(),
            state: crate::plaid::LinkStatus::Active,
            env: Environment::Development,
        };
        store.save_link(&link).await.unwrap();

        let transaction = Transaction {
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
            amount: 33.25,
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
        };

        let result = store.save_tx(&link.item_id, &transaction).await;

        assert!(result.is_ok());

        let result = store
            .save_tx(&link.item_id, &transaction)
            .await
            .unwrap_err();
        assert!(matches!(result, Error::AlreadyExists));
    }

    #[tokio::test]
    async fn get_link_by_id() {
        let mut store = test_store().await;
        let link = Link {
            alias: "test_link".to_string(),
            access_token: "1234".to_string(),
            item_id: "plaid-id-123".to_string(),
            state: crate::plaid::LinkStatus::Active,
            env: Environment::Development,
        };
        store.save_link(&link).await.unwrap();

        let fetch_link = store.link(&link.item_id).await.unwrap();

        assert_eq!(&link.alias, &fetch_link.alias);
        assert_eq!(&link.access_token, &fetch_link.access_token);
        assert_eq!(&link.item_id, &fetch_link.item_id);
        assert!(matches!(link.state, crate::plaid::LinkStatus::Active));
        assert!(matches!(link.env, Environment::Development));
    }
}
