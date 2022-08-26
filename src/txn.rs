use std::collections::HashSet;
use std::io::prelude::*;

use anyhow::Result;
use chrono::prelude::*;
use clap::ArgMatches;
use tabwriter::TabWriter;
use tracing::info;

use crate::model::ConfigFile;
use crate::plaid::{default_plaid_client, Link};
use crate::rules::Transformer;
use crate::upstream::{plaid::Source, AccountSource, TransactionSource};

#[tracing::instrument(skip(conf))]
async fn pull(start: &str, end: &str, conf: ConfigFile) -> Result<()> {
    let mut store = crate::store::SqliteStore::new(&conf.data_path()?).await?;
    let plaid = default_plaid_client(&conf);
    let links: Vec<Link> = store
        .links()
        .await?
        .into_iter()
        .filter(|e| e.env == conf.config().plaid.env)
        .collect();

    let start_date = NaiveDate::parse_from_str(start, "%Y-%m-%d")?;
    let end_date = NaiveDate::parse_from_str(end, "%Y-%m-%d")?;

    for link in links {
        let upstream = Source {
            client: &plaid,
            token: link.access_token.clone(),
        };

        info!("Pulling transactions for item {}.", link.item_id);
        for tx in upstream.transactions(start_date, end_date).await? {
            let result = store.save_tx(&link.item_id, &tx).await;

            if result.contains_err(&crate::store::Error::AlreadyExists) {
                info!("Transaction {} already found, skipping.", tx.transaction_id);

                continue;
            }

            result?
        }
    }

    Ok(())
}

fn print_table(txs: &[crate::rules::TransactionValue]) -> Result<String> {
    let mut tw = TabWriter::new(vec![]);

    for tx in txs {
        let status = if tx.pending { "!" } else { "*" };
        writeln!(tw, "{} {} {}", tx.date, status, tx.payee)?;
        writeln!(tw, "\t; TXID: {}", tx.plaid_id)?;
        writeln!(tw, "\t{}\t${:.2}", tx.dest_account, tx.amount)?;
        writeln!(tw, "\t{}\n", tx.source_account)?;
    }

    Ok(String::from_utf8(tw.into_inner()?)?)
}

async fn print_ledger(start: Option<&str>, end: Option<&str>, conf: ConfigFile) -> Result<()> {
    let mut store = crate::store::SqliteStore::new(&conf.data_path()?).await?;
    let rules = Transformer::from_rules(conf.rules())?;
    let plaid = default_plaid_client(&conf);
    let mut account_ids = HashSet::new();

    let links: Vec<Link> = store
        .links()
        .await?
        .into_iter()
        .filter(|e| e.env == conf.config().plaid.env)
        .collect();
    for link in links {
        let upstream = Source::new(&plaid, link.access_token.clone());
        account_ids.extend(upstream.accounts().await?.into_iter().map(|a| a.account_id));
    }

    let txs: Vec<crate::rules::TransactionValue> = store
        .transactions()
        .await?
        .iter()
        .filter(|t| account_ids.contains(&t.account_id))
        .filter(|t| {
            let date = NaiveDate::parse_from_str(&t.date, "%Y-%m-%d").unwrap();
            if let Some(start_date) = start {
                let start_parsed = NaiveDate::parse_from_str(start_date, "%Y-%m-%d").unwrap();
                if start_parsed > date {
                    return false;
                }
            }

            if let Some(end_date) = end {
                let end_parsed = NaiveDate::parse_from_str(end_date, "%Y-%m-%d").unwrap();
                if end_parsed < date {
                    return false;
                }
            }

            true
        })
        .map(|t| rules.apply(t).unwrap())
        .collect();

    println!("{}", print_table(&txs)?);

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
            let start = link_matches.value_of("begin").map_or_else(|| None, Some);

            let end = link_matches.value_of("until").map_or_else(|| None, Some);

            print_ledger(start, end, conf).await
        }
        None => unreachable!("command is requires"),
        _ => unreachable!(),
    }
}
