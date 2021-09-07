use std::io::prelude::*;

use anyhow::Result;
use clap::ArgMatches;
use rplaid::{Credentials, PlaidBuilder};
use tabwriter::TabWriter;

use crate::model::{Conf, Config, Link};
use crate::COUNTRY_CODES;

async fn print(conf: Conf) -> Result<()> {
    let state = Config::new();
    let plaid = PlaidBuilder::new()
        .with_credentials(Credentials {
            client_id: conf.plaid.client_id.clone(),
            secret: conf.plaid.secret.clone(),
        })
        .with_env(conf.plaid.env.clone())
        .build();

    let links: Vec<Link> = state
        .links()
        .into_iter()
        .filter(|link| link.env == conf.plaid.env)
        .collect();

    let mut tw = TabWriter::new(vec![]);
    writeln!(tw, "Institution\tAccount\tAccount ID\tType\tStatus")?;
    for link in links {
        let out = plaid.item(link.access_token.clone()).await?;
        let ins = plaid
            .get_institution_by_id(&rplaid::InstitutionGetRequest {
                institution_id: out.institution_id.unwrap().as_str(),
                country_codes: &COUNTRY_CODES,
            })
            .await?;

        let accounts = plaid.accounts(link.access_token).await?;
        for account in accounts {
            writeln!(
                tw,
                "{}\t{}\t{}\t{}\t{:?}",
                ins.name,
                account.name,
                account.account_id,
                account.r#type,
                out.consent_expiration_time
            )?;
        }
    }

    let table = String::from_utf8(tw.into_inner()?)?;
    println!("{}", table);

    Ok(())
}

pub(crate) async fn run(_matches: &ArgMatches, conf: Conf) -> Result<()> {
    print(conf).await?;
    Ok(())
}
