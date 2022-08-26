#![feature(result_contains_err)]
#[allow(clippy::derive_hash_xor_eq)]
mod accounts;
mod init;
mod link;
mod model;
mod plaid;
mod rules;
mod store;
mod txn;
mod upstream;

#[macro_use]
extern crate ketos;
#[macro_use]
extern crate ketos_derive;

use anyhow::Result;
use clap::{arg, Command};
use tracing_subscriber::{
    filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter,
};
use tracing_tree::HierarchicalLayer;

use crate::model::ConfigFile;

static CLIENT_NAME: &str = "clerk";
static COUNTRY_CODES: [&str; 1] = ["US"];

async fn run() -> Result<()> {
    let app = Command::new(CLIENT_NAME)
        .about("The clerk utility pulls data from an upstream source, such \
         as Plaid APIs, and generates Ledger records from the transactions.")
        .version("0.1.0")
        .author("Allan Calix <allan@acx.dev>")
        .subcommand_required(true)
        .allow_external_subcommands(false)
        .arg(arg!(CONFIG: -c --config [FILE] "Sets a custom config file"))
        .arg(arg!(verbose: -v --verbose [Boolean] "Sets the level of verbosity"))
        .arg(arg!(env: -e --env [String] "Selects the environment to run against."))
        .subcommand(Command::new("init").about("Initialize CLI for use."))
        .subcommand(Command::new("link")
            .about("Links a new account for tracking.")
            .arg(arg!(name: -n --name [ALIAS] "An alias to easily identify what accounts the link belongs to."))
            .arg(arg!(update: -u --update [ITEM_ID] "Update a link for an existing account link, must pass the access token for the expired link."))
            .subcommand(Command::new("status").about("Displays all links and their current status."))
            .subcommand(Command::new("delete")
                .about("Deletes a Plaid account link.")
                .arg(arg!(item_id: <ITEM_ID> "The item ID of the link to delete."))))
        .subcommand(Command::new("accounts")
            .about("Prints tracked accounts to stdout.")
            .subcommand(Command::new("balance")
                .about("Prints balances of all accounts. This command fetches current data and may take some time to complete.")))
        .subcommand(Command::new("transactions")
            .subcommand_required(true)
            .about("pulls a set of transactions to the store")
            .subcommand(Command::new("sync")
                .about("Pulls transactions from the given range, defaults to a weeks worth of transactions going back from today.")
                .arg(arg!(begin: --begin [DATE] "The first day of transactions to pull, defaults to a week before today. Start date is inclusive."))
                .arg(arg!(until: --until [DATE] "The last day of transactions to pull, defaults to today. End date is inclusive.")))
            .subcommand(Command::new("print")
                .about("Prints all synced transactions as Ledger records.")
                .arg(arg!(begin: --begin [DATE] "The first day of Ledger records to generate."))
                .arg(arg!(until: --until [DATE] "The last day of Ledger records to generate."))));

    if app.clone().get_matches().value_of("verbose") == Some("true") {
        tracing_subscriber::registry()
            .with(
                EnvFilter::builder()
                    .with_default_directive(LevelFilter::INFO.into())
                    .from_env_lossy(),
            )
            .with(tracing_subscriber::fmt::layer())
            .with(
                HierarchicalLayer::new(2)
                    .with_targets(true)
                    .with_bracketed_fields(true),
            )
            .init();
    }

    match app.clone().get_matches().subcommand() {
        Some(("init", _link_matches)) => {
            init::run(app.get_matches().value_of("CONFIG")).await?;
        }
        Some(("link", link_matches)) => {
            let conf = ConfigFile::read(app.get_matches().value_of("CONFIG"))?;
            link::run(link_matches, conf).await?;
        }
        Some(("transactions", link_matches)) => {
            let conf = ConfigFile::read(app.get_matches().value_of("CONFIG"))?;
            txn::run(link_matches, conf).await?;
        }
        Some(("accounts", link_matches)) => {
            let conf = ConfigFile::read(app.get_matches().value_of("CONFIG"))?;
            accounts::run(link_matches, conf).await?;
        }
        None => unreachable!("subcommand is required"),
        _ => unreachable!(),
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        println!("{}", err);
        std::process::exit(1);
    }
}
