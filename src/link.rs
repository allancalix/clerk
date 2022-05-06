use std::sync::{Arc, Mutex};

use anyhow::Result;
use clap::ArgMatches;
use crossbeam_channel::{bounded, Receiver};
use rplaid::client::{Builder, Credentials};
use tokio::signal;
use tokio::time::{sleep_until, Instant, Duration};

use crate::link_server::LinkMode;
use crate::model::{AppData, ConfigFile};

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

async fn server(conf: ConfigFile, mode: crate::link_server::LinkMode) -> Result<()> {
    let state = Arc::new(Mutex::new(AppData::new()?));
    let plaid = Builder::new()
        .with_credentials(Credentials {
            client_id: conf.config().plaid.client_id.clone(),
            secret: conf.config().plaid.secret.clone(),
        })
        .with_env(conf.config().plaid.env.clone())
        .build();

    let (tx, rx) = bounded(1);
    let server = crate::link_server::LinkServer {
        client: plaid,
        on_exchange: move |link| {
            state.lock().unwrap().add_link(link).unwrap();

            tx.send(()).unwrap();
        },
    };

    let router = server.start();
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 4545));
    let server = axum::Server::bind(&addr)
        .serve(router.into_make_service())
        .with_graceful_shutdown(shutdown_signal(rx));

    match mode {
        LinkMode::Create => println!("Visit http://{}/link to link a new account.", addr),
        LinkMode::Update(s) => println!(
            "Visit http://{}/link?mode=update&token={} to link a new account.",
            addr, s
        ),
    };

    server.await.expect("failed to start Plaid link server");

    Ok(())
}

async fn remove(conf: ConfigFile, item_id: &str) -> Result<()> {
    let mut app_data = AppData::new()?;
    let plaid = Builder::new()
        .with_credentials(Credentials {
            client_id: conf.config().plaid.client_id.clone(),
            secret: conf.config().plaid.secret.clone(),
        })
        .with_env(conf.config().plaid.env.clone())
        .build();
    let mut link_controller =
        crate::plaid::LinkController::new(plaid, app_data.links_by_env(&conf.config().plaid.env))
            .await?;
    link_controller.remove_item(item_id).await?;
    app_data.update_links(link_controller.links())?;

    Ok(())
}

async fn status(conf: ConfigFile) -> Result<()> {
    let app_data = AppData::new()?;
    let plaid = Builder::new()
        .with_credentials(Credentials {
            client_id: conf.config().plaid.client_id.clone(),
            secret: conf.config().plaid.secret.clone(),
        })
        .with_env(conf.config().plaid.env.clone())
        .build();

    let link_controller =
        crate::plaid::LinkController::new(plaid, app_data.links_by_env(&conf.config().plaid.env))
            .await?;
    println!("{}", link_controller.display_connections_table()?);
    Ok(())
}

pub(crate) async fn run(matches: &ArgMatches, conf: ConfigFile) -> Result<()> {
    match matches.subcommand() {
        Some(("status", _status_matches)) => status(conf).await,
        Some(("delete", remove_matches)) => {
            // SAFETY: This should be fine so long as this is a positional
            // argument as clap will prevent this code from executing without a
            // value.
            let item_id = remove_matches.value_of("item_id").unwrap();
            remove(conf, item_id).await
        }
        _ => match matches.value_of("update") {
            Some(token) => {
                server(
                    conf,
                    crate::link_server::LinkMode::Update(token.to_string()),
                )
                .await
            }
            None => server(conf, crate::link_server::LinkMode::Create).await,
        },
    }
}
