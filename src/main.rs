mod model;

use std::collections::BTreeMap;
use std::io::prelude::*;
use std::io::SeekFrom;

use clap::clap_app;
use futures_util::pin_mut;
use futures_util::StreamExt;
use model::*;
use rplaid::*;
use warp::Filter;

const PRODUCTS: [&str; 5] = [
    "assets",
    "auth",
    "balance",
    "credit_details",
    "transactions",
];

fn credentials() -> Credentials {
    Credentials {
        client_id: std::env::var("PLAID_CLIENT_ID")
            .expect("Variable PLAID_CLIENT_ID must be defined."),
        secret: std::env::var("PLAID_SECRET").expect("Variable PLAID_SECRET must be defined."),
    }
}

pub async fn create_link(
    client: std::sync::Arc<Plaid<impl HttpClient>>,
) -> Result<impl warp::Reply, std::convert::Infallible> {
    let res = client
        .create_link_token(&CreateLinkTokenRequest {
            client_name: "test_client",
            user: LinkUser::new("test-user"),
            language: "en",
            country_codes: &["US"],
            products: &PRODUCTS,
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
    state: std::sync::Arc<std::sync::Mutex<Config>>,
    client: std::sync::Arc<Plaid<impl HttpClient>>,
) -> Result<impl warp::Reply, std::convert::Infallible> {
    let res = client.exchange_public_token(public_token).await.unwrap();
    state.lock().unwrap().add_link(Link {
        access_token: res.access_token,
        item_id: res.item_id,
        state: "NEW".into(),
        env: rplaid::Environment::Sandbox,
    });
    Ok(warp::reply::html("OK"))
}

async fn server(env: rplaid::Environment) {
    let state = std::sync::Arc::new(std::sync::Mutex::new(Config::from_path("state.json")));
    let plaid = std::sync::Arc::new(
        rplaid::PlaidBuilder::new()
            .with_credentials(credentials())
            .with_env(env)
            .build(),
    );
    let client = warp::any().map(move || plaid.clone());
    let state_filter = warp::any().map(move || state.clone());

    let link = warp::path("link")
        .and(warp::get())
        .and(client.clone())
        .and_then(create_link);

    let exchange = warp::path!("exchange" / String)
        .and(warp::get())
        .and(state_filter)
        .and(client)
        .and_then(exchange_token);

    let router = warp::get().and(link.or(exchange));

    warp::serve(router).run(([127, 0, 0, 1], 3030)).await;
}
//
async fn pull_transactions(start: &str, end: &str, env: rplaid::Environment) {
    let state = Config::from_path("state.json");
    let plaid = rplaid::PlaidBuilder::new()
        .with_credentials(credentials())
        .with_env(env.clone())
        .build();
    let links: Vec<model::Link> = state
        .links()
        .into_iter()
        .filter(|link| &link.env == &env)
        .collect();

    let mut fd = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open("transactions.json")
        .unwrap();
    let mut content = String::new();
    fd.read_to_string(&mut content).unwrap();
    let known_txns: Vec<Transaction> = serde_json::from_str(&content).unwrap_or_default();

    let mut results = known_txns
        .into_iter()
        .fold(BTreeMap::new(), |mut acc, txn| {
            acc.insert((txn.date.clone(), txn.transaction_id.clone()), txn);
            acc
        });
    for link in links {
        let txns = plaid.transactions_iter(rplaid::GetTransactionsRequest {
            access_token: link.access_token.as_str(),
            start_date: start,
            end_date: end,
            options: Some(GetTransactionsOptions {
                count: Some(100),
                offset: Some(0),
                account_ids: None,
                include_original_description: None,
            }),
        });
        pin_mut!(txns);

        while let Some(txn_page) = txns.next().await {
            for txn in txn_page.unwrap() {
                results.insert((txn.date.clone(), txn.transaction_id.clone()), txn);
            }
        }
    }

    let output = results
        .into_iter()
        .map(|(_, v)| v)
        .collect::<Vec<Transaction>>();
    let json = serde_json::to_string_pretty(&output).unwrap();
    fd.seek(SeekFrom::Start(0)).unwrap();
    write!(fd, "{}", json).unwrap();
}

async fn print_accounts(env: rplaid::Environment) {
    let state = Config::from_path("state.json");
    let plaid = std::sync::Arc::new(
        rplaid::PlaidBuilder::new()
            .with_credentials(credentials())
            .with_env(env.clone())
            .build(),
    );

    let links: Vec<model::Link> = state
        .links()
        .into_iter()
        .filter(|link| &link.env == &env)
        .collect();

    let mut tw = tabwriter::TabWriter::new(vec![]);
    write!(tw, "Institution\tAccount\tType\tStatus\n").unwrap();
    for link in links {
        let out = plaid.item(link.access_token.clone()).await.unwrap();
        let ins
            = plaid.get_institution_by_id(rplaid::InstitutionGetRequest{
                institution_id: out.institution_id.unwrap(),
                country_codes: vec!["US".into()],
            }).await.unwrap();

        let accounts = plaid.accounts(link.access_token).await.unwrap();
        for account in accounts {
            write!(
                tw,
                "{}\t{}\t{}\t{:?}\n",
                ins.name, account.name, account.r#type, out.consent_expiration_time
            )
            .unwrap();
        }
    }

    let table = String::from_utf8(tw.into_inner().unwrap()).unwrap();
    println!("{}", table);
}

#[tokio::main]
async fn main() {
    let matches = clap_app!(ledgersync =>
        (setting: clap::AppSettings::SubcommandRequired)
        (version: "1.0")
        (author: "Allan Calix <allan@acx.dev>")
        (about: "TODO: Write a nice description.")
        (@arg CONFIG: -c --config [FILE] "Sets a custom config file")
        (@arg verbose: -v --verbose "Sets the level of verbosity")
        (@arg env: -e --env [String] "Selects the environment to run against.")
        (@subcommand link =>
            (about: "links a new account for tracking")
            (version: "1.0")
        )
        (@subcommand accounts =>
            (about: "TODO")
            (version: "1.0")
        )
        (@subcommand pull =>
            (about: "pulls a set of transactions to the store")
            (version: "1.0")
            (@arg begin: --begin [DATE] "Sets a custom config file")
            (@arg until: --until [DATE] "Sets the level of verbosity")
        )
    )
    .get_matches();

    let env = matches.value_of("env").map_or_else(
        || rplaid::Environment::Sandbox,
        |e| match e {
            "Production" => rplaid::Environment::Production,
            "Development" => rplaid::Environment::Development,
            "Sandbox" => rplaid::Environment::Sandbox,
            _ => {
                println!("Environment {} not recognized.", e);
                std::process::exit(1);
            }
        },
    );

    match matches.subcommand() {
        Some(("link", _link_matches)) => {
            server(env).await;
        }
        Some(("pull", link_matches)) => {
            let start = link_matches
                .value_of("begin")
                .unwrap_or_else(|| "2021-07-01");
            let end = link_matches
                .value_of("until")
                .unwrap_or_else(|| "2021-09-06");
            pull_transactions(start, end, env).await;
        }
        Some(("accounts", _link_matches)) => {
            print_accounts(env).await;
        }
        None => unreachable!("subcommand is required"),
        _ => unreachable!(),
    }
}
