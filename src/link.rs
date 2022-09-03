use std::collections::HashMap;

use anyhow::Result;
use clap::ArgMatches;
use crossbeam_channel::{bounded, Receiver};
use plaid_link::{LinkMode, State};
use tokio::signal;
use tokio::time::{sleep_until, Duration, Instant};

use crate::plaid::{default_plaid_client, Link, LinkController, LinkStatus};
use crate::settings::Settings;
use crate::store;

const LINK_NAME_KEY: &str = "link_name";

async fn shutdown_signal(rx: Receiver<()>) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    let program_shutdown = async {
        tokio::task::spawn_blocking(move || rx.recv().expect("failed to read from channel"))
            .await
            .unwrap();
    };

    let timeout = async {
        sleep_until(Instant::now() + Duration::from_secs(300)).await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
        _ = program_shutdown => {},
        _ = timeout => {},
    }

    println!("signal received, starting graceful shutdown");
}

async fn server(settings: Settings, mode: plaid_link::LinkMode, name: &str) -> Result<()> {
    let plaid = default_plaid_client(&settings);

    let (tx, rx) = bounded(1);
    let server = plaid_link::LinkServer::new(plaid);

    let mut listener = server.on_exchange();
    let mut store = store::SqliteStore::new(&settings.db_file).await?;
    let link = match &mode {
        plaid_link::LinkMode::Update(s) => Some(store.links().link(s).await?),
        plaid_link::LinkMode::Create => None,
    };

    let mode = std::sync::Arc::new(mode);
    let m = mode.clone();
    tokio::spawn(async move {
        let token = listener.recv().await.unwrap();
        let name = match token.state.context {
            Some(map) => map.get(LINK_NAME_KEY).unwrap().clone(),
            None => "".to_string(),
        };

        match m.as_ref() {
            plaid_link::LinkMode::Update(_) => {
                store
                    .links()
                    .update(&Link {
                        alias: name,
                        access_token: token.access_token,
                        item_id: token.item_id,
                        state: LinkStatus::Active,
                        sync_cursor: None,
                    })
                    .await
                    .unwrap();
            }
            _ => {
                store
                    .links()
                    .save(&Link {
                        alias: name,
                        access_token: token.access_token.clone(),
                        item_id: token.item_id.clone(),
                        state: LinkStatus::Active,
                        sync_cursor: None,
                    })
                    .await
                    .unwrap();

                let plaid = default_plaid_client(&settings);
                for acc in plaid.accounts(token.access_token).await.unwrap() {
                    store
                        .accounts()
                        .save(&token.item_id, &acc.into())
                        .await
                        .unwrap();
                }
            }
        }

        tx.send(()).unwrap();
    });

    let router = server.start();
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
    let server = axum::Server::bind(&addr).serve(router.into_make_service());

    let mut context = HashMap::new();
    context.insert(LINK_NAME_KEY.to_string(), name.to_string());

    let state = State {
        user_id: "test-user".to_string(),
        context: Some(context),
    };
    match mode.as_ref() {
        LinkMode::Create => println!(
            "Visit http://{}/link?state={} to link a new account.",
            server.local_addr(),
            state.to_opaque()?
        ),
        LinkMode::Update(_) => {
            println!(
                "Visit http://{}/link?mode=update&token={}&state={} to link a new account.",
                server.local_addr(),
                link.expect("must have existing link when using update")
                    .access_token,
                state.to_opaque()?
            )
        }
    };

    server
        .with_graceful_shutdown(shutdown_signal(rx))
        .await
        .expect("failed to start Plaid link server");

    Ok(())
}

async fn remove(settings: Settings, item_id: &str) -> Result<()> {
    let mut store = store::SqliteStore::new(&settings.db_file).await?;
    let plaid = default_plaid_client(&settings);

    let link = store.links().link(item_id).await?;
    plaid.item_del(&link.access_token).await?;
    store.links().delete(item_id).await?;

    Ok(())
}

async fn status(settings: Settings) -> Result<()> {
    let store = store::SqliteStore::new(&settings.db_file).await?;
    let plaid = default_plaid_client(&settings);

    let link_controller = LinkController::new(plaid, store).await?;

    println!("{}", link_controller.display_connections_table()?);

    Ok(())
}

pub(crate) async fn run(matches: &ArgMatches, settings: Settings) -> Result<()> {
    match matches.subcommand() {
        Some(("status", _status_matches)) => status(settings).await,
        Some(("delete", remove_matches)) => {
            // SAFETY: This should be fine so long as this is a positional
            // argument as clap will prevent this code from executing without a
            // value.
            let item_id = remove_matches.value_of("item_id").unwrap();
            remove(settings, item_id).await
        }
        _ => {
            let name = matches.value_of("name").unwrap_or("");
            match matches.value_of("update") {
                Some(token) => {
                    server(
                        settings,
                        plaid_link::LinkMode::Update(token.to_string()),
                        name,
                    )
                    .await
                }
                None => server(settings, plaid_link::LinkMode::Create, name).await,
            }
        }
    }
}
