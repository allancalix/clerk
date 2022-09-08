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

    let mut futures = vec![];
    for link in links {
        futures.push(plaid.balances(link.access_token));
    }

    let results = futures_lite::stream::iter(futures)
        .then(|f| f)
        .collect::<Vec<_>>()
        .await;

    let mut accounts = vec![];
    for result in results {
        for account in result? {
            accounts.push(account);
        }
    }

    let stdout = std::io::stdout().lock();
    let mut tw = TabWriter::new(stdout);

    writeln!(tw, "Assets")?;
    writeln!(tw, "Name\tAvailable\tCurrent")?;
    for account in accounts
        .iter()
        .filter(|account| account.r#type == AccountType::Depository)
    {
        let currency_code = account
            .balances
            .iso_currency_code
            .as_deref()
            .and_then(iso::find)
            .unwrap_or(iso::USD);
        writeln!(
            tw,
            "{}\t{}\t{}",
            account.name,
            account
                .balances
                .available
                .map(|amount| { Money::from_decimal(amount, currency_code) })
                .as_ref()
                .unwrap_or(&ZERO_DOLLARS),
            account
                .balances
                .current
                .map(|amount| { Money::from_decimal(amount, currency_code) })
                .as_ref()
                .unwrap_or(&ZERO_DOLLARS),
        )?;
    }

    writeln!(tw, "\nLiabililties")?;
    writeln!(tw, "Name\tAvailable\tCurrent")?;
    for account in accounts
        .iter()
        .filter(|account| account.r#type == AccountType::Credit)
    {
        let currency_code = account
            .balances
            .iso_currency_code
            .as_deref()
            .and_then(iso::find)
            .unwrap_or(iso::USD);
        writeln!(
            tw,
            "{}\t{}\t{}",
            account.name,
            account
                .balances
                .available
                .map(|amount| { Money::from_decimal(amount, currency_code) })
                .as_ref()
                .unwrap_or(&ZERO_DOLLARS),
            account
                .balances
                .current
                .map(|amount| { Money::from_decimal(amount, currency_code) })
                .as_ref()
                .unwrap_or(&ZERO_DOLLARS),
        )?;
    }

    tw.flush()?;

    Ok(())
}

pub(crate) async fn run(matches: &ArgMatches, settings: Settings) -> Result<()> {
    match matches.subcommand() {
        Some(("balances", _link_matches)) => balances(settings).await,
        None => print(settings).await,
        _ => unreachable!(),
    }
}
