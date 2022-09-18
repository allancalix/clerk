use anyhow::{anyhow, Result};
use clap::ArgMatches;
use tracing::info;

use crate::plaid::{default_plaid_client, Link};
use crate::settings::Settings;
use crate::store::SqliteStore;
use crate::upstream::{plaid::Source, TransactionEvent, TransactionSource};

#[tracing::instrument]
async fn pull(settings: Settings) -> Result<()> {
    let mut store = SqliteStore::new(&settings.db_file).await?;
    let plaid = default_plaid_client(&settings.plaid);
    let links: Vec<Link> = store.links().list().await?;

    for link in links {
        let mut upstream = Source::new(&plaid, link.access_token.clone(), link.sync_cursor.clone());

        info!("Pulling transactions for item {}.", link.item_id);
        let mut added_count = 0;
        let mut modified_count = 0;
        let mut removed_count = 0;
        for tx in upstream.transactions().await? {
            match tx {
                TransactionEvent::Added(entry) => {
                    if !entry.source.pending {
                        if let Some(pending_txn_id) = &entry.source.pending_transaction_id {
                            let canonical_id = store.txns().by_upstream_id(pending_txn_id).await?;

                            info!("update of existing transaction. id={:?}", canonical_id);
                        }

                        store.txns().save(&entry.source.account_id, &entry).await?;

                        added_count += 1;
                    }
                }
                TransactionEvent::Modified(entry) => {
                    match store
                        .txns()
                        .by_upstream_id(&entry.source.transaction_id)
                        .await?
                    {
                        Some(id) => {
                            store.txns().update_source(&id, entry.source).await?;

                            modified_count += 1;
                        }
                        None => return Err(anyhow!("transaction modified with no base")),
                    }
                }
                TransactionEvent::Removed(id) => {
                    store.txns().delete(&id).await?;

                    removed_count += 1;
                }
            }
        }

        info!(
            "{} total transactions. added={} modified={} removed={}",
            added_count + modified_count + removed_count,
            added_count,
            modified_count,
            removed_count
        );

        let updated_link = Link {
            sync_cursor: Some(upstream.next_cursor()),
            ..link
        };
        if updated_link.sync_cursor != link.sync_cursor {
            info!(
                "Updating link with latest cursor. cursor={:?}",
                &updated_link.sync_cursor
            );
            store.links().update(&updated_link).await?;
        }
    }

    Ok(())
}

pub(crate) async fn run(matches: &ArgMatches, settings: Settings) -> Result<()> {
    match matches.subcommand() {
        Some(("sync", _link_matches)) => pull(settings).await,
        None => unreachable!("command is requires"),
        _ => unreachable!(),
    }
}
