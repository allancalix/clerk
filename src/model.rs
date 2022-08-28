use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::PathBuf;

use anyhow::Result;
use rplaid::client::Environment;

use crate::CLIENT_NAME;

const CONFIG_NAME: &str = "config.toml";

#[derive(Debug, Default, Clone)]
pub(crate) struct ConfigFile {
    path: PathBuf,
    conf: Conf,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub(crate) struct Conf {
    pub(crate) plaid: PlaidOpts,
    pub(crate) db_file: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub(crate) struct PlaidOpts {
    pub(crate) client_id: String,
    pub(crate) secret: String,
    pub(crate) env: Environment,
}

impl ConfigFile {
    pub(crate) fn new(file_path: std::path::PathBuf) -> Self {
        Self {
            path: file_path,
            conf: Conf {
                plaid: PlaidOpts {
                    client_id: String::new(),
                    secret: String::new(),
                    env: Environment::Sandbox,
                },
                db_file: None,
            },
        }
    }

    pub(crate) fn default_config_path() -> Result<std::path::PathBuf> {
        Ok(dirs::config_dir()
            .unwrap_or(std::env::current_dir()?)
            .join(CLIENT_NAME)
            .join(CONFIG_NAME))
    }

    pub(crate) fn read(path: Option<&str>) -> Result<Self> {
        let p = match path {
            Some(p) => p.into(),
            None => ConfigFile::default_config_path()?,
        };

        let mut fd = OpenOptions::new()
            .read(true)
            .open(&p)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => {
                    let ctx = format!("no configuration file found at: {:?}", p);
                    anyhow::Error::new(e).context(ctx)
                }
                _ => {
                    let ctx = format!("Failed to read configuration {}: {}.", p.display(), e);
                    anyhow::Error::new(e).context(ctx)
                }
            })?;
        let mut content = String::new();
        fd.read_to_string(&mut content)?;

        let config: Conf = toml::from_str(&content)?;
        Ok(ConfigFile {
            path: p,
            conf: config,
        })
    }

    pub(crate) fn read_from_file(mut fd: std::fs::File, path: PathBuf) -> Result<Self> {
        let mut content = String::new();
        fd.read_to_string(&mut content)?;

        let config: Conf = toml::from_str(&content)?;
        Ok(ConfigFile { path, conf: config })
    }

    pub(crate) fn update(&self, conf: &Conf) -> Result<()> {
        let mut fd = OpenOptions::new().write(true).open(&self.path)?;

        let contents = toml::to_string_pretty(conf)?;

        // Overwrite existing file contents.
        fd.set_len(0)?;
        write!(fd, "{}", contents)?;

        Ok(())
    }

    pub(crate) fn config(&self) -> &Conf {
        &self.conf
    }

    pub(crate) fn data_path(&self) -> String {
        self.conf
            .db_file
            .as_ref()
            .cloned()
            .unwrap_or_else(default_data_path)
    }

    pub(crate) fn valid(&self) -> bool {
        !self.conf.plaid.client_id.is_empty() && !self.conf.plaid.secret.is_empty()
    }
}

fn default_data_path() -> String {
    dirs::data_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir()))
        .join(CLIENT_NAME)
        .join(format!("{}.db", CLIENT_NAME))
        .display()
        .to_string()
}
