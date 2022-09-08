use std::collections::HashMap;
use std::io::Write;

use anyhow::Result;
use rplaid::client::{Builder, Credentials, Plaid};
use tabwriter::TabWriter;
use tracing::{info, warn};

use crate::settings::Settings;
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

        let ins_cache: HashMap<String, String> = client
            .get_institutions(&rplaid::model::InstitutionsGetRequest {
                count: 500,
                offset: 0,
                country_codes: &COUNTRY_CODES,
                options: None,
            })
            .await?
            .into_iter()
            .map(|i| (i.institution_id, i.name))
            .collect();

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

            let accounts = store.accounts().by_item(&link.item_id).await?;

            connections.push(Connection {
                accounts,
                state: link.state.clone(),
                alias: link.alias,
                item_id: link.item_id,
                ins_name: ins_cache
                    .get(&link.institution_id.unwrap())
                    .unwrap()
                    .to_string(),
            });
        }

        Ok(LinkController { connections })
    }

    pub fn display_connections_table<T: std::io::Write>(&self, wr: T) -> Result<()> {
        let mut tw = TabWriter::new(wr);
        writeln!(tw, "Name\tItem ID\tInstitution\tState")?;

        for conn in &self.connections {
            writeln!(
                tw,
                "{}\t{}\t{}\t{:?}",
                conn.alias, conn.item_id, conn.ins_name, conn.state
            )?;
        }

        tw.flush()?;

        Ok(())
    }

    pub fn display_accounts_table<T: std::io::Write>(&self, wr: T) -> Result<()> {
        let mut tw = TabWriter::new(wr);
        writeln!(tw, "Institution\tAccount\tAccount ID\tType")?;

        for conn in &self.connections {
            for account in &conn.accounts {
                writeln!(
                    tw,
                    "{}\t{}\t{}\t{:?}",
                    conn.ins_name, account.name, account.id, account.ty,
                )?;
            }
        }

        tw.flush()?;

        Ok(())
    }
}

pub(crate) fn default_plaid_client(settings: &Settings) -> rplaid::client::Plaid {
    Builder::new()
        .with_credentials(Credentials {
            client_id: settings.plaid.client_id.clone(),
            secret: settings.plaid.secret.clone(),
        })
        .with_env(settings.plaid.env.clone())
        .build()
}

#[derive(Debug, Clone)]
pub struct Link {
    pub alias: String,
    pub access_token: String,
    pub item_id: String,
    pub state: LinkStatus,
    pub sync_cursor: Option<String>,
    pub institution_id: Option<String>,
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
    ins_name: String,
    accounts: Vec<crate::core::Account>,
}
