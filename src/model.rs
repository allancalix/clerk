use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::io::SeekFrom;
use std::path::PathBuf;

use anyhow::Result;
use rplaid::client::Environment;
use rplaid::model::*;

use crate::CLIENT_NAME;

const CONFIG_NAME: &str = "config.toml";
const DATA_FILE_NAME: &str = "state.json";
const DEFAULT_TRANSFORM_FILE_NAME: &str = "transform.keto";

#[derive(Debug, Default, Clone)]
pub(crate) struct ConfigFile {
    path: PathBuf,
    conf: Conf,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub(crate) struct Conf {
    pub(crate) rules: Vec<String>,
    pub(crate) plaid: PlaidOpts,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub(crate) struct PlaidOpts {
    pub(crate) client_id: String,
    pub(crate) secret: String,
    pub(crate) env: Environment,
}

impl ConfigFile {
    pub(crate) fn read(path: Option<&str>) -> Result<Self> {
        let p = match path {
            Some(p) => p.into(),
            None => dirs::config_dir()
                .unwrap_or(std::env::current_dir()?)
                .join(CLIENT_NAME)
                .join(CONFIG_NAME),
        };

        let mut fd = OpenOptions::new().read(true).open(&p).map_err(|e| {
            let context = format!("Failed to read configuration {}: {}.", p.display(), e);
            anyhow::Error::new(e).context(context)
        })?;
        let mut content = String::new();
        fd.read_to_string(&mut content)?;

        let mut config: Conf = toml::from_str(&content)?;
        if config.rules.is_empty() {
            config.rules.push(DEFAULT_TRANSFORM_FILE_NAME.into());
        }

        Ok(ConfigFile {
            path: p,
            conf: config,
        })
    }

    pub(crate) fn config(&self) -> &Conf {
        &self.conf
    }

    pub(crate) fn rules(&self) -> Vec<PathBuf> {
        let mut rules = vec![];
        let mut root = self.path.clone();
        root.pop();

        for rule in &self.config().rules {
            let mut path = PathBuf::from(rule);
            if path.is_relative() {
                path = PathBuf::from(root.clone()).join(rule)
            };

            rules.push(path);
        }

        rules
    }
}

pub(crate) struct AppData {
    handle: std::fs::File,
    store: AppStorage,
}

impl AppData {
    pub(crate) fn new() -> Result<AppData> {
        let data_path = dirs::data_dir()
            .unwrap_or(std::env::current_dir()?)
            .join(CLIENT_NAME)
            .join(DATA_FILE_NAME);
        let mut fd = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(data_path)?;
        let mut content = String::new();
        fd.read_to_string(&mut content)?;
        let store: AppStorage = serde_json::from_str(&content).unwrap_or_default();

        Ok(Self { store, handle: fd })
    }

    pub(crate) fn set_txns(&mut self, txns: Vec<Transaction>) -> Result<()> {
        self.store.transactions = txns;
        let json = serde_json::to_string_pretty(&self.store)?;
        self.handle.seek(SeekFrom::Start(0))?;
        write!(self.handle, "{}", json)?;
        self.handle.flush()?;

        Ok(())
    }

    pub(crate) fn links(&self) -> Vec<Link> {
        self.store.links.clone()
    }

    pub(crate) fn add_link(&mut self, link: Link) -> Result<()> {
        self.store.links.push(link);
        self.handle.seek(SeekFrom::Start(0))?;
        write!(
            self.handle,
            "{}",
            serde_json::to_string_pretty(&self.store)?
        )?;
        self.handle.flush()?;

        Ok(())
    }

    pub(crate) fn transactions(&self) -> &Vec<Transaction> {
        &self.store.transactions
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct AppStorage {
    links: Vec<Link>,
    transactions: Vec<Transaction>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct Link {
    pub(crate) access_token: String,
    pub(crate) item_id: String,
    pub(crate) state: LinkStatus,
    pub(crate) env: Environment,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum LinkStatus {
    New,
}
