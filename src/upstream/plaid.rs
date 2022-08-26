use anyhow::Result;
use axum::async_trait;
use chrono::NaiveDate;
use futures_util::pin_mut;
use futures_util::StreamExt;
use rplaid::client::Plaid;
use rplaid::model::*;
use rplaid::HttpClient;

use crate::upstream::{AccountSource, TransactionSource};

pub struct Source<'a, T: HttpClient> {
    pub(crate) client: &'a Plaid<T>,
    pub(crate) token: String,
}

impl<'a, T: HttpClient> Source<'a, T> {
    pub fn new(client: &'a Plaid<T>, token: String) -> Self {
        Self { client, token }
    }
}

#[async_trait]
impl<'a, T: HttpClient> AccountSource for Source<'a, T> {
    async fn accounts(&self) -> Result<Vec<Account>> {
        Ok(self.client.accounts(&self.token).await?)
    }
}

#[async_trait]
impl<'a, T: HttpClient> TransactionSource for Source<'a, T> {
    async fn transactions(&self, start: NaiveDate, end: NaiveDate) -> Result<Vec<Transaction>> {
        let start = start.format("%Y-%m-%d").to_string();
        let end = end.format("%Y-%m-%d").to_string();

        let tx_pages = self.client.transactions_iter(GetTransactionsRequest {
            access_token: self.token.as_str(),
            start_date: &start,
            end_date: &end,
            options: Some(GetTransactionsOptions {
                count: Some(100),
                offset: Some(0),
                account_ids: None,
                include_original_description: None,
            }),
        });
        pin_mut!(tx_pages);

        let mut tx_list = vec![];
        while let Some(page) = tx_pages.next().await {
            tx_list.extend_from_slice(&page?);
        }

        Ok(tx_list)
    }
}
