use std::io::Write;

use tabwriter::TabWriter;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use rplaid::client::{Environment, Plaid};
use rplaid::HttpClient;

use crate::COUNTRY_CODES;

pub struct LinkController<T: HttpClient> {
    client: Plaid<T>,
    connections: Vec<Connection>,
}

impl<T: HttpClient> LinkController<T> {
    pub async fn new(client: Plaid<T>, links: Vec<Link>) -> Result<LinkController<T>> {
        let mut connections = vec!();

        for link in links {
            let canonical = client.item(link.access_token.clone()).await?;
            let state = match &canonical.error {
                Some(err) => {
                    let message = match &err.error_message {
                        Some(m) => m.into(),
                        None => "unexpected error with item".to_string(),
                    };
                    LinkStatus::Degraded(message)
                },
                None => LinkStatus::Active,
            };

            let ins: Result<rplaid::model::Institution> = match &canonical.institution_id {
                Some(id) => {
                    Ok(client
                        .get_institution_by_id(&rplaid::model::InstitutionGetRequest {
                            institution_id: id.as_str(),
                            country_codes: &COUNTRY_CODES,
                            options: None,
                        })
                    .await?)
                },
                None => Err(anyhow!("no institutions associated with item {}", link.item_id)),
            };

            connections.push(Connection {
                canonical,
                state,
                institution: ins?,
                alias: link.alias,
                access_token: link.access_token,
                item_id: link.item_id,
                env: link.env,
            });
        }

        Ok(LinkController {
            client,
            connections,
        })
    }

    pub fn get_access_token_by_item_id(&self, id: &str) -> Option<String> {
        self.connections
            .iter()
            .filter(|connection| connection.item_id == id)
            .next()
            .map(|item| item.access_token.to_string())
    }

    pub fn display_connections_table(&self) -> Result<String> {
        let mut tw = TabWriter::new(vec![]);
        writeln!(tw, "Name\tItem ID\tInstitution\tState")?;

        for conn in &self.connections {
            writeln!(tw, "{}\t{}\t{}\t{:?}", conn.alias, conn.item_id, conn.institution.name, conn.state)?;
        }

        Ok(String::from_utf8(tw.into_inner()?)?)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Link {
    pub alias: String,
    pub access_token: String,
    pub item_id: String,
    pub state: LinkStatus,
    pub env: Environment,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LinkStatus {
    Active,
    Degraded(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct Connection {
    alias: String,
    access_token: String,
    item_id: String,
    state: LinkStatus,
    env: Environment,

    canonical: rplaid::model::Item,
    institution: rplaid::model::Institution,
    // accounts: Vec<rplaid::model::Account>,
}

#[cfg(test)]
mod tests {
    fn its_ok() {}
}
