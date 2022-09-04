use anyhow::Result;
use axum::async_trait;
use chrono::NaiveDate;
use futures_lite::{pin, stream::StreamExt};
use rplaid::client::Plaid;
use rplaid::model::{
    self, Account, SyncTransactionsRequest, SyncTransactionsRequestOptions, TransactionStream,
};

use crate::core::{Status, Transaction};
use crate::upstream::{AccountSource, TransactionEntry, TransactionEvent, TransactionSource};

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
    Ok(Transaction {
        id: ulid::Ulid::new(),
        date: NaiveDate::parse_from_str(&tx.date, "%Y-%m-%d").unwrap(),
        narration: tx.name.clone(),
        status: if tx.pending {
            Status::Pending
        } else {
            Status::Resolved
        },
        payee: tx.merchant_name.clone(),
    })
}

impl<'a> Source<'a> {
    pub fn next_cursor(self) -> String {
        self.cursor
            .expect("must call transactions on source before checking cursor")
    }
}

type PlaidTransactionEvent = TransactionEvent<model::Transaction>;

#[async_trait]
impl<'a> TransactionSource<model::Transaction> for Source<'a> {
    async fn transactions(&mut self) -> Result<Vec<PlaidTransactionEvent>> {
        let tx_pages = self.client.transactions_sync_iter(SyncTransactionsRequest {
            access_token: self.token.clone(),
            cursor: self.cursor.clone(),
            count: Some(500),
            options: Some(SyncTransactionsRequestOptions {
                include_personal_finance_category: Some(true),
                include_original_description: Some(false),
            }),
        });
        pin!(tx_pages);

        let mut tx_list = vec![];
        while let Some(txn_page) = tx_pages.next().await {
            tx_list.extend(txn_page?);
        }

        if let Some(next_cursor) = tx_list.last() {
            assert!(matches!(next_cursor, TransactionStream::Done(_)));

            match next_cursor {
                TransactionStream::Done(cursor) => self.cursor = Some(cursor.clone()),
                _ => unreachable!(),
            }
        }

        Ok(tx_list
            .into_iter()
            .filter_map(|e| match e {
                TransactionStream::Added(txn) => {
                    let entry = PlaidTransactionEvent::Added(TransactionEntry {
                        canonical: to_canonical_txn(&txn).unwrap(),
                        source: txn,
                    });

                    Some(entry)
                }
                TransactionStream::Modified(txn) => {
                    let entry = PlaidTransactionEvent::Modified(TransactionEntry {
                        canonical: to_canonical_txn(&txn).unwrap(),
                        source: txn,
                    });

                    Some(entry)
                }
                TransactionStream::Removed(id) => Some(PlaidTransactionEvent::Removed(id)),
                TransactionStream::Done(cursor) => {
                    self.cursor = Some(cursor);

                    None
                }
            })
            .collect::<Vec<PlaidTransactionEvent>>())
    }
}
