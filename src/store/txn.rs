use sea_query::{func::Func, types::Alias, Expr, Iden, Query, SqliteQueryBuilder};
use serde::Serialize;
use sqlx::{Connection, Row};

use super::{bind_query, Result, SqliteStore, TransactionEntry};

#[derive(Iden)]
enum Transactions {
    Table,
    Id,
    AccountId,
    Source,
}

struct JsonExtract;

impl Iden for JsonExtract {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        write!(s, "JSON_EXTRACT").unwrap();
    }
}

pub struct Store<'a>(&'a mut SqliteStore);

impl<'a> Store<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self(store)
    }

    pub async fn by_upstream_id(&mut self, id: &str) -> Result<Option<String>> {
        #[derive(Iden)]
        enum TransactionsLocal {
            UpstreamId,
        }

        let (query, values) = Query::select()
            .expr_as(
                Func::cust(JsonExtract).args(vec![
                    Expr::col(Transactions::Source),
                    Expr::val("$.transaction_id"),
                ]),
                Alias::new(&TransactionsLocal::UpstreamId.to_string()),
            )
            .columns([Transactions::Id])
            .from(Transactions::Table)
            .and_where(Expr::col(TransactionsLocal::UpstreamId).eq(id))
            .build(SqliteQueryBuilder);

        Ok(bind_query(sqlx::query(&query), &values)
            .fetch_optional(&mut self.0.conn.acquire().await?)
            .await?
            .map(|row| row.try_get("id").unwrap()))
    }

    pub async fn update_source<S: Serialize>(&mut self, id: &str, source: S) -> Result<()> {
        let (query, values) = Query::update()
            .table(Transactions::Table)
            .values(vec![(
                Transactions::Source,
                serde_json::to_string(&source)?.into(),
            )])
            .and_where(Expr::col(Transactions::Id).eq(id))
            .build(SqliteQueryBuilder);

        bind_query(sqlx::query(&query), &values)
            .execute(&mut self.0.conn.acquire().await?)
            .await?;

        Ok(())
    }

    pub async fn delete(&mut self, id: &str) -> Result<()> {
        let (query, values) = Query::delete()
            .from_table(Transactions::Table)
            .and_where(Expr::col(Transactions::Id).eq(id))
            .build(SqliteQueryBuilder);

        bind_query(sqlx::query(&query), &values)
            .execute(&mut self.0.conn.acquire().await?)
            .await?;

        Ok(())
    }

    pub async fn save<S: Serialize>(
        &mut self,
        account_id: &str,
        tx: &TransactionEntry<S>,
    ) -> Result<()> {
        let source = tx.serialize_string()?;
        let canonical = tx.canonical.clone();
        let account_id = account_id.to_string();

        self.0
            .conn
            .acquire()
            .await?
            .transaction(|conn| {
                Box::pin(async move {
                    let (query, values) = Query::insert()
                        .into_table(Transactions::Table)
                        .columns([
                            Transactions::Id,
                            Transactions::AccountId,
                            Transactions::Source,
                        ])
                        .values_panic(vec![
                            canonical.id.to_string().into(),
                            account_id.into(),
                            source.into(),
                        ])
                        .build(SqliteQueryBuilder);

                    bind_query(sqlx::query(&query), &values)
                        .execute(conn)
                        .await?;

                    Ok(())
                })
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use rplaid::model::Transaction as PlaidTransaction;
    use ulid::Ulid;

    use crate::core::{Account, Status, Transaction};
    use crate::plaid::Link;

    use super::*;

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
            account_id: "test-account-id".to_string(),
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
    async fn save_transaction() {
        let mut store = test_store().await;
        let link = Link {
            institution_id: Some("10".to_string()),
            alias: "test_link".to_string(),
            access_token: "1234".to_string(),
            item_id: "plaid-id-123".to_string(),
            state: crate::plaid::LinkStatus::Active,
            sync_cursor: None,
        };
        store.links().save(&link).await.unwrap();
        store
            .accounts()
            .save(
                &link.item_id,
                &Account {
                    id: "test-account-id".into(),
                    ty: "CREDIT_NORMAL".into(),
                    name: "Test Account".into(),
                },
            )
            .await
            .unwrap();

        let entry = TransactionEntry {
            canonical: Transaction {
                id: Ulid::new(),
                date: NaiveDate::parse_from_str("2022-05-01", "%Y-%m-%d").unwrap(),
                narration: "Test Transaction".to_string(),
                payee: None,
                status: Status::Resolved,
            },
            source: plaid_transaction(),
        };

        store.txns().save("test-account-id", &entry).await.unwrap();
    }

    #[tokio::test]
    async fn delete() {
        let mut store = test_store().await;
        let link = Link {
            institution_id: Some("10".to_string()),
            alias: "test_link".to_string(),
            access_token: "1234".to_string(),
            item_id: "plaid-id-123".to_string(),
            state: crate::plaid::LinkStatus::Active,
            sync_cursor: None,
        };
        store.links().save(&link).await.unwrap();
        store
            .accounts()
            .save(
                &link.item_id,
                &Account {
                    id: "test-account-id".into(),
                    ty: "CREDIT_NORMAL".into(),
                    name: "Test Account".into(),
                },
            )
            .await
            .unwrap();

        let txn_id = Ulid::new();
        let entry = TransactionEntry {
            canonical: Transaction {
                id: txn_id.clone(),
                date: NaiveDate::parse_from_str("2022-05-01", "%Y-%m-%d").unwrap(),
                narration: "Test Transaction".to_string(),
                payee: None,
                status: Status::Resolved,
            },
            source: plaid_transaction(),
        };

        store.txns().save("test-account-id", &entry).await.unwrap();

        store
            .txns()
            .delete(txn_id.to_string().as_str())
            .await
            .unwrap();
    }
}
