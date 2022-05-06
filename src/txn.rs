use std::collections::{BTreeMap, HashSet};
use std::io::prelude::*;

use anyhow::Result;
use axum::async_trait;
use chrono::prelude::*;
use clap::ArgMatches;
use futures_util::pin_mut;
use futures_util::StreamExt;
use rplaid::client::{Builder, Credentials, Plaid};
use rplaid::model::*;
use rplaid::HttpClient;
use tabwriter::TabWriter;

use crate::model::{AppData, ConfigFile};
use crate::plaid::Link;
use crate::rules::Transformer;

#[async_trait]
trait TransactionUpstream {
    async fn pull(&self, start: NaiveDate, end: NaiveDate) -> Result<Vec<Transaction>>;
}

struct PlaidUpstream<'a, T: HttpClient> {
    client: &'a Plaid<T>,
    token: String,
}

#[async_trait]
impl<'a, T: HttpClient> TransactionUpstream for PlaidUpstream<'a, T> {
    async fn pull(&self, start: NaiveDate, end: NaiveDate) -> Result<Vec<Transaction>> {
        let start = start.format("%Y-%m-%d").to_string();
        let end = end.format("%Y-%m-%d").to_string();

        let tx_pages = self.client.transactions_iter(GetTransactionsRequest {
            access_token: self.token.as_str(),
            start_date: &start,
            end_date: &end,
            options: Some(GetTransactionsOptions {
                count: Some(100),
                offset: Some(0),
                account_ids: None,
                include_original_description: None,
            }),
        });
        pin_mut!(tx_pages);

        let mut tx_list = vec![];
        while let Some(page) = tx_pages.next().await {
            tx_list.extend_from_slice(&page?);
        }

        Ok(tx_list)
    }
}

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

    let start_date = NaiveDate::parse_from_str(start, "%Y-%m-%d")?;
    let end_date = NaiveDate::parse_from_str(end, "%Y-%m-%d")?;

    for link in links {
        let upstream = PlaidUpstream {
            client: &plaid,
            token: link.access_token.clone(),
        };

        let txs = upstream.pull(start_date, end_date).await?;
        for tx in txs {
            results.insert((tx.date.clone(), tx.transaction_id.clone()), tx);
        }
    }

    let output = results
        .into_iter()
        .map(|(_, v)| v)
        .collect::<Vec<Transaction>>();
    app_data.set_txns(output)?;

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

    let txs: Vec<crate::rules::TransactionValue> = app_data
        .transactions()
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
