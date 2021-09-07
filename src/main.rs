mod accounts;
mod link;
mod model;
mod rules;
mod txn;

extern crate pest;
#[macro_use]
extern crate pest_derive;

use clap::clap_app;

static CLIENT_NAME: &str = "ledgersync";
static PRODUCTS: [&str; 1] = ["transactions"];
static COUNTRY_CODES: [&str; 1] = ["US"];

#[tokio::main]
async fn main() {
    let matches = clap_app!(ledgersync =>
        (setting: clap::AppSettings::SubcommandRequired)
        (version: "0.1.0")
        (author: "Allan Calix <allan@acx.dev>")
        (about: "The ledgersync utility pulls data from an upstream source, such \
         as Plaid APIs, and generates Ledger records from the transactions.")
        (@arg CONFIG: -c --config [FILE] "Sets a custom config file")
        (@arg verbose: -v --verbose "Sets the level of verbosity")
        (@arg env: -e --env [String] "Selects the environment to run against.")
        (@subcommand link =>
            (about: "Links a new account for tracking.")
        )
        (@subcommand accounts =>
            (about: "Prints tracked accounts to stdout.")
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
                (version: "1.0")
            )
        )
    )
    .get_matches();

    let conf = crate::model::Conf::read(matches.value_of("CONFIG")).unwrap();

    match matches.subcommand() {
        Some(("link", link_matches)) => {
            link::run(link_matches, conf).await.unwrap();
        }
        Some(("transactions", link_matches)) => {
            txn::run(link_matches, conf).await.unwrap();
        }
        Some(("accounts", link_matches)) => {
            accounts::run(link_matches, conf).await.unwrap();
        }
        None => unreachable!("subcommand is required"),
        _ => unreachable!(),
    }
}
