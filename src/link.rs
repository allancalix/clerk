use std::sync::{Arc, Mutex};

use anyhow::Result;
use clap::ArgMatches;
use rplaid::client::{Builder, Credentials, Environment, Plaid};
use rplaid::model::*;
use rplaid::HttpClient;
use tokio::sync::{mpsc, oneshot};
use warp::Filter;

use crate::plaid::{Link, LinkStatus};
use crate::model::{AppData, ConfigFile};
use crate::{CLIENT_NAME, COUNTRY_CODES};

type AccessToken = String;

#[derive(Debug, Clone)]
enum Mode {
    Create,
    Update(AccessToken),
}

async fn create_link(
    client: Arc<Plaid<impl HttpClient>>,
    mode: Mode,
) -> Result<impl warp::Reply, std::convert::Infallible> {
    let req = match &mode {
        Mode::Create => CreateLinkTokenRequest {
            client_name: CLIENT_NAME,
            user: LinkUser::new("test-user"),
            language: "en",
            country_codes: &COUNTRY_CODES,
            products: &crate::PRODUCTS,
            ..CreateLinkTokenRequest::default()
        },
        Mode::Update(token) => CreateLinkTokenRequest {
            client_name: CLIENT_NAME,
            user: LinkUser::new("test-user"),
            language: "en",
            country_codes: &COUNTRY_CODES,
            access_token: Some(&token),
            ..CreateLinkTokenRequest::default()
        },
    };

    match client.create_link_token(&req).await {
        Ok(r) => Ok(warp::reply::html(format!(
            r#"
                    <!DOCTYPE html>
                    <script src="https://cdn.plaid.com/link/v2/stable/link-initialize.js"></script>
                    <body></body>
                    <script>var handler = Plaid.create({{
                        token: "{}",
                        onSuccess: (public_token, metadata) => {{
                            window.location.href = `/exchange/${{public_token}}`
                        }},
                        onLoad: () => null,
                        onExit: (event_name, metadata) => null,
                        receivedRedirectUri: null,
                    }}); handler.open();</script>
                    </DOCTYPE>
                    "#,
            r.link_token
        ))),
        Err(err) => Ok(warp::reply::html(err.to_string())),
    }
}

async fn exchange_token(
    public_token: String,
    shutdown: mpsc::Sender<()>,
    env: Environment,
    state: Arc<Mutex<AppData>>,
    client: Arc<Plaid<impl HttpClient>>,
) -> Result<impl warp::Reply, std::convert::Infallible> {
    let res = client.exchange_public_token(public_token).await.unwrap();
    state
        .lock()
        .unwrap()
        .add_link(Link {
            alias: "test".to_string(),
            access_token: res.access_token,
            item_id: res.item_id,
            state: LinkStatus::Active,
            env,
        })
        .map_err(|e| {
            warp::reply::with_status(format!("{}", e), http::StatusCode::INTERNAL_SERVER_ERROR)
        })
        .unwrap();
    shutdown.send(()).await.unwrap();
    Ok(warp::reply::html("OK"))
}

async fn server(conf: ConfigFile, mode: Mode) -> Result<()> {
    let state = Arc::new(Mutex::new(AppData::new()?));
    let plaid = Arc::new(
        Builder::new()
            .with_credentials(Credentials {
                client_id: conf.config().plaid.client_id.clone(),
                secret: conf.config().plaid.secret.clone(),
            })
            .with_env(conf.config().plaid.env.clone())
            .build(),
    );
    let client = warp::any().map(move || plaid.clone());
    let state_filter = warp::any().map(move || state.clone());
    let env_filter = warp::any().map(move || conf.config().plaid.env.clone());
    let mode_filter = warp::any().map(move || mode.clone());

    let link = warp::path("link")
        .and(warp::get())
        .and(client.clone())
        .and(mode_filter)
        .and_then(create_link);

    let (tx, mut rx) = mpsc::channel(1);
    let tx_filter = warp::any().map(move || tx.clone());

    let exchange = warp::path!("exchange" / String)
        .and(warp::get())
        .and(tx_filter)
        .and(env_filter)
        .and(state_filter)
        .and(client)
        .and_then(exchange_token);

    let router = warp::get().and(link.or(exchange));
    let (tx_shutdown, rx_shutdown) = oneshot::channel();
    let (addr, server) =
        warp::serve(router).bind_with_graceful_shutdown(([127, 0, 0, 1], 3030), async {
            rx_shutdown.await.ok();
        });

    println!("Visit http://{}/link to link a new account.", addr);
    tokio::task::spawn(server);
    tokio::task::spawn(async move {
        rx.recv().await.unwrap();
        println!("Successfully linked account... shutting down link server.");
        rx.close();
        let _ = tx_shutdown.send(());
    })
    .await?;

    Ok(())
}

async fn remove(conf: ConfigFile, item_id: &str) -> Result<()> {
    let mut app_data = AppData::new()?;
    let plaid =
        Builder::new()
            .with_credentials(Credentials {
                client_id: conf.config().plaid.client_id.clone(),
                secret: conf.config().plaid.secret.clone(),
            })
            .with_env(conf.config().plaid.env.clone())
            .build();
    let mut link_controller = crate::plaid::LinkController::new(
        plaid, app_data.links_by_env(&conf.config().plaid.env)).await?;
    link_controller.remove_item(item_id).await?;
    app_data.update_links(link_controller.links())?;

    Ok(())
}

async fn status(conf: ConfigFile) -> Result<()> {
    let app_data = AppData::new()?;
    let plaid =
        Builder::new()
            .with_credentials(Credentials {
                client_id: conf.config().plaid.client_id.clone(),
                secret: conf.config().plaid.secret.clone(),
            })
            .with_env(conf.config().plaid.env.clone())
            .build();

    let link_controller = crate::plaid::LinkController::new(
        plaid, app_data.links_by_env(&conf.config().plaid.env)).await?;
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
        },
        _ => match matches.value_of("update") {
            Some(token) => server(conf, Mode::Update(token.into())).await,
            None => server(conf, Mode::Create).await,
        }
    }
}
