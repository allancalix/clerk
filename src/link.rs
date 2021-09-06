use anyhow::Result;
use clap::ArgMatches;
use rplaid::{CreateLinkTokenRequest, Environment, HttpClient, LinkUser, Plaid};
use warp::Filter;

use crate::model::{Config, Link};
use crate::CLIENT_NAME;

pub async fn create_link(
    client: std::sync::Arc<Plaid<impl HttpClient>>,
) -> Result<impl warp::Reply, std::convert::Infallible> {
    let res = client
        .create_link_token(&CreateLinkTokenRequest {
            client_name: CLIENT_NAME,
            user: LinkUser::new("test-user"),
            language: "en",
            country_codes: &["US"],
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
    env: rplaid::Environment,
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
    Ok(warp::reply::html("OK"))
}

async fn server(env: rplaid::Environment) {
    let state = std::sync::Arc::new(std::sync::Mutex::new(Config::new()));
    let plaid = std::sync::Arc::new(
        rplaid::PlaidBuilder::new()
            .with_credentials(crate::credentials())
            .with_env(env.clone())
            .build(),
    );
    let client = warp::any().map(move || plaid.clone());
    let state_filter = warp::any().map(move || state.clone());
    let env_filter = warp::any().map(move || env.clone());

    let link = warp::path("link")
        .and(warp::get())
        .and(client.clone())
        .and_then(create_link);

    let exchange = warp::path!("exchange" / String)
        .and(warp::get())
        .and(env_filter)
        .and(state_filter)
        .and(client)
        .and_then(exchange_token);

    let router = warp::get().and(link.or(exchange));

    warp::serve(router).run(([127, 0, 0, 1], 3030)).await;
}

pub(crate) async fn run(_matches: &ArgMatches, env: Environment) -> Result<()> {
    server(env).await;

    Ok(())
}
