use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::prelude::*;

use anyhow::Result;
use clap::ArgMatches;
use futures::future::join_all;
use rplaid::client::{Builder, Credentials};
use rplaid::model::*;
use tabwriter::TabWriter;

use crate::plaid::Link;
use crate::model::{AppData, ConfigFile};

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

    fn ne(&self, other: &AccountType) -> bool {
        self.0 != *other
    }
}

async fn print(conf: ConfigFile) -> Result<()> {
    let state = AppData::new()?;
    let plaid = Builder::new()
        .with_credentials(Credentials {
            client_id: conf.config().plaid.client_id.clone(),
            secret: conf.config().plaid.secret.clone(),
        })
        .with_env(conf.config().plaid.env.clone())
        .build();
    let link_controller = crate::plaid::LinkController::new(
        plaid, state.links_by_env(&conf.config().plaid.env)).await?;

    let table = link_controller.display_accounts_table()?;
    println!("{}", table);

    Ok(())
}

async fn balances(conf: ConfigFile) -> Result<()> {
    let state = AppData::new()?;
    let plaid = Builder::new()
        .with_credentials(Credentials {
            client_id: conf.config().plaid.client_id.clone(),
            secret: conf.config().plaid.secret.clone(),
        })
        .with_env(conf.config().plaid.env.clone())
        .build();

    let links: Vec<Link> = state
        .links()
        .into_iter()
        .filter(|link| link.env == conf.config().plaid.env)
        .collect();

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
            writeln!(
                tw,
                "{}\t{}\t{}",
                b.name,
                b.balances.available.unwrap_or(0.0),
                b.balances.current.unwrap_or(0.0)
            )?;
        }
    }

    writeln!(tw, "\nLiabililties")?;
    for (_k, v) in balances_by_type
        .iter()
        .filter(|(t, _)| *t == &AccountType::Credit)
    {
        for b in v {
            writeln!(
                tw,
                "{}\t{}\t{}",
                b.name,
                b.balances.available.unwrap_or(0.0),
                b.balances.current.unwrap_or(0.0)
            )?;
        }
    }

    let table = String::from_utf8(tw.into_inner()?)?;
    println!("{}", table);

    Ok(())
}

pub(crate) async fn run(matches: &ArgMatches, conf: ConfigFile) -> Result<()> {
    match matches.subcommand() {
        Some(("balance", _link_matches)) => balances(conf).await,
        None => print(conf).await,
        _ => unreachable!(),
    }
}
