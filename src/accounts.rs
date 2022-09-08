use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::prelude::*;

use anyhow::Result;
use clap::ArgMatches;
use futures_lite::stream::StreamExt;
use lazy_static::lazy_static;
use rplaid::model::*;
use rusty_money::{
    iso::{self, Currency},
    Money,
};
use tabwriter::TabWriter;

use crate::plaid::{default_plaid_client, Link};
use crate::settings::Settings;

lazy_static! {
    static ref ZERO_DOLLARS: Money<'static, Currency> = Money::from_minor(0_i64, iso::USD);
}

#[derive(Eq, PartialEq)]
struct AccountTypeWrapper(AccountType);

#[allow(clippy::derive_hash_xor_eq)]
impl Hash for AccountTypeWrapper {
    fn hash<H: Hasher>(&self, state: &mut H) {
        format!("{:?}", self.0).hash(state);
    }
}

impl PartialEq<AccountType> for AccountTypeWrapper {
    fn eq(&self, other: &AccountType) -> bool {
        self.0 == *other
    }
}

async fn print(settings: Settings) -> Result<()> {
    let link_controller =
        crate::plaid::LinkController::new(crate::store::SqliteStore::new(&settings.db_file).await?)
            .await?;

    let stdout = std::io::stdout().lock();

    link_controller.display_accounts_table(stdout)
}

async fn balances(settings: Settings) -> Result<()> {
    let mut store = crate::store::SqliteStore::new(&settings.db_file).await?;
    let plaid = default_plaid_client(&settings);

    let links: Vec<Link> = store.links().list().await?;

    let mut balances_by_type = HashMap::new();
    let mut futures = vec![];
    for link in links {
        futures.push(plaid.balances(link.access_token));
    }

    let results = futures_lite::stream::iter(futures)
        .then(|f| f)
        .collect::<Vec<_>>()
        .await;

    for result in results {
        for account in result? {
            balances_by_type
                .entry(AccountTypeWrapper(account.r#type))
                .or_insert(Vec::new())
                .push(account);
        }
    }

    let stdout = std::io::stdout().lock();
    let mut tw = TabWriter::new(stdout);

    writeln!(tw, "Assets")?;
    writeln!(tw, "Name\tAvailable\tCurrent")?;
    for (_k, v) in balances_by_type
        .iter()
        .filter(|(t, _)| *t == &AccountType::Depository)
    {
        for b in v {
            let currency_code = b
                .balances
                .iso_currency_code
                .as_deref()
                .and_then(iso::find)
                .unwrap_or(iso::USD);
            writeln!(
                tw,
                "{}\t{}\t{}",
                b.name,
                b.balances
                    .available
                    .map(|amount| { Money::from_decimal(amount, currency_code) })
                    .as_ref()
                    .unwrap_or(&ZERO_DOLLARS),
                b.balances
                    .current
                    .map(|amount| { Money::from_decimal(amount, currency_code) })
                    .as_ref()
                    .unwrap_or(&ZERO_DOLLARS),
            )?;
        }
    }

    writeln!(tw, "\nLiabililties")?;
    writeln!(tw, "Name\tAvailable\tCurrent")?;
    for (_k, v) in balances_by_type
        .iter()
        .filter(|(t, _)| *t == &AccountType::Credit)
    {
        for b in v {
            let currency_code = b
                .balances
                .iso_currency_code
                .as_deref()
                .and_then(iso::find)
                .unwrap_or(iso::USD);
            writeln!(
                tw,
                "{}\t{}\t{}",
                b.name,
                b.balances
                    .available
                    .map(|amount| { Money::from_decimal(amount, currency_code) })
                    .as_ref()
                    .unwrap_or(&ZERO_DOLLARS),
                b.balances
                    .current
                    .map(|amount| { Money::from_decimal(amount, currency_code) })
                    .as_ref()
                    .unwrap_or(&ZERO_DOLLARS),
            )?;
        }
    }

    Ok(())
}

pub(crate) async fn run(matches: &ArgMatches, settings: Settings) -> Result<()> {
    match matches.subcommand() {
        Some(("balances", _link_matches)) => balances(settings).await,
        None => print(settings).await,
        _ => unreachable!(),
    }
}
