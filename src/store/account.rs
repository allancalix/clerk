use sea_query::{Expr, Iden, Query, SqliteQueryBuilder};
sea_query::sea_query_driver_sqlite!();
use sea_query_driver_sqlite::bind_query;
use sqlx::Row;

use crate::core::Account;

use super::{Result, SqliteStore};

#[derive(Iden)]
enum Accounts {
    Table,
    Id,
    ItemId,
    Name,
    Type,
}

pub struct Store<'a>(&'a mut SqliteStore);

impl<'a> Store<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self(store)
    }

    pub async fn by_id(&mut self, id: &str) -> Result<Option<Account>> {
        let (query, values) = Query::select()
            .from(Accounts::Table)
            .columns([Accounts::Id, Accounts::Name, Accounts::Type])
            .and_where(Expr::col(Accounts::Id).eq(id))
            .build(SqliteQueryBuilder);

        Ok(bind_query(sqlx::query(&query), &values)
            .fetch_optional(&mut self.0.conn.acquire().await?)
            .await?
            .map(|row| Account {
                id: row.try_get("id").unwrap(),
                name: row.try_get("name").unwrap(),
                ty: row.try_get("type").unwrap(),
            }))
    }

    pub async fn by_item(&mut self, id: &str) -> Result<Vec<Account>> {
        let (query, values) = Query::select()
            .from(Accounts::Table)
            .columns([Accounts::Id, Accounts::Name, Accounts::Type])
            .and_where(Expr::col(Accounts::ItemId).eq(id))
            .build(SqliteQueryBuilder);

        let rows = bind_query(sqlx::query(&query), &values)
            .fetch_all(&mut self.0.conn.acquire().await?)
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| Account {
                id: row.try_get("id").unwrap(),
                name: row.try_get("name").unwrap(),
                ty: row.try_get("type").unwrap(),
            })
            .collect())
    }

    pub async fn save(&mut self, item_id: &str, account: &Account) -> Result<()> {
        let (query, values) = Query::insert()
            .into_table(Accounts::Table)
            .columns([
                Accounts::Id,
                Accounts::ItemId,
                Accounts::Name,
                Accounts::Type,
            ])
            .values_panic(vec![
                account.id.as_str().into(),
                item_id.into(),
                account.name.as_str().into(),
                account.ty.as_str().into(),
            ])
            .build(SqliteQueryBuilder);

        bind_query(sqlx::query(&query), &values)
            .execute(&mut self.0.conn.acquire().await?)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rplaid::model::{Account, AccountType, Balance};

    use crate::store::link::tests::TestStore;

    #[tokio::test]
    async fn get_account() {
        let mut store = TestStore::new().await;
        let link = store.new_link().await;

        store
            .db()
            .accounts()
            .save(
                &link.item_id,
                &Account {
                    account_id: "account-id".into(),
                    name: "Test Account".into(),
                    r#type: AccountType::Credit,
                    official_name: None,
                    verification_status: None,
                    subtype: None,
                    mask: None,
                    balances: Balance {
                        available: None,
                        current: None,
                        iso_currency_code: None,
                        limit: None,
                        unofficial_currency_code: None,
                    },
                }
                .into(),
            )
            .await
            .unwrap();

        let account = store
            .db()
            .accounts()
            .by_id("account-id")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&account.name, "Test Account");
    }
}
