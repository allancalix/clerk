use std::collections::BTreeMap;
use std::io::prelude::*;

use anyhow::Result;
use chrono::prelude::*;
use clap::ArgMatches;
use futures_util::pin_mut;
use futures_util::StreamExt;
use rplaid::{Environment, PlaidBuilder, Transaction};
use tabwriter::TabWriter;

use crate::credentials;
use crate::model::{AppData, Config, Link};

async fn pull(start: &str, end: &str, env: Environment) -> Result<()> {
    let state = Config::new();
    let plaid = PlaidBuilder::new()
        .with_credentials(credentials())
        .with_env(env.clone())
        .build();
    let links: Vec<Link> = state
        .links()
        .into_iter()
        .filter(|link| link.env == env)
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

fn rules(id: &str) -> String {
    match id {
        "zejzDgrmNbIPo9Rp4Qnrupk5Rmg36EIAYjod6" => "Assets:Chase Checking".to_string(),
        "merz5mD9yNIRQzxVM4BAIZnbNO7RPKHrYKX3A" => "Liabilities:Chase Freedom".to_string(),
        "BJMkD6PA7qFmKjEX89ZEFEpgxgYJv9S9MeV8K" => "Liabilities:AMEX".to_string(),
        "YgrMKqXebzcPzVLLzJRVFQ4Oy0jopMcej63pn" => "Assets:AMEX Savings".to_string(),
        "5MP9EJojZ6s8DjJy6zqruxvxv9vKkXfBOP5zR" => "Assets:Paypal".to_string(),
        "ZjYbvyD9ZLCxEyZgk530tNJNJBJOb0CR3Bp7r" => "Liabilities:Paypal Credit".to_string(),
        "Bpm6EnnPOKFP8Jp1KLMgf49A917gYkF9MykKQ" => "Assets:Wealthfront Cash".to_string(),
        "qDnaY338gJCP4Mgb8dXafvE6EZJOjxFJOwvKn" => "Assets:Wealthfront Investment".to_string(),
        _ => panic!(),
    }
}

fn print_ledger() -> Result<()> {
    let app_data = AppData::new()?;

    let mut tw = TabWriter::new(vec![]);
    for txn in app_data.transactions {
        let date = NaiveDate::parse_from_str(&txn.date, "%Y-%m-%d")?;
        let status = if txn.pending { "!" } else { "*" };
        writeln!(tw, "{} {} {}", date.format("%Y/%m/%d"), status, txn.name)?;
        writeln!(tw, "\t; TXID: {}", txn.transaction_id)?;
        writeln!(tw, "\t{}\t${:.2}", "Expenses:Unknown", txn.amount)?;
        writeln!(tw, "\t{}\n", rules(&txn.account_id))?;
    }

    let output = String::from_utf8(tw.into_inner()?)?;
    println!("{}", output);

    Ok(())
}

pub(crate) async fn run(matches: &ArgMatches, env: Environment) -> Result<()> {
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

            pull(&start, &end, env).await
        }
        Some(("print", _link_matches)) => print_ledger(),
        None => unreachable!("command is requires"),
        _ => unreachable!(),
    }
}
