use anyhow::Result;
use chrono::prelude::*;
use clap::ArgMatches;
use tracing::{debug, info};

use crate::model::ConfigFile;
use crate::plaid::{default_plaid_client, Link};
use crate::upstream::{plaid::Source, TransactionSource};

#[tracing::instrument(skip(conf))]
async fn pull(start: &str, end: &str, conf: ConfigFile) -> Result<()> {
    let mut store = crate::store::SqliteStore::new(&conf.data_path()).await?;
    let plaid = default_plaid_client(&conf);
    let links: Vec<Link> = store.links().await?;

    let start_date = NaiveDate::parse_from_str(start, "%Y-%m-%d")?;
    let end_date = NaiveDate::parse_from_str(end, "%Y-%m-%d")?;

    for link in links {
        let upstream = Source::new(&plaid, link.access_token.clone());

        info!("Pulling transactions for item {}.", link.item_id);
        let mut count = 0;
        for tx in upstream.transactions(start_date, end_date).await? {
            if !tx.source.pending {
                if let Some(pending_txn_id) = &tx.source.pending_transaction_id {
                    let canonical_id = store.tx_by_plaid_id(&link.item_id, pending_txn_id).await?;

                    info!("update of existing transaction. id={:?}", canonical_id);
                }
            }

            let result = store
                .save_tx(&link.item_id, &tx.source.transaction_id, &tx)
                .await;

            match result {
                Ok(_) => {
                    count += 1;
                }
                Err(crate::store::Error::AlreadyExists) => {
                    debug!(
                        "Transaction with id {} already exists, skipping.",
                        &tx.source.transaction_id
                    );

                    continue;
                }
                Err(e) => {
                    return Err(e.into());
                }
            }
        }

        info!(
            "Inserted {} new transactions for item {}.",
            count, link.item_id
        );
    }

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
        None => unreachable!("command is requires"),
        _ => unreachable!(),
    }
}
