use sea_query::{Expr, Iden, Query, SqliteQueryBuilder};
use sqlx::{FromRow, Row};

use super::{bind_query, Result, SqliteStore};
use crate::plaid::{Link, LinkStatus};

#[derive(Iden)]
enum PlaidLinks {
    Table,
    Id,
    Alias,
    AccessToken,
    LinkState,
    SyncCursor,
    Institution,
}

pub struct Store<'a>(&'a mut SqliteStore);

impl<'a> Store<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self(store)
    }

    pub async fn update(&mut self, link: &Link) -> Result<()> {
        let (query, values) = Query::update()
            .table(PlaidLinks::Table)
            .values(vec![
                (PlaidLinks::Alias, link.alias.as_str().into()),
                (PlaidLinks::AccessToken, link.access_token.as_str().into()),
                (PlaidLinks::LinkState, to_status_enum(&link.state).into()),
                (PlaidLinks::SyncCursor, link.sync_cursor.as_deref().into()),
                (
                    PlaidLinks::Institution,
                    link.institution_id.as_deref().into(),
                ),
            ])
            .and_where(Expr::col(PlaidLinks::Id).eq(link.item_id.as_str()))
            .build(SqliteQueryBuilder);

        bind_query(sqlx::query(&query), &values)
            .execute(&mut self.0.conn.acquire().await?)
            .await?;

        Ok(())
    }

    pub async fn link(&mut self, id: &str) -> Result<Link> {
        let (query, values) = Query::select()
            .columns([
                PlaidLinks::Id,
                PlaidLinks::Alias,
                PlaidLinks::AccessToken,
                PlaidLinks::LinkState,
                PlaidLinks::SyncCursor,
                PlaidLinks::Institution,
            ])
            .from(PlaidLinks::Table)
            .and_where(Expr::col(PlaidLinks::Id).eq(id))
            .build(SqliteQueryBuilder);

        let row = bind_query(sqlx::query(&query), &values)
            .fetch_one(&mut self.0.conn.acquire().await?)
            .await?;

        Ok(Link::from_row(&row)?)
    }

    pub async fn list(&mut self) -> Result<Vec<Link>> {
        let (query, values) = Query::select()
            .columns([
                PlaidLinks::Id,
                PlaidLinks::Alias,
                PlaidLinks::AccessToken,
                PlaidLinks::LinkState,
                PlaidLinks::SyncCursor,
                PlaidLinks::Institution,
            ])
            .from(PlaidLinks::Table)
            .build(SqliteQueryBuilder);

        let rows = bind_query(sqlx::query(&query), &values)
            .fetch_all(&mut self.0.conn.acquire().await?)
            .await?;

        let mut links = vec![];
        for row in rows {
            links.push(Link::from_row(&row)?);
        }

        Ok(links)
    }

    pub async fn save(&mut self, link: &Link) -> Result<()> {
        let (query, values) = Query::insert()
            .into_table(PlaidLinks::Table)
            .columns([
                PlaidLinks::Id,
                PlaidLinks::Alias,
                PlaidLinks::AccessToken,
                PlaidLinks::LinkState,
                PlaidLinks::Institution,
            ])
            .values_panic(vec![
                link.item_id.as_str().into(),
                link.alias.as_str().into(),
                link.access_token.as_str().into(),
                to_status_enum(&link.state).as_str().into(),
                link.institution_id.as_deref().into(),
            ])
            .build(SqliteQueryBuilder);

        bind_query(sqlx::query(&query), &values)
            .execute(&mut self.0.conn.acquire().await?)
            .await?;

        Ok(())
    }

    pub async fn delete(&mut self, id: &str) -> Result<Link> {
        let (query, values) = Query::delete()
            .from_table(PlaidLinks::Table)
            .and_where(Expr::col(PlaidLinks::Id).eq(id))
            .returning(Query::returning().columns([
                PlaidLinks::Id,
                PlaidLinks::Alias,
                PlaidLinks::AccessToken,
                PlaidLinks::LinkState,
                PlaidLinks::Institution,
            ]))
            .build(SqliteQueryBuilder);

        let row = bind_query(sqlx::query(&query), &values)
            .fetch_one(&mut self.0.conn.acquire().await?)
            .await?;

        Ok(Link::from_row(&row)?)
    }
}

impl<'r, R: sqlx::Row> sqlx::FromRow<'r, R> for Link
where
    std::string::String: sqlx::Decode<'r, <R as Row>::Database> + sqlx::Type<<R as Row>::Database>,
    &'r str: sqlx::Decode<'r, <R as Row>::Database> + sqlx::Type<<R as Row>::Database>,
    &'static str: sqlx::ColumnIndex<R>,
{
    fn from_row(row: &'r R) -> ::std::result::Result<Self, sqlx::Error> {
        Ok(Link {
            item_id: row.try_get("id")?,
            alias: row.try_get("alias")?,
            access_token: row.try_get("access_token")?,
            state: from_status_enum(row.try_get("link_state")?).unwrap(),
            sync_cursor: row.try_get("sync_cursor")?,
            institution_id: row.try_get("institution")?,
        })
    }
}

fn to_status_enum(status: &LinkStatus) -> String {
    match *status {
        LinkStatus::Degraded(_) => "REQUIRES_VERIFICATION".into(),
        LinkStatus::Active => "ACTIVE".into(),
    }
}

fn from_status_enum(status: &str) -> anyhow::Result<LinkStatus> {
    match status {
        "ACTIVE" => Ok(LinkStatus::Active),
        "REQUIRES_VERIFICATION" => Ok(LinkStatus::Degraded("requires verification".to_string())),
        s => Err(anyhow::anyhow!("unknown status {}", s)),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use ulid::Ulid;

    use crate::plaid::Link;

    use super::SqliteStore;

    pub(crate) struct TestStore {
        store: SqliteStore,
    }

    impl TestStore {
        pub(crate) async fn new() -> Self {
            TestStore {
                store: SqliteStore::new("sqlite::memory:").await.unwrap(),
            }
        }

        pub(crate) async fn new_link(&mut self) -> Link {
            let link = Link {
                alias: "test_link".to_string(),
                access_token: "access-token-1234".to_string(),
                item_id: Ulid::new().to_string(),
                state: crate::plaid::LinkStatus::Active,
                sync_cursor: None,
                institution_id: None,
            };

            self.store.links().save(&link).await.unwrap();

            link
        }

        pub(crate) fn db(&mut self) -> &mut SqliteStore {
            &mut self.store
        }
    }

    async fn test_store() -> TestStore {
        TestStore {
            store: SqliteStore::new("sqlite::memory:").await.unwrap(),
        }
    }

    #[tokio::test]
    async fn retrieve_link() {
        let mut store = test_store().await;
        let link = store.new_link().await;

        let fetch_link = store.db().links().link(&link.item_id).await.unwrap();

        assert_eq!(&link.alias, &fetch_link.alias);
        assert_eq!(&link.access_token, &fetch_link.access_token);
        assert_eq!(&link.item_id, &fetch_link.item_id);
        assert!(matches!(link.state, crate::plaid::LinkStatus::Active));
    }

    #[tokio::test]
    async fn list_links() {
        let mut store = test_store().await;
        for _ in 0..5 {
            store.new_link().await;
        }

        let links = store.db().links().list().await.unwrap();

        assert_eq!(links.len(), 5);
    }

    #[tokio::test]
    async fn update_plaid_link() {
        let mut store = test_store().await;
        let link = store.new_link().await;

        let updated_link = Link {
            alias: "updated name".into(),
            ..link
        };
        store.db().links().update(&updated_link).await.unwrap();
    }
}
