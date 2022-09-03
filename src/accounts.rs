use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::prelude::*;

use anyhow::Result;
use clap::ArgMatches;
use futures::future::join_all;
use lazy_static::lazy_static;
use rplaid::model::*;
use rusty_money::{
    iso::{self, Currency},
    Money,
};
use tabwriter::TabWriter;

use crate::model::ConfigFile;
use crate::plaid::{default_plaid_client, Link};

lazy_static! {
    static ref ZERO_DOLLARS: Money<'static, Currency> = Money::from_minor(0_i64, iso::USD);
}

#[derive(Eq, PartialEq)]
struct AccountTypeWrapper(AccountType);

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

async fn print(conf: ConfigFile) -> Result<()> {
    let plaid = default_plaid_client(&conf);
    let store = crate::store::SqliteStore::new(&conf.data_path()).await?;

    let link_controller = crate::plaid::LinkController::new(plaid, store).await?;

    let table = link_controller.display_accounts_table()?;

    println!("{}", table);

    Ok(())
}

async fn balances(conf: ConfigFile) -> Result<()> {
    let mut store = crate::store::SqliteStore::new(&conf.data_path()).await?;
    let plaid = default_plaid_client(&conf);

    let links: Vec<Link> = store.links().list().await?;

    let mut balances_by_type = HashMap::new();
    let mut futures = vec![];
    for link in links {
        futures.push(plaid.balances(link.access_token));
    }
    let results = join_all(futures).await;

    for balance in results {
        let balance = balance?;
        for b in balance {
            balances_by_type
                .entry(AccountTypeWrapper(b.r#type))
                .or_insert(Vec::new())
                .push(b);
        }
    }

    let mut tw = TabWriter::new(vec![]);

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

    let table = String::from_utf8(tw.into_inner()?)?;
    println!("{}", table);

    Ok(())
}

pub(crate) async fn run(matches: &ArgMatches, conf: ConfigFile) -> Result<()> {
    match matches.subcommand() {
        Some(("balances", _link_matches)) => balances(conf).await,
        None => print(conf).await,
        _ => unreachable!(),
    }
}
