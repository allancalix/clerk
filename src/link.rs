use anyhow::Result;
use clap::ArgMatches;
use rplaid::{CreateLinkTokenRequest, Environment, HttpClient, LinkUser, Plaid};
use warp::Filter;

use crate::model::{Conf, Config, Link};
use crate::{CLIENT_NAME, COUNTRY_CODES};

pub async fn create_link(
    client: std::sync::Arc<Plaid<impl HttpClient>>,
) -> Result<impl warp::Reply, std::convert::Infallible> {
    let res = client
        .create_link_token(&CreateLinkTokenRequest {
            client_name: CLIENT_NAME,
            user: LinkUser::new("test-user"),
            language: "en",
            country_codes: &COUNTRY_CODES,
            products: &crate::PRODUCTS,
            webhook: None,
            access_token: None,
            link_customization_name: None,
            redirect_uri: None,
            android_package_name: None,
            institution_id: None,
        })
        .await
        .unwrap();

    Ok(warp::reply::html(format!(
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
        res.link_token
    )))
}

async fn exchange_token(
    public_token: String,
    shutdown: tokio::sync::mpsc::Sender<()>,
    env: Environment,
    state: std::sync::Arc<std::sync::Mutex<Config>>,
    client: std::sync::Arc<Plaid<impl HttpClient>>,
) -> Result<impl warp::Reply, std::convert::Infallible> {
    let res = client.exchange_public_token(public_token).await.unwrap();
    state.lock().unwrap().add_link(Link {
        access_token: res.access_token,
        item_id: res.item_id,
        state: "NEW".into(),
        env,
    });
    shutdown.send(()).await.unwrap();
    Ok(warp::reply::html("OK"))
}

async fn server(conf: Conf) {
    let state = std::sync::Arc::new(std::sync::Mutex::new(Config::new()));
    let plaid = std::sync::Arc::new(
        rplaid::PlaidBuilder::new()
            .with_credentials(rplaid::Credentials {
                client_id: conf.plaid.client_id.clone(),
                secret: conf.plaid.secret.clone(),
            })
            .with_env(conf.plaid.env.clone())
            .build(),
    );
    let client = warp::any().map(move || plaid.clone());
    let state_filter = warp::any().map(move || state.clone());
    let env_filter = warp::any().map(move || conf.plaid.env.clone());

    let link = warp::path("link")
        .and(warp::get())
        .and(client.clone())
        .and_then(create_link);

    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let tx_filter = warp::any().map(move || tx.clone());

    let exchange = warp::path!("exchange" / String)
        .and(warp::get())
        .and(tx_filter)
        .and(env_filter)
        .and(state_filter)
        .and(client)
        .and_then(exchange_token);

    let router = warp::get().and(link.or(exchange));
    let (tx_shutdown, rx_shutdown) = tokio::sync::oneshot::channel();
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
    .await
    .unwrap();
}

pub(crate) async fn run(_matches: &ArgMatches, conf: Conf) -> Result<()> {
    server(conf).await;

    Ok(())
}
