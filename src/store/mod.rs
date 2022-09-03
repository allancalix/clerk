mod account;
pub(crate) mod link;
mod txn;

use std::sync::Arc;

use thiserror::Error;

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
    Database(#[from] sqlx::Error),
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

    pub fn links(&mut self) -> link::Store {
        link::Store::new(self)
    }

    pub fn txns(&mut self) -> txn::Store {
        txn::Store::new(self)
    }

    pub fn accounts(&mut self) -> account::Store {
        account::Store::new(self)
    }
}
