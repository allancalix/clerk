use anyhow::Result;
use axum::async_trait;
use chrono::NaiveDate;
use futures_util::pin_mut;
use futures_util::StreamExt;
use rplaid::client::Plaid;
use rplaid::model::{self, Account, GetTransactionsOptions, GetTransactionsRequest};
use rplaid::HttpClient;

use crate::core::{Account as CoreAccount, Posting, Status, Transaction};
use crate::upstream::{AccountSource, TransactionEntry, TransactionSource};

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

fn to_canonical_txn(tx: &model::Transaction) -> Result<Transaction> {
    let currency = tx
        .iso_currency_code
        .as_ref()
        .and_then(|c| rusty_money::iso::find(c))
        .unwrap_or(rusty_money::iso::USD);

    let amount = rusty_money::Money::from_str(&tx.amount.to_string(), currency).unwrap();
    let source_posting = Posting {
        units: amount.clone(),
        account: CoreAccount(tx.account_id.to_string()),
        status: Status::Resolved,
        meta: Default::default(),
    };

    let double = amount.clone() * 2;
    let balance_amount = if amount.is_negative() {
        amount + double
    } else {
        amount - double
    };
    let dest_posting = Posting {
        units: balance_amount,
        account: CoreAccount("Expenses:Unknown".to_string()),
        status: Status::Resolved,
        meta: Default::default(),
    };

    Ok(Transaction {
        id: ulid::Ulid::new(),
        date: NaiveDate::parse_from_str(&tx.date, "%Y-%m-%d").unwrap(),
        narration: tx.name.clone(),
        postings: vec![source_posting, dest_posting],
        status: if tx.pending {
            Status::Pending
        } else {
            Status::Resolved
        },
        payee: tx.merchant_name.clone(),
        tags: Default::default(),
        links: Default::default(),
        meta: Default::default(),
    })
}

#[async_trait]
impl<'a, T: HttpClient> TransactionSource<model::Transaction> for Source<'a, T> {
    async fn transactions(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<TransactionEntry<model::Transaction>>> {
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

        Ok(tx_list
            .into_iter()
            .map(|txn| TransactionEntry {
                canonical: to_canonical_txn(&txn).unwrap(),
                source: txn,
            })
            .collect())
    }
}
