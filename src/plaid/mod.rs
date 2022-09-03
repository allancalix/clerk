use std::io::Write;

use anyhow::{anyhow, Result};
use rplaid::client::{Builder, Credentials, Plaid};
use tabwriter::TabWriter;
use tracing::{info, warn};

use crate::model::ConfigFile;
use crate::COUNTRY_CODES;

pub struct LinkController {
    connections: Vec<Connection>,
}

impl LinkController {
    pub async fn new(
        client: Plaid,
        mut store: crate::store::SqliteStore,
    ) -> Result<LinkController> {
        let mut connections = vec![];
        let links = store.links().list().await?;

        for mut link in links {
            let canonical = client.item(&link.access_token).await?;

            if let Some(e) = &canonical.error {
                if let Some("ITEM_LOGIN_REQUIRED") = &e.error_code.as_deref() {
                    info!("Link: {} failed with status {:?}", link.item_id, e);

                    link.state =
                        LinkStatus::Degraded(e.error_message.as_ref().unwrap().to_string());

                    store.links().update(&link).await?;

                    continue;
                }

                warn!("Unexpected link error. id={}", link.item_id);
            }

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
                LinkStatus::Active => store.accounts().by_item(&link.item_id).await?,
                _ => vec![],
            };

            connections.push(Connection {
                canonical,
                accounts,
                state: link.state.clone(),
                institution: ins?,
                alias: link.alias,
                item_id: link.item_id,
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
                    account.id,
                    account.ty,
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

#[derive(Debug, Clone)]
pub struct Link {
    pub alias: String,
    pub access_token: String,
    pub item_id: String,
    pub state: LinkStatus,
    pub sync_cursor: Option<String>,
}

#[derive(Debug, Clone)]
pub enum LinkStatus {
    Active,
    Degraded(String),
}

#[derive(Debug)]
struct Connection {
    alias: String,
    item_id: String,
    state: LinkStatus,

    canonical: rplaid::model::Item,
    institution: rplaid::model::Institution,
    accounts: Vec<crate::core::Account>,
}
