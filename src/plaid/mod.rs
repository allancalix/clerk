use std::io::Write;

use anyhow::{anyhow, Result};
use rplaid::client::{Builder, Credentials, Environment, Plaid};
use serde::{Deserialize, Serialize};
use tabwriter::TabWriter;

use crate::model::ConfigFile;
use crate::COUNTRY_CODES;

pub struct LinkController {
    connections: Vec<Connection>,
}

impl LinkController {
    pub async fn new(client: Plaid, links: Vec<Link>) -> Result<LinkController> {
        let mut connections = vec![];

        for link in links {
            let canonical = client.item(&link.access_token).await?;
            let state = match &canonical.error {
                Some(err) => {
                    let message = match &err.error_message {
                        Some(m) => m.into(),
                        None => "unexpected error with item".to_string(),
                    };
                    LinkStatus::Degraded(message)
                }
                None => LinkStatus::Active,
            };

            let ins: Result<rplaid::model::Institution> = match &canonical.institution_id {
                Some(id) => Ok(client
                    .get_institution_by_id(&rplaid::model::InstitutionGetRequest {
                        institution_id: id.as_str(),
                        country_codes: &COUNTRY_CODES,
                        options: None,
                    })
                    .await?),
                None => Err(anyhow!(
                    "no institutions associated with item {}",
                    link.item_id
                )),
            };

            let accounts = match link.state {
                LinkStatus::Active => client.accounts(&link.access_token).await?,
                _ => vec![],
            };

            connections.push(Connection {
                canonical,
                accounts,
                state,
                institution: ins?,
                alias: link.alias,
                access_token: link.access_token,
                item_id: link.item_id,
                env: link.env,
            });
        }

        Ok(LinkController { connections })
    }

    pub fn display_connections_table(&self) -> Result<String> {
        let mut tw = TabWriter::new(vec![]);
        writeln!(tw, "Name\tItem ID\tInstitution\tState")?;

        for conn in &self.connections {
            writeln!(
                tw,
                "{}\t{}\t{}\t{:?}",
                conn.alias, conn.item_id, conn.institution.name, conn.state
            )?;
        }

        Ok(String::from_utf8(tw.into_inner()?)?)
    }

    pub fn display_accounts_table(&self) -> Result<String> {
        let mut tw = TabWriter::new(vec![]);
        writeln!(tw, "Institution\tAccount\tAccount ID\tType\tStatus")?;

        for conn in &self.connections {
            for account in &conn.accounts {
                writeln!(
                    tw,
                    "{}\t{}\t{}\t{:?}\t{:?}",
                    conn.institution.name,
                    account.name,
                    account.account_id,
                    account.r#type,
                    conn.canonical.consent_expiration_time
                )?;
            }
        }

        Ok(String::from_utf8(tw.into_inner()?)?)
    }
}

pub(crate) fn default_plaid_client(conf: &ConfigFile) -> rplaid::client::Plaid {
    Builder::new()
        .with_credentials(Credentials {
            client_id: conf.config().plaid.client_id.clone(),
            secret: conf.config().plaid.secret.clone(),
        })
        .with_env(conf.config().plaid.env.clone())
        .build()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Link {
    pub alias: String,
    pub access_token: String,
    pub item_id: String,
    pub state: LinkStatus,
    pub sync_cursor: Option<String>,
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
    accounts: Vec<rplaid::model::Account>,
}
