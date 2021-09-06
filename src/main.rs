mod accounts;
mod link;
mod model;
mod txn;

use clap::clap_app;
use rplaid::Credentials;

const PRODUCTS: [&str; 5] = [
    "assets",
    "auth",
    "balance",
    "credit_details",
    "transactions",
];

pub(crate) fn credentials() -> Credentials {
    Credentials {
        client_id: std::env::var("PLAID_CLIENT_ID")
            .expect("Variable PLAID_CLIENT_ID must be defined."),
        secret: std::env::var("PLAID_SECRET").expect("Variable PLAID_SECRET must be defined."),
    }
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
        Some(("link", link_matches)) => {
            link::run(link_matches, env).await.unwrap();
        }
        Some(("transactions", link_matches)) => {
            txn::run(link_matches, env).await.unwrap();
        }
        Some(("accounts", link_matches)) => {
            accounts::run(link_matches, env).await.unwrap();
        }
        None => unreachable!("subcommand is required"),
        _ => unreachable!(),
    }
}
