use std::collections::{BTreeMap, HashSet};
use std::io::prelude::*;

use anyhow::Result;
use chrono::prelude::*;
use clap::ArgMatches;
use futures_util::pin_mut;
use futures_util::StreamExt;
use rplaid::client::{Builder, Credentials};
use rplaid::model::*;
use tabwriter::TabWriter;

use crate::model::{AppData, ConfigFile, Link};
use crate::rules::Transformer;

async fn pull(start: &str, end: &str, conf: ConfigFile) -> Result<()> {
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

    let mut app_data = AppData::new()?;
    let mut results =
        app_data
            .transactions()
            .clone()
            .into_iter()
            .fold(BTreeMap::new(), |mut acc, txn| {
                acc.insert((txn.date.clone(), txn.transaction_id.clone()), txn);
                acc
            });
    for link in links {
        let txns = plaid.transactions_iter(GetTransactionsRequest {
            access_token: link.access_token.as_str(),
            start_date: start,
            end_date: end,
            options: Some(GetTransactionsOptions {
                count: Some(100),
                offset: Some(0),
                account_ids: None,
                include_original_description: None,
            }),
        });
        pin_mut!(txns);

        while let Some(txn_page) = txns.next().await {
            for txn in txn_page? {
                results.insert((txn.date.clone(), txn.transaction_id.clone()), txn);
            }
        }
    }

    let output = results
        .into_iter()
        .map(|(_, v)| v)
        .collect::<Vec<Transaction>>();
    app_data.set_txns(output)?;

    Ok(())
}

async fn print_ledger(start: Option<&str>, end: Option<&str>, conf: ConfigFile) -> Result<()> {
    let app_data = AppData::new()?;
    let rules = Transformer::from_rules(conf.rules())?;
    let plaid = Builder::new()
        .with_credentials(Credentials {
            client_id: conf.config().plaid.client_id.clone(),
            secret: conf.config().plaid.secret.clone(),
        })
        .with_env(conf.config().plaid.env.clone())
        .build();

    let mut account_ids = HashSet::new();
    for link in app_data
        .links()
        .iter()
        .filter(|link| link.env == conf.config().plaid.env)
    {
        let accounts = plaid.accounts(&link.access_token).await?;
        for account in accounts {
            account_ids.insert(account.account_id);
        }
    }

    let mut tw = TabWriter::new(vec![]);
    for mut txn in app_data
        .transactions()
        .iter()
        .filter(|t| account_ids.contains(&t.account_id))
    {
        let transformed_txn = rules.apply(&mut txn)?;

        let date = NaiveDate::parse_from_str(&transformed_txn.date, "%Y-%m-%d")?;
        if let Some(start_date) = start {
            let start_parsed = NaiveDate::parse_from_str(&start_date, "%Y-%m-%d")?;
            if start_parsed > date {
                continue;
            }
        }

        if let Some(end_date) = end {
            let end_parsed = NaiveDate::parse_from_str(&end_date, "%Y-%m-%d")?;
            if end_parsed < date {
                continue;
            }
        }

        let status = if transformed_txn.pending { "!" } else { "*" };
        writeln!(
            tw,
            "{} {} {}",
            date.format("%Y/%m/%d"),
            status,
            transformed_txn.payee
        )?;
        writeln!(tw, "\t; TXID: {}", transformed_txn.plaid_id)?;
        writeln!(
            tw,
            "\t{}\t${:.2}",
            transformed_txn.dest_account, transformed_txn.amount
        )?;
        writeln!(tw, "\t{}\n", &transformed_txn.source_account)?;
    }

    let output = String::from_utf8(tw.into_inner()?)?;
    println!("{}", output);

    Ok(())
}

pub(crate) async fn run(matches: &ArgMatches, conf: ConfigFile) -> Result<()> {
    match matches.subcommand() {
        Some(("sync", link_matches)) => {
            let start = link_matches.value_of("begin").map_or_else(
                || {
                    let last_week = Local::now() - chrono::Duration::weeks(1);
                    last_week.format("%Y-%m-%d").to_string()
                },
                |v| v.to_string(),
            );
            let end = link_matches.value_of("until").map_or_else(
                || Local::now().format("%Y-%m-%d").to_string(),
                |v| v.to_string(),
            );

            pull(&start, &end, conf).await
        }
        Some(("print", link_matches)) => {
            let start = link_matches
                .value_of("begin")
                .map_or_else(|| None, |v| Some(v));

            let end = link_matches
                .value_of("until")
                .map_or_else(|| None, |v| Some(v));

            print_ledger(start, end, conf).await
        }
        None => unreachable!("command is requires"),
        _ => unreachable!(),
    }
}
