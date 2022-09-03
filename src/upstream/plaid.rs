use std::collections::HashSet;

use anyhow::Result;
use axum::async_trait;
use chrono::NaiveDate;
use futures_util::pin_mut;
use futures_util::StreamExt;
use rplaid::client::Plaid;
use rplaid::model::{
    self, Account, SyncTransactionsRequest, SyncTransactionsRequestOptions, TransactionStream,
};

use crate::core::{Status, Transaction};
use crate::upstream::{AccountSource, TransactionEntry, TransactionSource};

pub struct Source<'a> {
    pub(crate) client: &'a Plaid,
    pub(crate) token: String,
    cursor: Option<String>,
    removed: HashSet<String>,
    modified: HashSet<String>,
}

impl<'a> Source<'a> {
    pub fn new(client: &'a Plaid, token: String, cursor: Option<String>) -> Self {
        Self {
            client,
            token,
            cursor,
            removed: Default::default(),
            modified: Default::default(),
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
        tags: Default::default(),
        links: Default::default(),
        meta: Default::default(),
    })
}

impl<'a> Source<'a> {
    pub fn next_cursor(self) -> String {
        self.cursor
            .expect("must call transactions on source before checking cursor")
    }

    pub fn removed(&self) -> &HashSet<String> {
        &self.removed
    }

    pub fn modified(&self) -> &HashSet<String> {
        &self.modified
    }

    fn remove(&mut self, id: &str) {
        self.removed.insert(id.to_string());
    }

    fn modify(&mut self, id: &str) {
        self.modified.insert(id.to_string());
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

        Ok(tx_list
            .into_iter()
            .filter_map(|e| match e {
                TransactionStream::Added(txn) => {
                    let entry = TransactionEntry {
                        canonical: to_canonical_txn(&txn).unwrap(),
                        source: txn,
                    };

                    Some(entry)
                }
                TransactionStream::Modified(txn) => {
                    self.modify(&txn.transaction_id);

                    let entry = TransactionEntry {
                        canonical: to_canonical_txn(&txn).unwrap(),
                        source: txn,
                    };

                    Some(entry)
                }
                TransactionStream::Removed(id) => {
                    self.remove(&id);

                    None
                }
                TransactionStream::Done(cursor) => {
                    self.cursor = Some(cursor);

                    None
                }
            })
            .collect::<Vec<TransactionEntry<model::Transaction>>>())
    }
}
