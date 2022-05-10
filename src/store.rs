use std::sync::Arc;

use anyhow::Result;
use rplaid::{client::Environment, model::Transaction};
use sqlx::Row;

use crate::plaid::Link;

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

    pub async fn update_link(&mut self, link: &Link) -> Result<()> {
        sqlx::query("UPDATE plaid_links SET access_token = $1, link_state = $2, alias = $3 WHERE item_id = $4")
            .bind(&link.access_token)
            .bind("REQUIRES_VERIFICATION".to_string())
            .bind(&link.alias)
            .bind(&link.item_id)
            .execute(&mut self.conn.acquire().await?).await?;

        Ok(())
    }

    pub async fn links(&mut self) -> Result<Vec<Link>> {
        let rows = sqlx::query(
            "SELECT item_id, alias, access_token, link_state, environment FROM plaid_links",
        )
        .fetch_all(&mut self.conn.acquire().await?)
        .await?;

        let mut links = vec![];
        for row in rows {
            links.push(Link {
                item_id: row.try_get("item_id")?,
                alias: row.try_get("alias")?,
                access_token: row.try_get("access_token")?,
                env: Environment::Development,
                state: crate::plaid::LinkStatus::Active,
            });
        }

        Ok(links)
    }

    pub async fn save_link(&mut self, link: &Link) -> Result<()> {
        sqlx::query("INSERT INTO plaid_links (item_id, alias, access_token, link_state, environment) VALUES ($1, $2, $3, $4, $5)")
            .bind(&link.item_id)
            .bind(&link.alias)
            .bind(&link.access_token)
            .bind("ACTIVE".to_string())
            .bind("DEVELOPMENT".to_string())
            .execute(&mut self.conn.acquire().await?).await?;

        Ok(())
    }

    pub async fn delete_link(&mut self, id: &str) -> Result<Link> {
        let row = sqlx::query("DELETE FROM plaid_links WHERE item_id = $1 RETURNING item_id, alias, access_token, link_state, environment")
            .bind(id)
            .fetch_one(&mut self.conn.acquire().await?).await?;

        Ok(Link {
            item_id: row.try_get("item_id")?,
            alias: row.try_get("alias")?,
            access_token: row.try_get("access_token")?,
            env: Environment::Development,
            state: crate::plaid::LinkStatus::Active,
        })
    }

    pub async fn save_tx(&mut self, item_id: &str, tx: &Transaction) -> Result<()> {
        let json = serde_json::to_string(&tx)?;

        sqlx::query(
            "INSERT INTO transactions (item_id, transaction_id, payload) values($1, $2, $3)",
        )
        .bind(item_id)
        .bind(&tx.transaction_id)
        .bind(json.as_bytes())
        .execute(&mut self.conn.acquire().await?)
        .await?;

        Ok(())
    }

    pub async fn transactions(&mut self) -> Result<Vec<Transaction>> {
        let rows = sqlx::query("SELECT item_id, payload FROM transactions")
            .fetch_all(&mut self.conn.acquire().await?)
            .await?;

        let mut txs = vec![];
        for row in rows {
            let buf: Vec<u8> = row.try_get("payload")?;
            txs.push(serde_json::from_slice(&buf)?);
        }

        Ok(txs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_store() -> SqliteStore {
        SqliteStore::new("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn save_plaid_link_to_table() {
        let mut store = test_store().await;

        let link = Link {
            alias: "test_link".to_string(),
            access_token: "1234".to_string(),
            item_id: "plaid-id-123".to_string(),
            state: crate::plaid::LinkStatus::Active,
            env: Environment::Development,
        };
        let result = store.save_link(&link).await;

        assert!(result.is_ok())
    }

    #[tokio::test]
    async fn list_plaid_links_to_table() {
        let mut store = test_store().await;
        let link = Link {
            alias: "test_link".to_string(),
            access_token: "1234".to_string(),
            item_id: "plaid-id-123".to_string(),
            state: crate::plaid::LinkStatus::Active,
            env: Environment::Development,
        };
        store.save_link(&link).await.unwrap();
        store.save_link(&link).await.unwrap();

        let links = store.links().await.unwrap();

        assert_eq!(links.len(), 2);
    }

    #[tokio::test]
    async fn update_plaid_link() {
        let mut store = test_store().await;
        let link = Link {
            alias: "test_link".to_string(),
            access_token: "1234".to_string(),
            item_id: "plaid-id-123".to_string(),
            state: crate::plaid::LinkStatus::Active,
            env: Environment::Development,
        };
        store.save_link(&link).await.unwrap();

        let mut updated_link = link.clone();
        updated_link.alias = "updated name".to_string();
        let result = store.update_link(&updated_link).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn can_save_transaction() {
        let mut store = test_store().await;
        let link = Link {
            alias: "test_link".to_string(),
            access_token: "1234".to_string(),
            item_id: "plaid-id-123".to_string(),
            state: crate::plaid::LinkStatus::Active,
            env: Environment::Development,
        };
        store.save_link(&link).await.unwrap();

        let transaction = Transaction {
            transaction_type: "".to_string(),
            pending_transaction_id: None,
            category_id: None,
            category: None,
            location: None,
            payment_meta: None,
            account_owner: None,
            name: "".to_string(),
            original_description: None,
            account_id: "".to_string(),
            amount: 33.25,
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
        };

        let result = store.save_tx(&link.item_id, &transaction).await;

        assert!(result.is_ok());
    }
}
