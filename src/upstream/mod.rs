pub mod plaid;

use anyhow::Result;
use axum::async_trait;
use chrono::NaiveDate;

use rplaid::model::{Account, Transaction};

#[async_trait]
pub trait AccountSource {
    async fn accounts(&self) -> Result<Vec<Account>>;
}

#[async_trait]
pub trait TransactionSource {
    async fn transactions(&self, start: NaiveDate, end: NaiveDate) -> Result<Vec<Transaction>>;
}
