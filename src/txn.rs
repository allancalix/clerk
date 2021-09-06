use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::io::SeekFrom;

use anyhow::Result;
use chrono::prelude::*;
use clap::ArgMatches;
use futures_util::pin_mut;
use futures_util::StreamExt;
use rplaid::{Environment, PlaidBuilder, Transaction};

use crate::credentials;
use crate::model::{Config, Link};

async fn pull(start: &str, end: &str, env: Environment) -> Result<()> {
    let state = Config::from_path("state.json");
    let plaid = PlaidBuilder::new()
        .with_credentials(credentials())
        .with_env(env.clone())
        .build();
    let links: Vec<Link> = state
        .links()
        .into_iter()
        .filter(|link| link.env == env)
        .collect();

    let mut fd = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open("transactions.json")?;
    let mut content = String::new();
    fd.read_to_string(&mut content)?;
    let known_txns: Vec<Transaction> = serde_json::from_str(&content).unwrap_or_default();

    let mut results = known_txns
        .into_iter()
        .fold(BTreeMap::new(), |mut acc, txn| {
            acc.insert((txn.date.clone(), txn.transaction_id.clone()), txn);
            acc
        });
    for link in links {
        let txns = plaid.transactions_iter(rplaid::GetTransactionsRequest {
            access_token: link.access_token.as_str(),
            start_date: start,
            end_date: end,
            options: Some(rplaid::GetTransactionsOptions {
                count: Some(100),
                offset: Some(0),
                account_ids: None,
                include_original_description: None,
            }),
        });
        pin_mut!(txns);

        while let Some(txn_page) = txns.next().await {
            for txn in txn_page.unwrap() {
                results.insert((txn.date.clone(), txn.transaction_id.clone()), txn);
            }
        }
    }

    let output = results
        .into_iter()
        .map(|(_, v)| v)
        .collect::<Vec<Transaction>>();
    let json = serde_json::to_string_pretty(&output)?;
    fd.seek(SeekFrom::Start(0))?;
    write!(fd, "{}", json)?;

    Ok(())
}

pub(crate) async fn run(matches: &ArgMatches, env: Environment) -> Result<()> {
    let start = matches.value_of("begin").map_or_else(
        || {
            let last_week = Local::now() - chrono::Duration::weeks(1);
            last_week.format("%Y-%m-%d").to_string()
        },
        |v| v.to_string(),
    );
    let end = matches.value_of("until").map_or_else(
        || Local::now().format("%Y-%m-%d").to_string(),
        |v| v.to_string(),
    );

    pull(&start, &end, env).await?;
    Ok(())
}
