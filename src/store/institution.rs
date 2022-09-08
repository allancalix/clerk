use sea_query::{Iden, Query, SqliteQueryBuilder};
use sqlx::{FromRow, Row};

use super::{bind_query, Result, SqliteStore};

#[derive(Iden)]
enum Institutions {
    Table,
    Id,
    Name,
}

pub struct Institution {
    pub id: String,
    pub name: String,
}

impl<'r, R: sqlx::Row> sqlx::FromRow<'r, R> for Institution
where
    std::string::String: sqlx::Decode<'r, <R as Row>::Database> + sqlx::Type<<R as Row>::Database>,
    &'r str: sqlx::Decode<'r, <R as Row>::Database> + sqlx::Type<<R as Row>::Database>,
    &'static str: sqlx::ColumnIndex<R>,
{
    fn from_row(row: &'r R) -> ::std::result::Result<Self, sqlx::Error> {
        Ok(Institution {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
        })
    }
}

pub struct Store<'a>(&'a mut SqliteStore);

impl<'a> Store<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self(store)
    }

    pub async fn list(&mut self) -> Result<Vec<Institution>> {
        let (query, values) = Query::select()
            .columns([Institutions::Id, Institutions::Name])
            .from(Institutions::Table)
            .build(SqliteQueryBuilder);

        let rows = bind_query(sqlx::query(&query), &values)
            .fetch_all(&mut self.0.conn.acquire().await?)
            .await?;

        let mut institutions = Vec::with_capacity(rows.len());
        for row in rows {
            institutions.push(Institution::from_row(&row)?);
        }

        Ok(institutions)
    }

    pub async fn save(&mut self, ins: &Institution) -> Result<()> {
        let (query, values) = Query::insert()
            .into_table(Institutions::Table)
            .columns([Institutions::Id, Institutions::Name])
            .values_panic(vec![ins.id.as_str().into(), ins.name.as_str().into()])
            .build(SqliteQueryBuilder);

        bind_query(sqlx::query(&query), &values)
            .execute(&mut self.0.conn.acquire().await?)
            .await?;

        Ok(())
    }
}
