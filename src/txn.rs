use anyhow::Result;
use clap::ArgMatches;
use tracing::{debug, info};

use crate::model::ConfigFile;
use crate::plaid::{default_plaid_client, Link};
use crate::store::SqliteStore;
use crate::upstream::{plaid::Source, TransactionSource};

#[tracing::instrument(skip(conf))]
async fn pull(conf: ConfigFile) -> Result<()> {
    let mut store = SqliteStore::new(&conf.data_path()).await?;
    let plaid = default_plaid_client(&conf);
    let links: Vec<Link> = store.links().list().await?;

    for link in links {
        let mut upstream = Source::new(&plaid, link.access_token.clone(), link.sync_cursor.clone());

        info!("Pulling transactions for item {}.", link.item_id);
        let mut count = 0;
        for tx in upstream.transactions().await? {
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

        info!("{} transactions modified.", upstream.modified().len());
        info!("{} transactions removed.", upstream.removed().len());

        let updated_link = Link {
            sync_cursor: Some(upstream.next_cursor()),
            ..link
        };
        if &updated_link.sync_cursor != &link.sync_cursor {
            info!(
                "Updating link with latest cursor. cursor={:?}",
                &updated_link.sync_cursor
            );
            store.links().update(&updated_link).await?;
        }

        info!(
            "Inserted {} new transactions for item {}.",
            count, updated_link.item_id
        );
    }

    Ok(())
}

pub(crate) async fn run(matches: &ArgMatches, conf: ConfigFile) -> Result<()> {
    match matches.subcommand() {
        Some(("sync", _link_matches)) => pull(conf).await,
        None => unreachable!("command is requires"),
        _ => unreachable!(),
    }
}
