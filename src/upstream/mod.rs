pub mod plaid;

use anyhow::Result;
use axum::async_trait;
use serde::Serialize;

use crate::core::Transaction;
use rplaid::model::Account;

#[async_trait]
pub trait AccountSource {
    async fn accounts(&self) -> Result<Vec<Account>>;
}

pub struct TransactionEntry<T> {
    pub canonical: Transaction,
    pub source: T,
}

impl<T> TransactionEntry<T>
where
    T: Serialize,
{
    pub fn serialize_string(&self) -> Result<String> {
        Ok(serde_json::to_string(&self.source)?)
    }
}

#[async_trait]
pub trait TransactionSource<T: Serialize> {
    async fn transactions(&mut self) -> Result<Vec<TransactionEntry<T>>>;
}
