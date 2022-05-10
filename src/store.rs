use std::sync::Arc;

use futures::TryStreamExt;
use rplaid::client::Environment;
use sqlx::{Connection, Row, SqliteConnection, SqlitePool};

use crate::plaid::Link;

type StoreResult<T> = Result<T, Box<dyn std::error::Error>>;

pub struct SqliteStore {
    conn: Arc<sqlx::pool::Pool<sqlx::sqlite::Sqlite>>,
}
impl SqliteStore {
    pub async fn new(uri: &str) -> StoreResult<Self> {
        let mut pool = sqlx::sqlite::SqlitePoolOptions::new().connect(uri).await?;

        let mut conn = pool.acquire().await?;
        sqlx::migrate!("./migrations").run(&mut conn).await?;

        Ok(Self { conn: Arc::new(pool) })
    }

    pub async fn update_link(&mut self, link: &Link) -> StoreResult<()> {
        sqlx::query("UPDATE plaid_links SET access_token = $1, link_state = $2, alias = $3 WHERE item_id = $4")
            .bind(&link.access_token)
            .bind("REQUIRES_VERIFICATION".to_string())
            .bind(&link.alias)
            .bind(&link.item_id)
            .execute(&mut self.conn.acquire().await?).await?;

        Ok(())

    }

    pub async fn links(&mut self) -> StoreResult<Vec<Link>> {
        let mut rows = sqlx::query(
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

    pub async fn save_link(&mut self, link: &Link) -> StoreResult<()> {
        sqlx::query("INSERT INTO plaid_links (item_id, alias, access_token, link_state, environment) VALUES ($1, $2, $3, $4, $5)")
            .bind(&link.item_id)
            .bind(&link.alias)
            .bind(&link.access_token)
            .bind("ACTIVE".to_string())
            .bind("DEVELOPMENT".to_string())
            .execute(&mut self.conn.acquire().await?).await?;

        Ok(())
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
}
