use std::io::prelude::*;

use anyhow::Result;
use clap::ArgMatches;
use rplaid::Environment;
use tabwriter::TabWriter;

use crate::model::{Config, Link};

async fn print(env: Environment) -> Result<()> {
    let state = Config::from_path("state.json");
    let plaid = std::sync::Arc::new(
        rplaid::PlaidBuilder::new()
            .with_credentials(crate::credentials())
            .with_env(env.clone())
            .build(),
    );

    let links: Vec<Link> = state
        .links()
        .into_iter()
        .filter(|link| &link.env == &env)
        .collect();

    let mut tw = TabWriter::new(vec![]);
    write!(tw, "Institution\tAccount\tType\tStatus\n")?;
    for link in links {
        let out = plaid.item(link.access_token.clone()).await?;
        let ins = plaid
            .get_institution_by_id(rplaid::InstitutionGetRequest {
                institution_id: out.institution_id.unwrap(),
                country_codes: vec!["US".into()],
            })
            .await?;

        let accounts = plaid.accounts(link.access_token).await?;
        for account in accounts {
            write!(
                tw,
                "{}\t{}\t{}\t{:?}\n",
                ins.name, account.name, account.r#type, out.consent_expiration_time
            )?;
        }
    }

    let table = String::from_utf8(tw.into_inner()?)?;
    println!("{}", table);

    Ok(())
}

pub(crate) async fn run(_matches: &ArgMatches, env: Environment) -> Result<()> {
    print(env).await?;
    Ok(())
}
