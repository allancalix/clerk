mod accounts;
mod link;
mod model;
mod rules;
mod txn;
mod init;
mod plaid;

#[macro_use]
extern crate ketos;
#[macro_use]
extern crate ketos_derive;

use anyhow::Result;
use clap::clap_app;

use crate::model::ConfigFile;

static CLIENT_NAME: &str = "ledgersync";
static PRODUCTS: [&str; 1] = ["transactions"];
static COUNTRY_CODES: [&str; 1] = ["US"];

async fn run() -> Result<()> {
    let matches = clap_app!(ledgersync =>
        (setting: clap::AppSettings::SubcommandRequired)
        (version: "0.1.0")
        (author: "Allan Calix <allan@acx.dev>")
        (about: "The ledgersync utility pulls data from an upstream source, such \
         as Plaid APIs, and generates Ledger records from the transactions.")
        (@arg CONFIG: -c --config [FILE] "Sets a custom config file")
        (@arg verbose: -v --verbose "Sets the level of verbosity")
        (@arg env: -e --env [String] "Selects the environment to run against.")
        (@subcommand init =>
            (about: "Initialize CLI for use.")
        )
        (@subcommand link =>
            (about: "Links a new account for tracking.")
            (@arg update: -u --update [ACCESS_TOKEN] "Update a link for an existing \
             account link, must pass the access token for the expired link.")
            (@subcommand status =>
                (about: "Displays all links and their current status known to
                 ledgersync.")
            )
        )
        (@subcommand accounts =>
            (about: "Prints tracked accounts to stdout.")
            (@subcommand balance =>
                (about: "Prints balances of all accounts. This command fetches
                 current data and may take some time to complete.")
            )
        )
        (@subcommand transactions =>
            (setting: clap::AppSettings::SubcommandRequired)
            (about: "pulls a set of transactions to the store")
            (@subcommand sync =>
                (about: "Pulls transactions from the given range, defaults to \
                 a weeks worth of transactions going back from today.")
                (@arg begin: --begin [DATE] "The first day of transactions to \
                 pull, defaults to a week before today. Start date is inclusive.")
                (@arg until: --until [DATE] "The last day of transactions to \
                 pull, defaults to today. End date is inclusive.")
            )
            (@subcommand print =>
                (about: "Prints all synced transactions as Ledger records.")
                (@arg begin: --begin [DATE] "The first day of Ledger records to \
                 generate.")
                (@arg until: --until [DATE] "The last day of Ledger records to \
                 generate.")
            )
        )
    )
    .get_matches();

    match matches.subcommand() {
        Some(("init", _)) => {
            init::run(matches.value_of("CONFIG")).await?;
        }
        Some(("link", link_matches)) => {
            let conf = ConfigFile::read(matches.value_of("CONFIG"))?;
            link::run(link_matches, conf).await?;
        }
        Some(("transactions", link_matches)) => {
            let conf = ConfigFile::read(matches.value_of("CONFIG"))?;
            txn::run(link_matches, conf).await?;
        }
        Some(("accounts", link_matches)) => {
            let conf = ConfigFile::read(matches.value_of("CONFIG"))?;
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

