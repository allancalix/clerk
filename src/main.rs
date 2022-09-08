mod accounts;
mod core;
mod link;
mod plaid;
mod settings;
mod store;
mod txn;
mod upstream;

use anyhow::Result;
use clap::{arg, Command};
use tracing_subscriber::{
    filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter,
};

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
        .arg(arg!(verbose: -d --debug ... "Outputs debug logging information."))
        .subcommand(Command::new("init").about("Initialize CLI for use."))
        .subcommand(Command::new("link")
            .about("Links a new account for tracking.")
            .arg(arg!(name: -n --name [ALIAS] "An alias to easily identify what accounts the link belongs to."))
            .arg(arg!(update: -u --update [ITEM_ID] "Update a link for an existing account link, must pass the access token for the expired link."))
            .arg(arg!(env: -e --env [String] "Selects the environment to run against."))
            .subcommand(Command::new("status").about("Displays all links and their current status."))
            .subcommand(Command::new("delete")
                .about("Deletes a Plaid account link.")
                .arg(arg!(item_id: <ITEM_ID> "The item ID of the link to delete."))))
        .subcommand(Command::new("account")
            .about("Prints tracked accounts to stdout.")
            .subcommand(Command::new("balances")
                .about("Prints balances of all accounts. This command fetches current data and may take some time to complete.")))
        .subcommand(Command::new("txn")
            .subcommand_required(true)
            .about("pulls a set of transactions to the store")
            .subcommand(Command::new("sync")
                .about("Pulls transactions from the given range, defaults to a weeks worth of transactions going back from today.")));

    let matches = app.get_matches();
    if matches.is_present("verbose") {
        tracing_subscriber::registry()
            .with(
                EnvFilter::builder()
                    .with_default_directive(LevelFilter::INFO.into())
                    .from_env_lossy(),
            )
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    let s = settings::Settings::new(matches.value_of("CONFIG"))?;
    match matches.subcommand() {
        Some(("link", link_matches)) => {
            link::run(link_matches, s).await?;
        }
        Some(("txn", link_matches)) => {
            txn::run(link_matches, s).await?;
        }
        Some(("account", link_matches)) => {
            accounts::run(link_matches, s).await?;
        }
        None => unreachable!("subcommand is required"),
        _ => unreachable!(),
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("Exited abnormally: {}", err);
        std::process::exit(1);
    }
}
