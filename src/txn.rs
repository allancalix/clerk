use std::collections::BTreeMap;
use std::io::prelude::*;

use anyhow::Result;
use chrono::prelude::*;
use clap::ArgMatches;
use futures_util::pin_mut;
use futures_util::StreamExt;
use rplaid::{Credentials, PlaidBuilder, Transaction};
use tabwriter::TabWriter;

use crate::model::{AppData, Conf, Config, Link};

const UNKNOWN_ACCOUNT: &str = "Expenses:Unknown";

async fn pull(start: &str, end: &str, conf: Conf) -> Result<()> {
    let state = Config::new();
    let plaid = PlaidBuilder::new()
        .with_credentials(Credentials {
            client_id: conf.plaid.client_id.clone(),
            secret: conf.plaid.secret.clone(),
        })
        .with_env(conf.plaid.env.clone())
        .build();
    let links: Vec<Link> = state
        .links()
        .into_iter()
        .filter(|link| link.env == conf.plaid.env)
        .collect();

    let mut app_data = AppData::new()?;
    let mut results =
        app_data
            .transactions
            .clone()
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
    app_data.set_txns(output)?;

    Ok(())
}

fn print_ledger() -> Result<()> {
    let app_data = AppData::new()?;
    let rules = crate::rules::Interpreter::from_rules_file()?;

    let mut tw = TabWriter::new(vec![]);
    for mut txn in app_data.transactions {
        rules.apply(&mut txn);

        let date = NaiveDate::parse_from_str(&txn.date, "%Y-%m-%d")?;
        let status = if txn.pending { "!" } else { "*" };
        writeln!(tw, "{} {} {}", date.format("%Y/%m/%d"), status, txn.name)?;
        writeln!(tw, "\t; TXID: {}", txn.transaction_id)?;
        writeln!(tw, "\t{}\t${:.2}", UNKNOWN_ACCOUNT, txn.amount)?;
        writeln!(tw, "\t{}\n", &txn.account_id)?;
    }

    let output = String::from_utf8(tw.into_inner()?)?;
    println!("{}", output);

    Ok(())
}

pub(crate) async fn run(matches: &ArgMatches, conf: Conf) -> Result<()> {
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
        Some(("print", _link_matches)) => print_ledger(),
        None => unreachable!("command is requires"),
        _ => unreachable!(),
    }
}
