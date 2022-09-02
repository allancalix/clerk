use anyhow::Result;
use axum::async_trait;
use chrono::NaiveDate;
use futures_util::pin_mut;
use futures_util::StreamExt;
use rplaid::client::Plaid;
use rplaid::model::{
    self, Account, SyncTransactionsRequest, SyncTransactionsRequestOptions, TransactionStream,
};

use crate::core::{Account as CoreAccount, Posting, Status, Transaction};
use crate::upstream::{AccountSource, TransactionEntry, TransactionSource};

pub struct Source<'a> {
    pub(crate) client: &'a Plaid,
    pub(crate) token: String,
    cursor: Option<String>,
}

impl<'a> Source<'a> {
    pub fn new(client: &'a Plaid, token: String, cursor: Option<String>) -> Self {
        Self {
            client,
            token,
            cursor,
        }
    }
}

#[async_trait]
impl<'a> AccountSource for Source<'a> {
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

impl<'a> Source<'a> {
    pub fn next_cursor(self) -> String {
        self.cursor
            .expect("must call transactions on source before checking cursor")
            .clone()
    }
}

#[async_trait]
impl<'a> TransactionSource<model::Transaction> for Source<'a> {
    async fn transactions(&mut self) -> Result<Vec<TransactionEntry<model::Transaction>>> {
        let tx_pages = self.client.transactions_sync_iter(SyncTransactionsRequest {
            access_token: self.token.clone(),
            cursor: self.cursor.clone(),
            count: Some(500),
            options: Some(SyncTransactionsRequestOptions {
                include_personal_finance_category: Some(true),
                include_original_description: Some(false),
            }),
        });
        pin_mut!(tx_pages);

        let tx_list = tx_pages
            .fold(vec![], |mut acc, x| async move {
                acc.append(&mut x.unwrap());
                acc
            })
            .await;

        if let Some(next_cursor) = tx_list.last() {
            assert!(matches!(next_cursor, TransactionStream::Done(_)));

            match next_cursor {
                TransactionStream::Done(cursor) => self.cursor = Some(cursor.clone()),
                _ => unreachable!(),
            }
        }

        let tx_list = tx_list
            .into_iter()
            .filter(|event| matches!(event, TransactionStream::Added(_)))
            .map(|event| match event {
                TransactionStream::Added(tx) => tx,
                _ => unreachable!(),
            })
            .collect::<Vec<model::Transaction>>();

        Ok(tx_list
            .into_iter()
            .map(|txn| TransactionEntry {
                canonical: to_canonical_txn(&txn).unwrap(),
                source: txn,
            })
            .collect())
    }
}
