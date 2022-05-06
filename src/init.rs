use crate::model::{Conf, ConfigFile, PlaidOpts};
use std::fs::OpenOptions;
use std::io::{stdin, stdout, Write};

use anyhow::{anyhow, Result};
use rplaid::client::Environment;

fn to_plaid_opts(id: &str, secret: &str, env: &str) -> Result<PlaidOpts> {
    if id.len() < 1 {
        return Err(anyhow!("Plaid client ID must not be empty"));
    }

    if secret.len() < 1 {
        return Err(anyhow!("Plaid client secret must not be empty"));
    }

    let e: Result<Environment, anyhow::Error> = match env.to_lowercase().as_str() {
        "sandbox" => Ok(Environment::Sandbox),
        "development" => Ok(Environment::Development),
        "production" => Ok(Environment::Production),
        _ => {
            return Err(anyhow!(
                "Plaid client environment must be one of SANDBOX, DEVELOPMENT, or PRODUCTION"
            ))
        }
    };

    Ok(PlaidOpts {
        secret: secret.to_string(),
        client_id: id.to_string(),
        env: e?,
    })
}

fn init_config(conf: ConfigFile) -> Result<()> {
    let mut buf = String::new();
    print!("Plaid Client ID: ");
    stdout().flush().unwrap();

    let stdin = stdin();
    stdin.read_line(&mut buf)?;

    print!("Plaid Client Secret: ");
    stdout().flush().unwrap();
    stdin.read_line(&mut buf)?;

    print!("Plaid Environment <SANDBOX | DEVELOPMENT | PRODUCTION>: ");
    stdout().flush().unwrap();
    stdin.read_line(&mut buf)?;

    let mut lines = buf.lines();
    let client_id = lines.next().expect("Plaid client ID must be provided");
    let client_secret = lines.next().expect("Plaid client secret must be provided");
    let client_environment = lines.next().expect("Plaid environment must be provided");

    let opts = to_plaid_opts(client_id, client_secret, client_environment)?;
    conf.update(&Conf {
        rules: vec![],
        plaid: opts,
    })?;
    Ok(())
}

pub(crate) async fn run(conf_path: Option<&str>) -> Result<()> {
    let path = match conf_path {
        Some(p) => p.into(),
        None => ConfigFile::default_config_path()?,
    };

    let fd = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&path)?;
    let conf = ConfigFile::read_from_file(fd, path)?;

    init_config(conf)?;

    Ok(())
}
